use interop::ivec::IVec;

#[repr(C)]
pub struct xrecord {
    ptr: *const u8,
    pub(crate) size_no_eol: usize,
    size_with_eol: usize,
}


impl xrecord {

    pub fn new(ptr: *const u8, size_no_eol: usize, size_with_eol: usize) -> Self {
        Self {
            ptr,
            size_no_eol,
            size_with_eol
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    pub fn size_no_eol(&self) -> usize {
        self.size_no_eol
    }

    pub fn size_with_eol(&self) -> usize {
        self.size_with_eol
    }

    pub fn as_ref(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.ptr, self.size_no_eol)
        }
    }

    pub fn eol_slice(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.ptr.add(self.size_no_eol),
                self.size_with_eol - self.size_no_eol
            )
        }
    }
}

#[repr(C)]
#[derive(Default)]
pub struct xdfile {
    pub minimal_perfect_hash: IVec<u64>,
    pub record: IVec<xrecord>,
}



impl xdfile {

    pub unsafe fn from_raw_mut<'a>(file: *mut xdfile, do_init: bool) -> &'a mut xdfile {
        if do_init {
            std::ptr::write(file, xdfile::default());
        }

        &mut *file
    }

}



