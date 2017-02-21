//! This module implements the in-memory database
//!
//! A support folder with SDK debug symbols can be processed into a
//! in-memory database format which is a flat file on the file system
//! that gets mmaped into the process.
use std::io::{Write, Seek, SeekFrom};
use std::mem;
use std::slice;
use std::cell::RefCell;
use std::collections::HashMap;

use uuid::Uuid;

use super::Result;
use super::sdk::SdkInfo;
use super::dsym::{Object, Variant};
use super::shoco::compress;
use super::memdbtypes::{IndexItem, StoredSlice, MemDbHeader, IndexedUuid};


pub struct MemDbBuilder<W> {
    writer: RefCell<W>,
    info: SdkInfo,
    symbols: Vec<String>,
    symbols_map: HashMap<String, u32>,
    object_names: Vec<String>,
    object_names_map: HashMap<String, u16>,
    object_uuid_mapping: Vec<(String, Uuid)>,
    variant_uuids: Vec<IndexedUuid>,
    variants: Vec<Vec<IndexItem>>,
}

impl<W: Write + Seek> MemDbBuilder<W> {

    pub fn new(writer: W, info: &SdkInfo) -> Result<MemDbBuilder<W>> {
        let rv = MemDbBuilder {
            writer: RefCell::new(writer),
            info: info.clone(),
            symbols: vec![],
            symbols_map: HashMap::new(),
            object_names: vec![],
            object_names_map: HashMap::new(),
            object_uuid_mapping: vec![],
            variant_uuids: vec![],
            variants: vec![],
        };
        let header = MemDbHeader { ..Default::default() };
        rv.write(&header)?;
        Ok(rv)
    }

    fn write_bytes(&self, x: &[u8]) -> Result<usize> {
        self.writer.borrow_mut().write_all(x)?;
        Ok(x.len())
    }

    fn write<T>(&self, x: &T) -> Result<usize> {
        unsafe {
            let bytes : *const u8 = mem::transmute(x);
            let size = mem::size_of_val(x);
            self.writer.borrow_mut().write_all(slice::from_raw_parts(bytes, size))?;
            Ok(size)
        }
    }

    fn seek(&self, new_pos: usize) -> Result<()> {
        self.writer.borrow_mut().seek(SeekFrom::Start(new_pos as u64))?;
        Ok(())
    }

    fn tell(&self) -> Result<usize> {
        Ok(self.writer.borrow_mut().seek(SeekFrom::Current(0))? as usize)
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

    pub fn write_object(&mut self, obj: &Object, filename: Option<&str>) -> Result<()> {
        for variant in obj.variants() {
            self.write_object_variant(&obj, &variant, filename)?;
        }
        Ok(())
    }

    pub fn write_object_variant(&mut self, obj: &Object, var: &Variant,
                                filename: Option<&str>) -> Result<()> {
        let mut symbols = obj.symbols(var.arch())?;
        let src = var.name().or(filename).unwrap();
        let src_id = self.add_object_name(src);

        // build symbol index
        let mut index = vec![];
        for (addr, sym) in symbols.iter() {
            let sym_id = self.add_symbol(sym);
            index.push(IndexItem::new(addr, src_id, sym_id));
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

    fn make_string_slices(&self, strings: &[String], try_compress: bool) -> Result<Vec<StoredSlice>> {
        let mut slices = vec![];
        for string in strings {
            let offset = self.tell()?;
            if try_compress {
                let compressed = compress(string.as_bytes());
                if compressed.len() < string.as_bytes().len() {
                    let len = self.write_bytes(compressed.as_slice())?;
                    slices.push(StoredSlice::new(offset, len, true));
                    continue;
                }
            }
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
        let mut header = MemDbHeader { ..Default::default() };
        header.version = 1;

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

        // now write out all the symbols
        let slices = self.make_string_slices(&self.symbols[..], true)?;
        self.write_slices(&slices[..], &mut header.symbols_start,
                          &mut header.symbols_count)?;

        // write the updated header
        self.seek(0)?;
        self.write(&header)?;

        Ok(())
    }
}
