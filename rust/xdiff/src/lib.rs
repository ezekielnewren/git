use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::Path;
use sha2::{Digest, Sha256};
use crate::xdfenv::xdfile_t;
use crate::xdiff::{mmfile_t};
use crate::xprepare::xdl_prepare_ctx;
use crate::xutils::xdl_hash_record;

pub(crate) mod xutils;
pub(crate) mod xdiff;
pub(crate) mod xprepare;
pub(crate) mod xrecord;
pub(crate) mod xtypes;
pub(crate) mod xdfenv;



#[no_mangle]
unsafe extern "C" fn rust_xdl_hash_record(
    data: *mut *const libc::c_char,
    top: *const libc::c_char,
    flags: libc::c_long
) -> u64 {
    let slice: &[u8] = unsafe {
        std::slice::from_raw_parts(*data as *const u8, top.sub(*data as usize) as usize)
    };
    let (line_hash, with_eol) = xdl_hash_record(slice, flags as u64);
    *data = (*data).add(with_eol);
    line_hash
}



#[no_mangle]
extern "C" fn rust_dump_file(ptr: *const u8, size: usize) {
    let data = unsafe {
        std::slice::from_raw_parts(ptr, size)
    };

    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();

    let dir_path = "/tmp/gitdump";
    let t = format!("{}/{:x}.txt", dir_path, hash);
    let file_path = Path::new(t.as_str());

    create_dir_all(dir_path).unwrap();

    if !file_path.exists() {
        let mut file = File::create(&file_path).unwrap();
        file.write_all(data).unwrap();
        drop(file);  // i.e. close the file
    }
}



#[no_mangle]
unsafe extern "C" fn rust_xdl_prepare_ctx(_mf: *const mmfile_t, _xdf: *mut xdfile_t, flags: u64) -> i32 {
    let mf: &[u8] = mmfile_t::from_raw(_mf);
    let xdf: &mut xdfile_t = xdfile_t::from_raw(_xdf, true);

	xdl_prepare_ctx(mf, xdf, flags);

    0
}

