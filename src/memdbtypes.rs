use std::str::from_utf8;

use uuid::Uuid;

use super::sdk::SdkInfo;


#[repr(C, packed)]
#[derive(Default, Copy, Clone)]
pub struct MemDbHeader {
    pub version: u32,
    pub sdk_info: PackedSdkInfo,
    pub variants_start: u32,
    pub variants_count: u32,
    pub uuids_start: u32,
    pub uuids_count: u32,
    pub tagged_object_names_start: u32,
    pub tagged_object_names_end: u32,
    pub object_names_start: u32,
    pub object_names_count: u32,
    pub symbols_start: u32,
    pub symbols_count: u32,
}

#[repr(C, packed)]
#[derive(Default, Copy, Clone)]
pub struct PackedSdkInfo {
    pub name: [u8; 8],
    pub version_major: u16,
    pub version_minor: u16,
    pub version_patchlevel: u16,
    pub build: [u8; 10],
}

#[repr(C, packed)]
pub struct StoredSlice {
    pub offset: u32,
    pub len: u32,
}

#[repr(C, packed)]
pub struct IndexedUuid {
    pub uuid: Uuid,
    pub idx: u16,
}

#[repr(C, packed)]
#[derive(Debug)]
pub struct IndexItem {
    addr_low: u32,
    addr_high: u16,
    src_id: u16,
    sym_id: u32,
}

fn copy_str_to_slice(slice: &mut [u8], s: &str) {
    let bytes = s.as_bytes();
    (&mut slice[..bytes.len()]).copy_from_slice(bytes);
}

fn str_from_zero_slice(slice: &[u8]) -> &str {
    from_utf8(slice).unwrap().trim_right_matches('\x00')
}

impl PackedSdkInfo {

    pub fn set_from_sdk_info(&mut self, info: &SdkInfo) {
        self.version_major = info.version_major() as u16;
        self.version_minor = info.version_minor() as u16;
        self.version_patchlevel = info.version_patchlevel() as u16;
        copy_str_to_slice(&mut self.name[..], info.name());
        copy_str_to_slice(&mut self.build[..], info.build());
    }

    pub fn to_sdk_info(&self) -> SdkInfo {
        SdkInfo::new(
            str_from_zero_slice(&self.name[..]),
            self.version_major as u32,
            self.version_minor as u32,
            self.version_patchlevel as u32,
            str_from_zero_slice(&self.build[..]),
        )
    }
}

impl IndexedUuid {

    pub fn new(uuid: &Uuid, idx: usize) -> IndexedUuid {
        IndexedUuid {
            uuid: *uuid,
            idx: idx as u16,
        }
    }

    pub fn uuid(&self) -> &Uuid {
        &self.uuid
    }

    pub fn idx(&self) -> usize {
        self.idx as usize
    }
}

impl StoredSlice {

    pub fn new(offset: usize, mut len: usize, is_compressed: bool) -> StoredSlice {
        if is_compressed {
            len |= 0x80000000;
        }
        StoredSlice {
            offset: offset as u32,
            len: len as u32,
        }
    }

    pub fn offset(&self) -> usize {
        self.offset as usize
    }

    pub fn len(&self) -> usize {
        (self.len as usize) & 0x7fffffff
    }

    pub fn is_compressed(&self) -> bool {
        self.len >> 31 != 0
    }
}

impl IndexItem {
    pub fn new(addr: u64, src_id: usize, sym_id: usize) -> IndexItem {
        IndexItem {
            addr_low: (addr & 0xffffffff) as u32,
            addr_high: ((addr >> 32) &0xffff) as u16,
            src_id: src_id as u16,
            sym_id: sym_id as u32,
        }
    }

    pub fn addr(&self) -> u64 {
        ((self.addr_high as u64) << 32) | (self.addr_low as u64)
    }

    pub fn src_id(&self) -> usize {
        self.src_id as usize
    }

    pub fn sym_id(&self) -> usize {
        self.sym_id as usize
    }
}
