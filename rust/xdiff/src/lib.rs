use std::hash::Hasher;
use std::io::Write;
use sha2::{Digest};
use xxhash_rust::xxh3::{xxh3_64, Xxh3};
use interop::ivec::IVec;
use crate::xdfenv::{xdfenv_t, xdfile_t};
use crate::xdiff::{mmfile_t, XDF_IGNORE_WHITESPACE_WITHIN};
use crate::xrecord::IterWhiteSpace;
use crate::xtypes::Occurrence;

pub(crate) mod xutils;
pub(crate) mod xdiff;
pub(crate) mod xrecord;
pub(crate) mod xtypes;
pub(crate) mod xdfenv;
#[cfg(test)]
pub(crate) mod mock;
mod mphb;

#[no_mangle]
unsafe extern "C" fn xdl_line_hash(ptr: *const u8, line_size_no_eol: usize, flags: u64) -> u64 {
    let slice = std::slice::from_raw_parts(ptr, line_size_no_eol);

    if (flags & XDF_IGNORE_WHITESPACE_WITHIN) == 0 {
        xxh3_64(slice)
    } else {
        let mut hasher = Xxh3::new();
        for run in IterWhiteSpace::new(slice, flags) {
            hasher.write(run);
        }
        hasher.finish()
    }
}

// #[no_mangle]
// unsafe extern "C" fn xdl_prepare_ctx(_mf: *const mmfile_t, _xdf: *mut xdfile_t, flags: u64) -> i32 {
//     let mf: &[u8] = mmfile_t::from_raw(_mf);
//     let xdf: &mut xdfile_t = xdfile_t::from_raw(_xdf, true);
//
//     *xdf = xdfile_t::new(mf, flags);
//
//     0
// }
//
// #[no_mangle]
// unsafe extern "C" fn xdl_construct_mph_and_occurrences(xe: *mut xdfenv_t, flags: u64, occurrence: *mut IVec<Occurrence>) {
//     let xe = xdfenv_t::from_raw(xe, false);
//     let occ = if occurrence.is_null() {
//         None
//     } else {
//         Some(IVec::from_raw_mut(occurrence))
//     };
//
//     xe.construct_mph_and_occurrences(occ, flags);
//
// }


// #[no_mangle]
// unsafe extern "C" fn rust_xdl_prepare_env(mf1: *const mmfile_t, mf2: *const mmfile_t, occ: *mut IVec<Occurrence>, flags: u64, xe: *mut xdfenv_t) -> i32 {
//     let mf1 = mmfile_t::from_raw(mf1);
//     let mf2 = mmfile_t::from_raw(mf2);
//     let occ = if occ.is_null() {
//         None
//     } else {
//         Some(IVec::from_raw_mut(occ))
//     };
//     let xe = xdfenv_t::from_raw(xe, true);
//
//
//     xe.xdf1 = xdfile_t::new(mf1, flags);
//     xe.xdf2 = xdfile_t::new(mf2, flags);
//
//     xe.construct_mph_and_occurrences(occ, flags);
//
//     std::ptr::write(xe, xdfenv_t::new(mf1, mf2, flags));
//
//     0
// }

// #[no_mangle]
// unsafe extern "C" fn xdl_free_env(xe: *mut xdfenv_t) {
//     std::ptr::drop_in_place(xe);
// }
