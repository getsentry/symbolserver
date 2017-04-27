//! This module implements the in-memory database
//!
//! A support folder with SDK debug symbols can be processed into a
//! in-memory database format which is a flat file on the file system
//! that gets mmaped into the process.
use std::io::{Write, Seek, SeekFrom};
use std::fs::File;
use std::mem;
use std::slice;
use std::cell::RefCell;
use std::collections::{HashSet, HashMap};

use uuid::Uuid;
use xz2::write::XzEncoder;
use tempfile::tempfile;
use indicatif::{ProgressBar, ProgressStyle, style, StyledObject};

use super::types::{IndexItem, StoredSlice, MemDbHeader, IndexedUuid};
use super::super::Result;
use super::super::sdk::{SdkInfo, DumpOptions, Objects};
use super::super::dsym::{Object, Variant};
use super::super::utils::{file_size_format, copy_with_progress};


struct MemDbBuilder<W> {
    writer: RefCell<W>,
    tempfile: Option<RefCell<File>>,
    info: SdkInfo,
    symbols: Vec<String>,
    symbols_map: HashMap<String, u32>,
    object_names: Vec<String>,
    object_names_map: HashMap<String, u16>,
    object_uuid_mapping: Vec<(String, Uuid)>,
    variant_uuids: Vec<IndexedUuid>,
    variant_uuids_seen: HashSet<Uuid>,
    variants: Vec<Vec<IndexItem>>,
    symbol_count: usize,
    options: DumpOptions,
}

fn format_step(step: usize, opts: &DumpOptions) -> StyledObject<String> {
    let steps = if opts.compress {
        5
    } else {
        4
    };
    style(format!("[{}/{}]", step, steps)).dim()
}

trait WriteSeek : Write + Seek {}
impl<T: Write+Seek> WriteSeek for T {}

impl<W: Write + Seek> MemDbBuilder<W> {

    pub fn new(writer: W, info: &SdkInfo, opts: DumpOptions)
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
            variant_uuids_seen: HashSet::new(),
            variants: vec![],
            symbol_count: 0,
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

    fn add_symbol(&mut self, sym: &str) -> u32 {
        if let Some(&sym_id) = self.symbols_map.get(sym) {
            return sym_id as u32;
        }
        let symbol_count = self.symbols.len();
        self.symbols.push(sym.to_string());
        self.symbols_map.insert(sym.to_string(), symbol_count as u32);
        symbol_count as u32
    }

    fn add_object_name(&mut self, src: &str) -> u16 {
        if let Some(&src_id) = self.object_names_map.get(src) {
            return src_id as u16;
        }
        let object_count = self.object_names.len();
        self.object_names.push(src.to_string());
        self.object_names_map.insert(src.to_string(), object_count as u16);
        object_count as u16
    }

    pub fn write_object(&mut self, obj: &Object, filename: Option<&str>) -> Result<()> {
        for variant in obj.variants() {
            let src = variant.name().or(filename).unwrap();
            if let Some(uuid) = variant.uuid() {
                self.write_object_variant(&obj, &variant, &uuid, src)?;
            }
        }
        Ok(())
    }

    fn write_object_variant(&mut self, obj: &Object, var: &Variant,
                            uuid: &Uuid, src: &str) -> Result<bool> {
        self.object_uuid_mapping.push((
            format!("{}:{}", src, var.arch()),
            *uuid
        ));

        if self.variant_uuids_seen.contains(uuid) {
            return Ok(false);
        }
        self.variant_uuids_seen.insert(*uuid);

        let mut symbols = obj.symbols(var.arch())?;
        let src_id = self.add_object_name(src);

        // build symbol index
        let mut index = vec![];
        for (addr, sym) in symbols.iter() {
            let sym_id = self.add_symbol(sym);
            index.push(IndexItem::new(addr - var.vmaddr(), src_id, Some(sym_id)));
            self.symbol_count += 1;
        }

        // write an end marker if we know the image size
        if var.vmsize() > 0 {
            index.push(IndexItem::new(var.vmsize(), src_id, None));
            self.symbol_count += 1;
        }

        index.sort_by_key(|item| item.addr());

        // register variant and uuid
        self.variant_uuids.push(IndexedUuid::new(uuid, self.variants.len()));
        self.variants.push(index);

        Ok(true)
    }

