use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::Path;
use sha2::{Digest, Sha256};
use interop::ivec::IVec;
use crate::xdfenv::xdfile_t;
use crate::xdiff::{mmfile_t};
use crate::xrecord::xrecord_t;
use crate::xutils::{line_length, xdl_hash_record, LineReader};

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

