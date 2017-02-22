//! This module implements the in-memory database
//!
//! A support folder with SDK debug symbols can be processed into a
//! in-memory database format which is a flat file on the file system
//! that gets mmaped into the process.
use std::io::{Read, Write, Seek, SeekFrom, Stdout, ErrorKind as IoErrorKind};
use std::fs::File;
use std::mem;
use std::slice;
use std::path::Path;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use uuid::Uuid;
use pbr::ProgressBar;
use xz2::write::XzEncoder;
use tempfile::tempfile;

use super::Result;
use super::sdk::{SdkInfo, DumpOptions, Objects};
use super::dsym::{Object, Variant};
use super::memdbtypes::{IndexItem, StoredSlice, MemDbHeader, IndexedUuid};


pub struct MemDbBuilder<W> {
    writer: RefCell<W>,
    tempfile: Option<RefCell<File>>,
    info: SdkInfo,
    symbols: Vec<String>,
    symbols_map: HashMap<String, u32>,
    object_names: Vec<String>,
    object_names_map: HashMap<String, u16>,
    object_uuid_mapping: Vec<(String, Uuid)>,
    variant_uuids: Vec<IndexedUuid>,
    variants: Vec<Vec<IndexItem>>,
    state: RefCell<DumpState>,
    options: DumpOptions,
}

pub struct DumpState {
    pb: Option<ProgressBar<Stdout>>,
}

fn make_progress_bar(count: usize) -> ProgressBar<Stdout> {
    let mut pb = ProgressBar::new(count as u64);
    pb.tick_format("⠇⠋⠙⠸⠴⠦");
    pb.set_max_refresh_rate(Some(Duration::from_millis(16)));
    pb.format("[■□□]");
    pb.show_tick = true;
    pb.show_speed = false;
    pb.show_percent = false;
    pb.show_counter = false;
    pb.show_time_left = false;
    pb
}

trait WriteSeek : Write + Seek {}
impl<T: Write+Seek> WriteSeek for T {}

impl<W: Write + Seek> MemDbBuilder<W> {

    pub fn new(writer: W, info: &SdkInfo, opts: DumpOptions, object_count: usize)
        -> Result<MemDbBuilder<W>>
    {
        let rv = MemDbBuilder {
            writer: RefCell::new(writer),
            tempfile: if opts.compress {
                Some(RefCell::new(tempfile()?))
            } else {
                None
            },
            info: info.clone(),
            symbols: vec![],
            symbols_map: HashMap::new(),
            object_names: vec![],
            object_names_map: HashMap::new(),
            object_uuid_mapping: vec![],
            variant_uuids: vec![],
            variants: vec![],
            state: RefCell::new(DumpState {
                pb: if opts.show_progress_bar {
                    Some(make_progress_bar(object_count))
                } else {
                    None
                },
            }),
            options: opts,
        };
        let header = MemDbHeader { ..Default::default() };
        rv.write(&header)?;
        Ok(rv)
    }

    fn with_file<T, F: FnOnce(&mut WriteSeek) -> T>(&self, f: F) -> T {
        if self.options.compress {
            f(&mut *self.tempfile.as_ref().unwrap().borrow_mut() as &mut WriteSeek)
        } else {
            f(&mut *self.writer.borrow_mut() as &mut WriteSeek)
        }
    }

    fn write_bytes(&self, x: &[u8]) -> Result<usize> {
        self.with_file(|mut w| {
            w.write_all(x)?;
            Ok(x.len())
        })
    }

    fn write<T>(&self, x: &T) -> Result<usize> {
        unsafe {
            let bytes : *const u8 = mem::transmute(x);
            let size = mem::size_of_val(x);
            self.with_file(|mut w| {
                w.write_all(slice::from_raw_parts(bytes, size))?;
                Ok(size)
            })
        }
    }

    fn seek(&self, new_pos: usize) -> Result<()> {
        self.with_file(|mut w| {
            w.seek(SeekFrom::Start(new_pos as u64))?;
            Ok(())
        })
    }

