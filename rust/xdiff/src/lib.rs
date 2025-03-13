use rayon::prelude::*;
use xxhash_rust::xxh3::xxh3_64;
use crate::xtypes::{xdfile, xrecord};

pub mod xtypes;


#[no_mangle]
unsafe extern "C" fn xdl_hash_records(file: *mut xdfile, flags: u64) {
    let file: &mut xdfile = &mut *file;

    let slice: &mut [xrecord] = file.record.as_mut_slice();
    slice.par_iter_mut().for_each(|rec| {
        rec.line_hash = xxh3_64(rec.as_ref());
    });

}

