#![allow(non_camel_case_types)]

use std::hash::Hasher;
use interop::ivec::IVec;
use crate::maps::HashEq;
use crate::xdiff::{XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE_WITHIN};
use crate::xutils::{chunked_iter_equal, LineReader, WhitespaceIter};

#[repr(C)]
pub struct xrecord {
    ptr: *const u8,
    size_no_eol: usize,
    size_with_eol: usize,
}


impl Clone for xrecord {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            size_no_eol: self.size_no_eol,
            size_with_eol: self.size_with_eol,
        }
    }
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

pub struct xrecord_he {
    flags: u64
}

impl xrecord_he {
    pub(crate) fn new(flags: u64) -> Self {
        Self {
            flags,
        }
    }
}
impl HashEq<xrecord> for xrecord_he {
    fn hash(&self, key: &xrecord) -> u64 {
        if (self.flags & XDF_IGNORE_WHITESPACE_WITHIN) == 0 {
            xxhash_rust::xxh3::xxh3_64(key.as_ref())
        } else {
            let mut state = xxhash_rust::xxh3::Xxh3::default();
            for run in WhitespaceIter::new(key.as_ref(), self.flags) {
                state.write(run);
            }
            state.finish()
        }
    }

    fn eq(&self, lhs: &xrecord, rhs: &xrecord) -> bool {
        if (self.flags & XDF_IGNORE_WHITESPACE_WITHIN) == 0 {
            lhs.as_ref() == rhs.as_ref()
        } else {
            let lhs = WhitespaceIter::new(lhs.as_ref(), self.flags);
            let rhs = WhitespaceIter::new(rhs.as_ref(), self.flags);
            chunked_iter_equal(lhs, rhs)
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

    pub unsafe fn from_raw_mut<'a>(file: *mut xdfile) -> &'a mut xdfile {
        if file.is_null() {
            panic!("null pointer");
        }

        &mut *file
    }

    pub unsafe fn from_raw<'a>(file: *mut xdfile) -> &'a xdfile {
        if file.is_null() {
            panic!("null pointer");
        }

        &*file
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

    pub unsafe fn from_raw_mut<'a>(pair: *mut xdpair) -> &'a mut xdpair {
        if pair.is_null() {
            panic!("null pointer");
        }
        &mut *pair
    }

}


#[macro_export]
macro_rules! get_file_context {
    ($pair:expr) => {
        (
            FileContext::new(&mut $pair.lhs),
	        FileContext::new(&mut $pair.rhs)
        )
    }
}


#[repr(C)]
pub struct xd2way {
    pub lhs: xdfile,
    pub rhs: xdfile,
    pub pair: xdpair,
	pub minimal_perfect_hash_size: usize,
}

#[repr(C)]
pub struct xd3way {
    pub base: xdfile,
    pub side1: xdfile,
    pub side2: xdfile,
    pub pair1: xdpair,
    pub pair2: xdpair,
	pub minimal_perfect_hash_size: usize,
}


