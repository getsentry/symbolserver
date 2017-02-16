use std::io::Cursor;
use std::path::Path;
use std::borrow::Cow;

use memmap;
use mach_object::{OFile, Symbol, Section, SymbolIter, SymbolReader,
    get_arch_name_from_types, get_arch_from_flag, SEG_TEXT, SECT_TEXT};

use super::{Result, Error, ErrorKind};


enum Backing<'a> {
    Buf(Cow<'a, [u8]>),
    Mmap(memmap::Mmap),
}

pub struct Object<'a> {
    backing: Backing<'a>,
    ofile: OFile,
    archs: Vec<&'a str>,
}

pub struct SymbolIterator<'a> {
    iter: Option<SymbolIter<'a>>,
}

pub struct Symbols<'a> {
    ofile: &'a OFile,
    cursor: Cursor<&'a [u8]>,
}

impl<'a> Symbols<'a> {
    pub fn iter(&'a mut self) -> SymbolIterator<'a> {
        SymbolIterator {
            iter: self.ofile.symbols(&mut self.cursor),
        }
    }
}

impl<'a> Iterator for SymbolIterator<'a> {
    type Item = (u64, &'a str);

    fn next(&mut self) -> Option<(u64, &'a str)> {
        if let Some(iter) = self.iter.as_mut() {
            while let Some(sym) = iter.next() {
                if let Symbol::Defined { ref name, external, ref section, entry, .. } = sym {
                    if !external && name.is_some() {
                        if let &Some(ref section) = section {
                            let Section { ref sectname, ref segname, .. } = **section;
                            if segname == SEG_TEXT && sectname == SECT_TEXT {
                                return Some((entry as u64, name.unwrap()));
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

impl<'a> Backing<'a> {

    #[inline(always)]
    pub fn buffer(&self) -> &[u8] {
        match *self {
            Backing::Buf(ref buf) => buf,
            Backing::Mmap(ref mmap) => unsafe { mmap.as_slice() }
        }
    }

    #[inline(always)]
    pub fn cursor(&self, offset: usize) -> Cursor<&[u8]> {
        Cursor::new(&self.buffer()[offset..])
    }
}

impl<'a> Object<'a> {

    fn from_backing(backing: Backing<'a>) -> Result<Object<'a>> {
        let ofile = OFile::parse(&mut backing.cursor(0))?;
        let mut archs = vec![];

        match ofile {
            OFile::FatFile { ref files, .. } => {
                for &(ref arch, _) in files {
                    if let Some(arch_str) = get_arch_name_from_types(
                            arch.cputype, arch.cpusubtype) {
                        archs.push(arch_str);
                    }
                }
            }
            OFile::MachFile { ref header, .. } => {
                if let Some(arch_str) = get_arch_name_from_types(
                        header.cputype, header.cpusubtype) {
                    archs.push(arch_str);
                }
            }
            _ => {}
        }

        Ok(Object {
            backing: backing,
            ofile: ofile,
            archs: archs,
        })
    }

    fn from_cow(cow: Cow<'a, [u8]>) -> Result<Object<'a>> {
        Object::from_backing(Backing::Buf(cow))
    }

    /// Parses a macho object from a given slice.
    pub fn from_slice(buf: &'a [u8]) -> Result<Object<'a>> {
        Object::from_cow(Cow::Borrowed(buf))
    }

    /// Parses a macho object from a vector.
    pub fn from_vec(buf: Vec<u8>) -> Result<Object<'a>> {
        Object::from_cow(Cow::Owned(buf))
    }

    /// Parses a macho object from a memory mapped file.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Object<'a>> {
        let mmap = memmap::Mmap::open_path(path, memmap::Protection::Read)?;
        Object::from_backing(Backing::Mmap(mmap))
    }

    /// Returns a list of contained CPU architectures
    pub fn architectures(&self) -> &[&str] {
        &self.archs[..]
    }

    /// Returns an iterator over the symbols of an architecture.
    pub fn symbols<'b>(&'a self, arch: &'b str) -> Result<Symbols<'a>> {
        let &(cputype, cpusubtype) = get_arch_from_flag(arch).ok_or_else(|| {
            Error::from(ErrorKind::UnknownArchitecture(arch.to_string()))
        })?;

        match self.ofile {
            OFile::FatFile { ref files, .. } => {
                for &(ref arch, ref file) in files {
                    if arch.cputype == cputype && arch.cpusubtype == cpusubtype {
                        let cursor = self.backing.cursor(arch.offset as usize);
                        return Ok(Symbols {
                            ofile: file,
                            cursor: cursor,
                        });
                    }
                }
            }
            OFile::MachFile { ref header, .. } => {
                if header.cputype == cputype && header.cpusubtype == cpusubtype {
                    let cursor = self.backing.cursor(0);
                    return Ok(Symbols {
                        ofile: &self.ofile,
                        cursor: cursor,
                    });
                }
            }
            _ => {}
        }

        return Err(ErrorKind::MissingArchitecture(arch.to_string()).into());
    }
}

pub fn test() {
    let obj = Object::from_path("/Users/mitsuhiko/Library/Developer/Xcode/iOS DeviceSupport/10.2 (14C92)/Symbols/System/Library/CoreServices/Encodings/libKoreanConverter.dylib").unwrap();

    for arch in obj.architectures() {
        let mut syms = obj.symbols(arch).unwrap();
        for (addr, sym) in syms.iter() {
            println!("{} | {} | {}", arch, addr, sym);
        }
    }
}
