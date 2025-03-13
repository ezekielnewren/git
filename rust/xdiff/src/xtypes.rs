use interop::ivec::IVec;

#[repr(C)]
pub struct xrecord {
    ptr: *const u8,
    size_no_eol: usize,
    size_with_eol: usize,
    pub(crate) line_hash: u64,
}


impl xrecord {
    pub fn as_ref(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.ptr, self.size_no_eol)
        }
    }
}

unsafe impl Send for xrecord {}
unsafe impl Sync for xrecord {}

pub struct xdfile {
    pub minimal_perfect_hash: IVec<u64>,
    pub record: IVec<xrecord>,
}

