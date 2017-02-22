//! This module implements the in-memory database
//!
//! A support folder with SDK debug symbols can be processed into a
//! in-memory database format which is a flat file on the file system
//! that gets mmaped into the process.
use std::str::from_utf8;
use std::mem;
use std::slice;
use std::path::Path;
use std::borrow::Cow;
use std::ffi::CStr;
use std::os::raw::c_char;

use std::fmt;
use uuid::Uuid;
use memmap::{Mmap, Protection};

use super::{Result, ErrorKind};
use super::sdk::SdkInfo;
use super::memdbtypes::{IndexItem, StoredSlice, MemDbHeader, IndexedUuid};
use super::utils::binsearch_by_key;


enum Backing<'a> {
    Buf(Cow<'a, [u8]>),
    Mmap(Mmap),
}

/// Provides access to a memdb file
pub struct MemDb<'a> {
    info: SdkInfo,
    backing: Backing<'a>
}

/// Represents a symbol from a memdb file.
#[derive(Debug)]
pub struct Symbol<'a> {
    object_name: Cow<'a, str>,
    symbol: Cow<'a, str>,
    addr: u64,
}

impl<'a> fmt::Display for Symbol<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:016x} {} ({})", self.addr(), self.symbol(), self.object_name())
    }
}

impl<'a> Backing<'a> {

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
    fn buffer(&self) -> &[u8] {
        match *self {
            Backing::Buf(ref buf) => buf,
            Backing::Mmap(ref mmap) => unsafe { mmap.as_slice() }
        }
    }
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

fn load_memdb<'a>(backing: Backing<'a>) -> Result<MemDb<'a>> {
    let info = {
        let header = backing.header()?;
        if header.version != 1 {
            return Err(ErrorKind::UnsupportedMemDbVersion.into());
        }
        header.sdk_info.to_sdk_info()
    };
    Ok(MemDb {
        backing: backing,
        info: info,
    })
}

impl<'a> MemDb<'a> {

    /// Constructs a memdb object from a byte slice cow.
    pub fn from_cow(cow: Cow<'a, [u8]>) -> Result<MemDb<'a>> {
        load_memdb(Backing::Buf(cow))
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
        load_memdb(Backing::Mmap(mmap))
    }

    /// Return the SDK info.
    pub fn info(&self) -> &SdkInfo {
        &self.info
    }

    /// Finds a symbol by UUID and address.
    pub fn lookup_by_uuid(&'a self, uuid: &Uuid, addr: u64) -> Option<Symbol<'a>> {
        self.lookup_impl(uuid, addr).ok().and_then(|x| x)
    }

    /// Finds a symbol by object name and architecture
    pub fn lookup_by_object_name(&'a self, object_name: &str, arch: &str, addr: u64)
        -> Option<Symbol<'a>>
    {
        if let Ok(Some(uuid)) = self.find_uuid(object_name, arch) {
            self.lookup_impl(uuid, addr).ok().and_then(|x| x)
        } else {
            None
        }
    }

    fn get_cstr(&self, offset: usize) -> Result<&str> {
        unsafe {
            Ok(from_utf8(CStr::from_ptr(self.backing.buffer().as_ptr().offset(
                offset as isize) as *const c_char).to_bytes())?)
        }
    }

    fn find_uuid(&self, object_name: &str, arch: &str) -> Result<Option<&Uuid>> {
        let header = self.backing.header()?;
        let mut offset = header.tagged_object_names_start as usize;
        let refstr = format!("{}:{}", object_name, arch);
        let mut uuid_idx = 0;
        while offset < header.tagged_object_names_end as usize {
            let s = self.get_cstr(offset)?;
            if s == &refstr {
                return Ok(Some(&self.uuids()?[uuid_idx].uuid));
            }
            offset += s.len();
            uuid_idx += 1;
        }
        Ok(None)
    }

    fn lookup_impl(&'a self, uuid: &Uuid, addr: u64) -> Result<Option<Symbol<'a>>>
    {
        if let Some(index) = self.get_index(uuid)? {
            if let Some(item) = binsearch_by_key(index, addr, |item| item.addr()) {
                return Ok(Some(self.index_item_to_symbol(item)?));
            }
        }
        Ok(None)
    }

    #[inline(always)]
    fn uuids(&self) -> Result<&[IndexedUuid]> {
        let head = self.backing.header()?;
        self.backing.get_slice(head.uuids_start as usize, head.uuids_count as usize)
    }

    #[inline(always)]
    fn variants(&self) -> Result<&[StoredSlice]> {
        let head = self.backing.header()?;
        self.backing.get_slice(head.variants_start as usize, head.variants_count as usize)
    }

    #[inline(always)]
    fn get_index(&self, uuid: &Uuid) -> Result<Option<&[IndexItem]>> {
        let uuids = self.uuids()?;
        if let Some(iuuid) = binsearch_by_key(uuids, *uuid, |item| *item.uuid()) {
            let variant_slice = &self.variants()?[iuuid.idx()];
            unsafe {
                let data = self.backing.get_data(variant_slice.offset(),
                                                 variant_slice.len())?;
                let count = variant_slice.len() / mem::size_of::<IndexItem>();
                return Ok(Some(slice::from_raw_parts(
                    mem::transmute(data.as_ptr()),
                    count
                )));
            }
        }
        Ok(None)
    }

    #[inline(always)]
    fn symbols(&self) -> Result<&[StoredSlice]> {
        let head = self.backing.header()?;
        self.backing.get_slice(head.symbols_start as usize, head.symbols_count as usize)
    }

    #[inline(always)]
    fn object_names(&self) -> Result<&[StoredSlice]> {
        let head = self.backing.header()?;
        self.backing.get_slice(head.object_names_start as usize, head.object_names_count as usize)
    }

    #[inline(always)]
    fn get_string(&'a self, slice: &StoredSlice) -> Result<Cow<'a, str>> {
        let bytes = self.backing.get_data(slice.offset(), slice.len())?;
        if slice.is_compressed() {
            panic!("We do not support compression");
        } else {
            Ok(Cow::Borrowed(from_utf8(bytes)?))
        }
    }

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
