#![allow(non_camel_case_types)]

use interop::ivec::IVec;
use crate::xdiff::XDF_IGNORE_CR_AT_EOL;
use crate::xutils::LineReader;

#[repr(C)]
pub struct xrecord {
    ptr: *const u8,
    size_no_eol: usize,
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

    /// Length of the line excluding end of line bytes.
    pub fn len(&self) -> usize {
        self.size_no_eol
    }

    pub fn as_ref(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.ptr, self.size_no_eol)
        }
    }

    /// Returns a slice of the end of line bytes.
    pub fn eol(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self.ptr.add(self.size_no_eol),
                self.size_with_eol - self.size_no_eol
            )
        }
    }
}


pub fn parse_lines(file: &[u8], ignore_cr_at_eol: bool, line_vec: &mut IVec<xrecord>) {
    for record in LineReader::new(file) {
        line_vec.push(record);
    }
    line_vec.shrink_to_fit();

    if ignore_cr_at_eol {
        for rec in line_vec.as_mut_slice() {
            if rec.size_no_eol > 0 && rec.as_ref()[rec.size_no_eol - 1] == b'\r' {
                rec.size_no_eol -= 1;
            }
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

#[repr(C)]
pub struct xd_file_context {
    pub minimal_perfect_hash: *mut IVec<u64>,
    pub record: *mut IVec<xrecord>,
    pub consider: IVec<u8>,
    pub rindex: IVec<usize>,
}

impl Default for xd_file_context {
    fn default() -> Self {
        Self {
            minimal_perfect_hash: std::ptr::null_mut(),
            record: std::ptr::null_mut(),
            consider: IVec::default(),
            rindex: IVec::default(),
        }
    }
}


pub struct FileContext<'a> {
    pub minimal_perfect_hash: &'a IVec<u64>,
    pub record: &'a IVec<xrecord>,
    pub consider: &'a mut IVec<u8>,
    pub rindex: &'a mut IVec<usize>,
}


impl<'a> FileContext<'a> {
    pub fn new(ctx: &'a mut xd_file_context) -> Self {
        Self {
            minimal_perfect_hash: unsafe { &*ctx.minimal_perfect_hash },
            record: unsafe { &*ctx.record },
            consider: &mut ctx.consider,
            rindex: &mut ctx.rindex,
        }
    }
}


#[repr(C)]
#[derive(Default)]
pub struct xdpair {
    pub lhs: xd_file_context,
    pub rhs: xd_file_context,
    pub delta_start: usize,
    pub delta_end: usize,
    pub minimal_perfect_hash_size: usize,
}

impl xdpair {

    pub unsafe fn from_raw_mut<'a>(pair: *mut xdpair, do_init: bool) -> &'a mut xdpair {
        if do_init {
            std::ptr::write(pair, xdpair::default());
        }

        &mut *pair
    }

}