    fn tell(&self) -> Result<usize> {
        self.with_file(|mut w| {
            Ok(w.seek(SeekFrom::Current(0))? as usize)
        })
    }

    fn inc_progress_bar(&self, step: usize) {
        if let Some(ref mut pb) = self.state.borrow_mut().pb {
            pb.add(step as u64);
        }
    }

    fn set_progress_message(&self, msg: &str) {
        if let Some(ref mut pb) = self.state.borrow_mut().pb {
            pb.message(&format!("  {: <40}", msg));
        }
    }

    fn tick_progress_bar(&self, idx: usize) {
        if idx % 100 == 0 {
            if let Some(ref mut pb) = self.state.borrow_mut().pb {
                pb.tick();
            }
        }
    }

    fn progress_bar_finish(&self) {
        if let Some(ref mut pb) = self.state.borrow_mut().pb {
            pb.finish();
            println!("");
        }
    }

    fn new_progress_bar(&self, count: usize) {
        if !self.options.show_progress_bar {
            return;
        }
        self.progress_bar_finish();
        self.state.borrow_mut().pb = Some(make_progress_bar(count));
    }

    fn add_symbol(&mut self, sym: &str) -> usize {
        if let Some(&sym_id) = self.symbols_map.get(sym) {
            return sym_id as usize;
        }
        let symbol_count = self.symbols.len();
        self.symbols.push(sym.to_string());
        self.symbols_map.insert(sym.to_string(), symbol_count as u32);
        symbol_count
    }

    fn add_object_name(&mut self, src: &str) -> usize {
        if let Some(&src_id) = self.object_names_map.get(src) {
            return src_id as usize;
        }
        let object_count = self.object_names.len();
        self.object_names.push(src.to_string());
        self.object_names_map.insert(src.to_string(), object_count as u16);
        object_count
    }

    pub fn write_object(&mut self, obj: &Object, filename: Option<&str>,
                        offset: usize) -> Result<()> {
        self.inc_progress_bar(offset);
        for variant in obj.variants() {
            let src = variant.name().or(filename).unwrap();
            if let Some(fname) = Path::new(src).file_name().and_then(|x| x.to_str()) {
                self.set_progress_message(fname);
            }
            self.write_object_variant(&obj, &variant, src)?;
        }
        Ok(())
    }

    pub fn write_object_variant(&mut self, obj: &Object, var: &Variant,
                                src: &str) -> Result<()> {
        let mut symbols = obj.symbols(var.arch())?;
        let src_id = self.add_object_name(src);

        // build symbol index
        let mut index = vec![];
        for (idx, (addr, sym)) in symbols.iter().enumerate() {
            let sym_id = self.add_symbol(sym);
            index.push(IndexItem::new(addr, src_id, sym_id));
            self.tick_progress_bar(idx);
        }
        index.sort_by_key(|item| item.addr());

        // register variant and uuid
        self.variant_uuids.push(IndexedUuid::new(&var.uuid().unwrap(), self.variants.len()));
        self.object_uuid_mapping.push((
            format!("{}:{}", src, var.arch()),
            var.uuid().unwrap()
        ));
        self.variants.push(index);

        Ok(())
    }

    fn make_string_slices(&self, strings: &[String], _try_compress: bool) -> Result<Vec<StoredSlice>> {
        let mut slices = vec![];
        for (idx, string) in strings.iter().enumerate() {
            let offset = self.tell()?;
            let len = self.write_bytes(string.as_bytes())?;
            slices.push(StoredSlice::new(offset, len, false));
            self.tick_progress_bar(idx);
        }
        Ok(slices)
    }

