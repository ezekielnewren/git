use crate::xutils::xdl_hash_record;

pub mod xutils;
pub mod xdiff;



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
    *data = *data.add(with_eol);
    line_hash
}


