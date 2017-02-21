//! This module implements the in-memory database
//!
//! A support folder with SDK debug symbols can be processed into a
//! in-memory database format which is a flat file on the file system
//! that gets mmaped into the process.
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
use super::shoco::{compress, decompress};
use super::memdbtypes::{IndexItem, StoredSlice, MemDbHeader};


enum Backing<'a> {
    Buf(Cow<'a, [u8]>),
    Mmap(Mmap),
}

/// Provides access to a memdb file
pub struct MemDb<'a> {
    backing: Backing<'a>
}

/// Represents a symbol from a memdb file.
#[derive(Debug)]
pub struct Symbol<'a> {
    object_name: Cow<'a, str>,
    symbol: Cow<'a, str>,
    addr: u64,
}

impl<'a> Symbol<'a> {

    /// The object name a string
    pub fn object_name(&self) -> &str {
        return &self.object_name
    }

    /// The symbol as string
    pub fn symbol(&self) -> &str {
        return &self.symbol
    }

    /// The symbol address as u64
    pub fn addr(&self) -> u64 {
        self.addr
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

    /// Constructs a memdb object from a byte slice cow.
    pub fn from_cow(cow: Cow<'a, [u8]>) -> Result<MemDb<'a>> {
        verify_version(MemDb {
            backing: Backing::Buf(cow),
        })
    }

    /// Constructs a memdb object from a byte slice.
    pub fn from_slice(buffer: &'a [u8]) -> Result<MemDb<'a>> {
        MemDb::from_cow(Cow::Borrowed(buffer))
    }

    /// Constructs a memdb object from a byte vector.
    pub fn from_vec(buffer: Vec<u8>) -> Result<MemDb<'a>> {
        MemDb::from_cow(Cow::Owned(buffer))
    }

    /// Constructs a memdb object by mmapping a file from the filesystem in.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<MemDb<'a>> {
        let mmap = Mmap::open_path(path, Protection::Read)?;
        verify_version(MemDb {
            backing: Backing::Mmap(mmap),
        })
    }

    /// Finds a symbol by UUID and address.
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

    #[inline(always)]
    fn buffer(&self) -> &[u8] {
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
                    let data = self.get_data(variant_slice.offset(),
                                             variant_slice.len())?;
                    let count = variant_slice.len() / mem::size_of::<IndexItem>();
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
    fn get_string(&'a self, slice: &StoredSlice) -> Result<Cow<'a, str>> {
        let bytes = self.get_data(slice.offset(), slice.len())?;
        if slice.is_compressed() {
            let decompressed = decompress(bytes);
            Ok(Cow::Owned(String::from_utf8(decompressed)?))
        } else {
            Ok(Cow::Borrowed(from_utf8(bytes)?))
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

    fn get_object_name(&'a self, src_id: usize) -> Result<Cow<'a, str>> {
        self.get_string(&self.object_names()?[src_id])
    }

    fn get_symbol(&'a self, sym_id: usize) -> Result<Cow<'a, str>> {
        self.get_string(&self.symbols()?[sym_id])
    }

    fn index_item_to_symbol(&'a self, ii: &IndexItem) -> Result<Symbol<'a>> {
        Ok(Symbol {
            object_name: self.get_object_name(ii.src_id())?,
            symbol: self.get_symbol(ii.sym_id())?,
            addr: ii.addr(),
        })
    }
}