    fn write_slices(&self, slices: &[StoredSlice], start: &mut u32, len: &mut u32) -> Result<()> {
        *start = self.tell()? as u32;
        for (idx, item) in slices.iter().enumerate() {
            self.write(item)?;
            self.tick_progress_bar(idx);
        }
        *len = slices.len() as u32;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.set_progress_message("Done processing");
        self.new_progress_bar(if self.options.compress {
            6
        } else {
            5
        });

        let mut header = MemDbHeader { ..Default::default() };
        header.version = 1;
        header.sdk_info.set_from_sdk_info(&self.info);

        self.inc_progress_bar(1);
        self.set_progress_message("Writing variants");

        // start by writing out the index of the variants and record the slices.
        let mut slices = vec![];
        let mut ii_idx = 0;
        for variant in self.variants.iter() {
            let offset = self.tell()?;
            for index_item in variant {
                self.write(index_item)?;
                ii_idx += 1;
                self.tick_progress_bar(ii_idx);
            }
            slices.push(StoredSlice::new(offset, (self.tell()? - offset), false));
        }
        self.write_slices(&slices[..], &mut header.variants_start,
                          &mut header.variants_count)?;

        // next write out the UUIDs.  Since these are fixed length we do not
        // need to use slices here.
        header.uuids_start = self.tell()? as u32;
        header.uuids_count = self.variant_uuids.len() as u32;
        self.variant_uuids.sort_by_key(|x| x.uuid);
        for (idx, indexed_uuid) in self.variant_uuids.iter().enumerate() {
            self.write(indexed_uuid)?;
            self.tick_progress_bar(idx);
        }

        self.inc_progress_bar(1);
        self.set_progress_message("Writing variant mapping");

        // next we write out the name + arch -> uuid index mapping.  We also sort
        // this by uuid so that the index matches up.
        header.tagged_object_names_start = self.tell()? as u32;
        self.object_uuid_mapping.sort_by_key(|&(_, b)| b);
        for (idx, &(ref tagged_object, _)) in self.object_uuid_mapping.iter().enumerate() {
            self.write_bytes(format!("{}\x00", tagged_object).as_bytes())?;
            self.tick_progress_bar(idx);
        }
        header.tagged_object_names_end = self.tell()? as u32;

        self.inc_progress_bar(1);
        self.set_progress_message("Writing sources");

        // now write out all the object name sources
        let slices = self.make_string_slices(&self.object_names[..], true)?;
        self.write_slices(&slices[..], &mut header.object_names_start,
                          &mut header.object_names_count)?;

        self.inc_progress_bar(1);
        self.set_progress_message("Writing symbols");

        // now write out all the symbols
        let slices = self.make_string_slices(&self.symbols[..], true)?;
        self.write_slices(&slices[..], &mut header.symbols_start,
                          &mut header.symbols_count)?;

        self.inc_progress_bar(1);
        self.set_progress_message("Writing headers");

        // write the updated header
        self.seek(0)?;
        self.write(&header)?;

        // compress if necessary
        if self.options.compress {
            self.inc_progress_bar(1);
            self.set_progress_message("Compressing file");
            self.seek(0)?;
            let mut reader = self.tempfile.as_ref().unwrap().borrow_mut();
            let mut writer = self.writer.borrow_mut();
            let mut buf = [0; 32768];
            let mut idx = 0;
            loop {
                idx += 1;
                self.tick_progress_bar(idx);
                let len = match reader.read(&mut buf) {
                    Ok(0) => { break },
                    Ok(len) => len,
                    Err(ref e) if e.kind() == IoErrorKind::Interrupted => continue,
                    Err(e) => return Err(e.into()),
                };
                writer.write_all(&buf[..len])?;
            }
        }

        // done!
        self.set_progress_message("Done writing");
        self.progress_bar_finish();

        Ok(())
    }
}

pub fn dump_memdb<W: Write + Seek>(writer: W, info: &SdkInfo,
                                   opts: DumpOptions, objects: Objects)
    -> Result<()>
{
    let mut builder = MemDbBuilder::new(writer, info, opts, objects.file_count())?;
    for obj_res in objects {
        let (offset, filename, obj) = obj_res?;
        builder.write_object(&obj, Some(&filename), offset)?;
    }
    builder.flush()?;
    Ok(())
}