    fn make_string_slices(&self, strings: &[String], _try_compress: bool) -> Result<Vec<StoredSlice>> {
        let mut slices = vec![];
        for string in strings.iter() {
            let offset = self.tell()?;
            let len = self.write_bytes(string.as_bytes())?;
            slices.push(StoredSlice::new(offset, len, false));
        }
        Ok(slices)
    }

    fn write_slices(&self, slices: &[StoredSlice], start: &mut u32, len: &mut u32) -> Result<()> {
        *start = self.tell()? as u32;
        for item in slices.iter() {
            self.write(item)?;
        }
        *len = slices.len() as u32;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        println!("      Found {} symbols", style(self.symbol_count).cyan());
        let mut header = MemDbHeader { ..Default::default() };
        header.version = 2;
        header.sdk_info.set_from_sdk_info(&self.info);

        println!("{} Writing metadata", format_step(2, &self.options));
        // start by writing out the index of the variants and record the slices.
        let mut slices = vec![];
        for variant in self.variants.iter() {
            let offset = self.tell()?;
            for index_item in variant {
                self.write(index_item)?;
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
        for indexed_uuid in self.variant_uuids.iter() {
            self.write(indexed_uuid)?;
        }

        // next we write out the name + arch -> uuid index mapping.  We also sort
        // this by uuid so that the index matches up.
        header.tagged_object_names_start = self.tell()? as u32;
        self.object_uuid_mapping.sort_by_key(|&(_, b)| b);
        for &(ref tagged_object, _) in self.object_uuid_mapping.iter() {
            self.write_bytes(format!("{}\x00", tagged_object).as_bytes())?;
        }
        header.tagged_object_names_end = self.tell()? as u32;

        // now write out all the object name sources
        let slices = self.make_string_slices(&self.object_names[..], true)?;
        self.write_slices(&slices[..], &mut header.object_names_start,
                          &mut header.object_names_count)?;

        println!("{} Writing symbols", format_step(3, &self.options));

        // now write out all the symbols
        let slices = self.make_string_slices(&self.symbols[..], true)?;
        self.write_slices(&slices[..], &mut header.symbols_start,
                          &mut header.symbols_count)?;

        println!("{} Writing headers", format_step(4, &self.options));

        let file_size = self.tell()?;

        // write the updated header
        self.seek(0)?;
        self.write(&header)?;

        println!("      Indexed {} variants",
                 style(self.variant_uuids.len()).cyan());

        // compress if necessary
        if self.options.compress {
            println!("{} Compressing", format_step(5, &self.options));
            let pb = ProgressBar::new(file_size as u64);
            pb.set_style(ProgressStyle::default_bar()
                .template("{wide_bar} {bytes}/{total_bytes}"));
            self.seek(0)?;
            let mut reader = self.tempfile.as_ref().unwrap().borrow_mut();
            let mut writer = self.writer.borrow_mut();
            {
                let mut zwriter = XzEncoder::new(&mut *writer, 9);
                copy_with_progress(&pb, &mut *reader, &mut zwriter)?;
            }
            let compressed_file_size = writer.seek(SeekFrom::Current(0))? as usize;
            let pct = (compressed_file_size * 100) / file_size;
            pb.finish_and_clear();
            println!(
                "      Compressed from {} to {} ({}% of original size)",
                file_size_format(file_size),
                file_size_format(compressed_file_size),
                pct);
        }

        Ok(())
    }
}

/// Dumps objects into a writer
pub fn dump_memdb<W: Write + Seek>(writer: W, info: &SdkInfo,
                                   opts: DumpOptions, objects: Objects)
    -> Result<()>
{
    println!("{} Processing {} files", format_step(1, &opts),
             style(objects.file_count()).cyan());
    let mut builder = MemDbBuilder::new(writer, info, opts)?;
    let pb = ProgressBar::new(objects.file_count() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{msg:.dim}\n{wide_bar} {pos:>5}/{len}"));
    for obj_res in objects {
        let (offset, filename, obj) = obj_res?;
        pb.set_message(&filename);
        builder.write_object(&obj, Some(&filename))?;
        pb.inc(offset as u64);
    }
    pb.finish_and_clear();
    builder.flush()?;
    Ok(())
}
