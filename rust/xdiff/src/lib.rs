use xxhash_rust::xxh3::xxh3_64;
use crate::xtypes::xdfile;

pub mod xrecord;
pub mod xtypes;


#[no_mangle]
unsafe extern "C" fn xdl_hash_records(file: *mut xdfile, flags: u64) {
    let file: &mut xdfile = &mut *file;

    for i in 0..file.record.len() {
        let mut rec = &mut file.record[i];
        rec.line_hash = xxh3_64(rec.as_ref());
    }
}

