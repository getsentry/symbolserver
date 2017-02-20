use std::io::{Write, Seek, SeekFrom};
use std::str::from_utf8;
use std::mem;
use std::slice;
use std::cell::RefCell;
use std::path::Path;
use std::borrow::Cow;
use std::collections::HashMap;

use uuid::Uuid;
use memmap::{Mmap, Protection};

use super::{Result, ErrorKind};
use super::sdk::SdkInfo;
use super::dsym::{Object, Variant};


// stored information:
//
//      object name + arch -> uuid
//      uuid -> stored variant
//      stored variant + addr -> symbol
//      symbol -> [object name id, symbol name id, symbol addr]
//      object name id -> object name
//      symbol name id -> symbol name
//

enum Backing<'a> {
    Buf(Cow<'a, [u8]>),
    Mmap(Mmap),
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone)]
struct MemDbHeader {
    pub version: u32,
    pub variants_start: u32,
    pub variants_count: u32,
    pub uuids_start: u32,
    pub uuids_count: u32,
    pub tagged_object_names_start: u32,
    pub tagged_object_names_count: u32,
    pub object_names_start: u32,
    pub object_names_count: u32,
    pub symbols_start: u32,
    pub symbols_count: u32,
}

#[repr(C, packed)]
struct StoredSlice {
    pub offset: u32,
    pub length: u32,
}

#[repr(C, packed)]
struct IndexItem {
    addr_low: u32,
    addr_high: u16,
    src_id: u16,
    sym_id: u32,
}

pub struct MemDb<'a> {
    backing: Backing<'a>
}

pub struct Symbol<'a> {
    object_name: &'a str,
    symbol: &'a str,
    addr: u64,
}

pub struct MemDbBuilder<W> {
    writer: RefCell<W>,
    info: SdkInfo,
    symbols: Vec<String>,
    symbols_map: HashMap<String, u32>,
    object_names: Vec<String>,
    object_names_map: HashMap<String, u16>,
    object_uuid_mapping: Vec<String>,
    variant_uuids: Vec<Uuid>,
    variants: Vec<Vec<IndexItem>>,
}

impl IndexItem {
    pub fn addr(&self) -> u64 {
        ((self.addr_high as u64) << 32) | (self.addr_low as u64)
    }
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

    fn add_symbol(&mut self, sym: &str) -> u32 {
        if let Some(&sym_id) = self.symbols_map.get(sym) {
            return sym_id;
        }
        let symbol_count = self.symbols.len() as u32;
        self.symbols.push(sym.to_string());
        self.symbols_map.insert(sym.to_string(), symbol_count);
        symbol_count
    }

    fn add_object_name(&mut self, src: &str) -> u16 {
        if let Some(&src_id) = self.object_names_map.get(src) {
            return src_id;
        }
        let object_count = self.object_names.len() as u16;
        self.object_names.push(src.to_string());
        self.object_names_map.insert(src.to_string(), object_count);
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
            index.push(IndexItem {
                addr_low: (addr & 0xffffffff) as u32,
                addr_high: ((addr >> 32) &0xffff) as u16,
                src_id: src_id,
                sym_id: sym_id,
            });
        }
        index.sort_by_key(|item| item.addr());

        // register variant and uuid as well as object name and arch
        self.variant_uuids.push(var.uuid().unwrap());
        self.variants.push(index);
        self.object_uuid_mapping.push(format!("{}:{}", src, var.arch()));

        Ok(())
    }

    fn make_string_slices(&self, strings: &[String]) -> Result<Vec<StoredSlice>> {
        let mut slices = vec![];
        for string in strings {
            let offset = self.tell()?;
            let len = self.write_bytes(string.as_bytes())?;
            slices.push(StoredSlice {
                offset: offset as u32,
                length: len as u32
            });
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

        // start by writing out the index of the variants and record the slices.
        let mut slices = vec![];
        for variant in self.variants.iter() {
            let offset = self.tell()?;
            for index_item in variant {
                self.write(index_item)?;
            }
            slices.push(StoredSlice {
                offset: offset as u32,
                length: (self.tell()? - offset) as u32
            });
        }
        self.write_slices(&slices[..], &mut header.variants_start,
                          &mut header.variants_count)?;

        // next write out the UUIDs.  Since these are fixed length we do not
        // need to use slices here.
        header.uuids_start = self.tell()? as u32;
        header.uuids_count = self.variant_uuids.len() as u32;
        for uuid in self.variant_uuids.iter() {
            self.write_bytes(uuid.as_bytes())?;
        }

        // next we write out the name + arch -> uuid index mapping
        let slices = self.make_string_slices(&self.object_uuid_mapping[..])?;
        self.write_slices(&slices[..], &mut header.tagged_object_names_start,
                          &mut header.tagged_object_names_count)?;

        // now write out all the object name sources
        let slices = self.make_string_slices(&self.object_names[..])?;
        self.write_slices(&slices[..], &mut header.object_names_start,
                          &mut header.tagged_object_names_count)?;

        // now write out all the symbols
        let slices = self.make_string_slices(&self.symbols[..])?;
        self.write_slices(&slices[..], &mut header.symbols_start,
                          &mut header.symbols_count)?;

        // write the updated header
        self.seek(0)?;
        self.write(&header)?;

        Ok(())
    }
}


