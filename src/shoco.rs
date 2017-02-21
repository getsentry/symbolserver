extern "C" {
    fn shoco_compress(input: *const u8, len: usize, out: *mut u8, bufsize: usize) -> usize;
    fn shoco_decompress(input: *const u8, len: usize, out: *mut u8, bufsize: usize) -> usize;
}

pub fn compress(bytes: &[u8]) -> Vec<u8> {
    let buflen = bytes.len() * 4;
    let mut buf = Vec::with_capacity(buflen);
    unsafe {
        let size = shoco_compress(bytes.as_ptr(), bytes.len(),
                                  buf.as_mut_ptr(), buflen);
        buf.set_len(size);
    }
    buf
}

pub fn decompress(bytes: &[u8]) -> Vec<u8> {
    let buflen = bytes.len() * 8;
    let mut buf = Vec::with_capacity(buflen);
    unsafe {
        let size = shoco_decompress(bytes.as_ptr(), bytes.len(),
                                    buf.as_mut_ptr(), buflen);
        buf.set_len(size);
    }
    buf.shrink_to_fit();
    buf
}
