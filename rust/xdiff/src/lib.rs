use std::io::Write;
use sha2::{Digest};
use interop::ivec::IVec;
use crate::xdfenv::{xdfenv_t, xdfile_t};
use crate::xdiff::{mmfile_t};
use crate::xtypes::Occurrence;

pub(crate) mod xutils;
pub(crate) mod xdiff;
pub(crate) mod xrecord;
pub(crate) mod xtypes;
pub(crate) mod xdfenv;
#[cfg(test)]
pub(crate) mod mock;


#[no_mangle]
unsafe extern "C" fn rust_xdl_prepare_ctx(_mf: *const mmfile_t, _xdf: *mut xdfile_t, flags: u64) -> i32 {
    let mf: &[u8] = mmfile_t::from_raw(_mf);
    let xdf: &mut xdfile_t = xdfile_t::from_raw(_xdf, true);

    *xdf = xdfile_t::new(mf, flags);

    0
}

#[no_mangle]
unsafe extern "C" fn rust_xdl_prepare_env(mf1: *const mmfile_t, mf2: *const mmfile_t, flags: u64, xe: *mut xdfenv_t, occurrence: *mut IVec<Occurrence>) -> i32 {
    let mf1 = mmfile_t::from_raw(mf1);
    let mf2 = mmfile_t::from_raw(mf2);
    let xe = xdfenv_t::from_raw(xe, true);
    let occurrence = IVec::from_raw_mut(occurrence);

    *xe = xdfenv_t::new(mf1, mf2, flags, occurrence);

    0
}