fn verify_version<'a>(rv: MemDb<'a>) -> Result<MemDb<'a>> {
    if rv.header()?.version != 1 {
        Err(ErrorKind::UnsupportedMemDbVersion.into())
    } else {
        Ok(rv)
    }
}

impl<'a> MemDb<'a> {

    pub fn from_cow(cow: Cow<'a, [u8]>) -> Result<MemDb<'a>> {
        verify_version(MemDb {
            backing: Backing::Buf(cow),
        })
    }

    pub fn from_slice(buffer: &'a [u8]) -> Result<MemDb<'a>> {
        MemDb::from_cow(Cow::Borrowed(buffer))
    }

    pub fn from_vec(buffer: Vec<u8>) -> Result<MemDb<'a>> {
        MemDb::from_cow(Cow::Owned(buffer))
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<MemDb<'a>> {
        let mmap = Mmap::open_path(path, Protection::Read)?;
        verify_version(MemDb {
            backing: Backing::Mmap(mmap),
        })
    }

    #[inline(always)]
    pub fn buffer(&self) -> &[u8] {
        match self.backing {
            Backing::Buf(ref buf) => buf,
            Backing::Mmap(ref mmap) => unsafe { mmap.as_slice() }
        }
    }

    fn get_data(&self, start: usize, len: usize) -> Result<&[u8]> {
        let buffer = self.buffer();
        let end = start.wrapping_add(len);
        if end < start || end > buffer.len() {
            Err(ErrorKind::BadMemDb.into())
        } else {
            Ok(&buffer[start..end])
        }
    }

    fn get_slice<T>(&self, offset: usize, count: usize) -> Result<&[T]> {
        let size = mem::size_of::<T>();
        Ok(unsafe {
            slice::from_raw_parts(
                mem::transmute(self.get_data(offset, count * size)?.as_ptr()),
                count
            )
        })
    }

    #[inline(always)]
    fn header(&self) -> Result<&MemDbHeader> {
        unsafe {
            Ok(mem::transmute(self.get_data(0, mem::size_of::<MemDbHeader>())?.as_ptr()))
        }
    }

    #[inline(always)]
    fn uuids(&self) -> Result<&[Uuid]> {
        let head = self.header()?;
        self.get_slice(head.uuids_start as usize, head.uuids_count as usize)
    }

    #[inline(always)]
    fn variants(&self) -> Result<&[StoredSlice]> {
        let head = self.header()?;
        self.get_slice(head.variants_start as usize, head.variants_count as usize)
    }

    #[inline(always)]
    fn get_index(&self, uuid: &Uuid) -> Result<Option<&[IndexItem]>> {
        let uuids = self.uuids()?;
        for (idx, item_uuid) in uuids.iter().enumerate() {
            if item_uuid == uuid {
                let variant_slice = &self.variants()?[idx];
                unsafe {
                    let data = self.get_data(variant_slice.offset as usize,
                                             variant_slice.length as usize)?;
                    let count = variant_slice.length as usize / mem::size_of::<IndexItem>();
                    return Ok(Some(slice::from_raw_parts(
                        mem::transmute(data.as_ptr()),
                        count
                    )));
                }
            }
        }
        Ok(None)
    }

    #[inline(always)]
    fn symbols(&self) -> Result<&[StoredSlice]> {
        let head = self.header()?;
        self.get_slice(head.symbols_start as usize, head.symbols_count as usize)
    }

    #[inline(always)]
    fn object_names(&self) -> Result<&[StoredSlice]> {
        let head = self.header()?;
        self.get_slice(head.object_names_start as usize, head.object_names_count as usize)
    }

    #[inline(always)]
    fn get_string(&self, slice: &StoredSlice) -> Result<&str> {
        let bytes = self.get_data(slice.offset as usize, slice.length as usize)?;
        Ok(from_utf8(bytes)?)
    }

    pub fn lookup_by_uuid(&'a self, uuid: &Uuid, addr: u64)
        -> Result<Option<Symbol<'a>>>
    {
        let index = match self.get_index(uuid)? {
            Some(idx) => idx,
            None => { return Ok(None); }
        };
        let mut low = 0;
        let mut high = index.len();

        while low < high {
            let mid = (low + high) / 2;
            let ii = &index[mid as usize];
            if addr < ii.addr() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }

        if low > 0 && low <= index.len() {
            Ok(Some(self.index_item_to_symbol(&index[low - 1])?))
        } else {
            Ok(None)
        }
    }

    /*
    pub fn lookup_by_object_name<'a>(&'a self, object_name: &str, arch: &str, addr: u64)
        -> Option<Symbol<'a>>
    {
        self.find_uuid(object_name, arch).and_then(|uuid| {
            self.lookup_by_uuid(uuid, addr)
        })
    }
    */

    fn get_object_name(&'a self, src_id: u16) -> Result<&'a str> {
        self.get_string(&self.object_names()?[src_id as usize])
    }

    fn get_symbol(&'a self, sym_id: u32) -> Result<&'a str> {
        self.get_string(&self.symbols()?[sym_id as usize])
    }

    fn index_item_to_symbol(&'a self, ii: &IndexItem) -> Result<Symbol<'a>> {
        Ok(Symbol {
            object_name: self.get_object_name(ii.src_id)?,
            symbol: self.get_symbol(ii.sym_id)?,
            addr: ii.addr(),
        })
    }
}
