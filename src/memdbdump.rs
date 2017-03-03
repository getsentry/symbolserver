//! This module implements the in-memory database
//!
//! A support folder with SDK debug symbols can be processed into a
//! in-memory database format which is a flat file on the file system
//! that gets mmaped into the process.
use std::io::{Write, Seek, SeekFrom};
use std::fs::File;
use std::mem;
use std::slice;
use std::path::Path;
use std::cell::RefCell;
use std::collections::HashMap;

use uuid::Uuid;
use xz2::write::XzEncoder;
use tempfile::tempfile;

use super::Result;
use super::sdk::{SdkInfo, DumpOptions, Objects};
use super::dsym::{Object, Variant};
use super::memdbtypes::{IndexItem, StoredSlice, MemDbHeader, IndexedUuid};
use super::utils::{file_size_format, ProgressIndicator, copy_with_progress};


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
    symbol_count: usize,
    progress: ProgressIndicator,
    options: DumpOptions,
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
            symbol_count: 0,
            progress: if opts.show_progress_bar {
                ProgressIndicator::new(object_count)
            } else {
                ProgressIndicator::disabled()
            },
            options: opts,
        };
        let header = MemDbHeader { ..Default::default() };
        rv.write(&header)?;
        rv.progress.set_message("Initializing");
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
        self.progress.inc(offset);
        for variant in obj.variants() {
            let src = variant.name().or(filename).unwrap();
            if let Some(fname) = Path::new(src).file_name().and_then(|x| x.to_str()) {
                self.progress.set_message(fname);
            }
            if variant.uuid().is_some() {
                self.write_object_variant(&obj, &variant, src)?;
            }
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
            if idx % 100 == 0 {
                self.progress.tick();
            }
            self.symbol_count += 1;
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
            if idx % 500 == 0 {
                self.progress.tick();
            }
        }
        Ok(slices)
    }

    fn write_slices(&self, slices: &[StoredSlice], start: &mut u32, len: &mut u32) -> Result<()> {
        *start = self.tell()? as u32;
        for (idx, item) in slices.iter().enumerate() {
            self.write(item)?;
            if idx % 500 == 0 {
                self.progress.tick();
            }
        }
        *len = slices.len() as u32;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.progress.finish(&format!(
            "Processed {} symbols", self.symbol_count));
        self.progress.add_bar(3);

        let mut header = MemDbHeader { ..Default::default() };
        header.version = 1;
        header.sdk_info.set_from_sdk_info(&self.info);

        self.progress.inc(1);
        self.progress.set_message("Writing metadata");

        // start by writing out the index of the variants and record the slices.
        let mut slices = vec![];
        let mut ii_idx = 0;
        for variant in self.variants.iter() {
            let offset = self.tell()?;
            for index_item in variant {
                self.write(index_item)?;
                ii_idx += 1;
                if ii_idx % 100 == 0 {
                    self.progress.tick();
                }
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
            if idx % 200 == 0 {
                self.progress.tick();
            }
        }

        // next we write out the name + arch -> uuid index mapping.  We also sort
        // this by uuid so that the index matches up.
        header.tagged_object_names_start = self.tell()? as u32;
        self.object_uuid_mapping.sort_by_key(|&(_, b)| b);
        for (idx, &(ref tagged_object, _)) in self.object_uuid_mapping.iter().enumerate() {
            self.write_bytes(format!("{}\x00", tagged_object).as_bytes())?;
            if idx % 100 == 0 {
                self.progress.tick();
            }
        }
        header.tagged_object_names_end = self.tell()? as u32;

        // now write out all the object name sources
        let slices = self.make_string_slices(&self.object_names[..], true)?;
        self.write_slices(&slices[..], &mut header.object_names_start,
                          &mut header.object_names_count)?;

        self.progress.inc(1);
        self.progress.set_message("Writing symbols");

        // now write out all the symbols
        let slices = self.make_string_slices(&self.symbols[..], true)?;
        self.write_slices(&slices[..], &mut header.symbols_start,
                          &mut header.symbols_count)?;

        self.progress.inc(1);
        self.progress.set_message("Writing headers");

        let file_size = self.tell()?;

        // write the updated header
        self.seek(0)?;
        self.write(&header)?;

        self.progress.finish(&format!(
            "Indexed {} variants", self.variant_uuids.len()));

        // compress if necessary
        if self.options.compress {
            self.progress.add_bar(file_size);
            self.progress.set_message("Compressing");
            self.seek(0)?;
            let mut reader = self.tempfile.as_ref().unwrap().borrow_mut();
            let mut writer = self.writer.borrow_mut();
            {
                let mut zwriter = XzEncoder::new(&mut *writer, 9);
                copy_with_progress(&self.progress, &mut *reader, &mut zwriter)?;
            }
            let compressed_file_size = writer.seek(SeekFrom::Current(0))? as usize;
            let pct = (compressed_file_size * 100) / file_size;
            self.progress.finish(&format!(
                "Compressed from {} to {} ({}% of original size)",
                file_size_format(file_size),
                file_size_format(compressed_file_size),
                pct));
        }

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
