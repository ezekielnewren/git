use std::io::Write;
use sha2::{Digest};
use interop::ivec::IVec;
use crate::xdfenv::{xdfenv_t, xdfile_t};
use crate::xdiff::{mmfile_t};
use crate::xhistogram::xdl_do_histogram_diff;
use crate::xtypes::Occurrence;

pub(crate) mod xutils;
pub(crate) mod xdiff;
pub(crate) mod xrecord;
pub(crate) mod xtypes;
pub(crate) mod xdfenv;
#[cfg(test)]
pub(crate) mod mock;
mod xhistogram;

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

#[no_mangle]
unsafe extern "C" fn rust_xdl_do_histogram_diff(env: *mut xdfenv_t, flags: u64) -> i32 {
    let env = xdfenv_t::from_raw(env, false);

    let mf1 = env.xdf1.as_ref();
    let mf2 = env.xdf2.as_ref();

    let mut copy = xdfenv_t::new(mf1, mf2, flags, &mut Default::default());


    let result = xdl_do_histogram_diff(&mut copy);
    assert_eq!(env.xdf1.record, copy.xdf1.record);
    assert_eq!(env.xdf1.minimal_perfect_hash, copy.xdf1.minimal_perfect_hash);
    assert_eq!(env.xdf1.rchg_vec, copy.xdf1.rchg_vec);
    assert_eq!(env.xdf1.rindex, copy.xdf1.rindex);

    assert_eq!(env.xdf2.record, copy.xdf2.record);
    assert_eq!(env.xdf2.minimal_perfect_hash, copy.xdf2.minimal_perfect_hash);
    assert_eq!(env.xdf2.rchg_vec, copy.xdf2.rchg_vec);
    assert_eq!(env.xdf2.rindex, copy.xdf2.rindex);

    result
}

