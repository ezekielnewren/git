use crate::xdiff::XDF_WHITESPACE_FLAGS;
use crate::xutils::xdl_hash_record;

pub mod xutils;
pub mod xdiff;



#[no_mangle]
unsafe extern "C" fn rust_xdl_hash_record(
    _data: *mut *const libc::c_char,
    top: *const libc::c_char,
    flags: libc::c_long
) -> u64 {
    let data: &mut *const u8 = &mut (*_data as *const u8);
    xdl_hash_record(data, top as *const u8, flags as u64)
}

// #[no_mangle]
// unsafe extern "C" fn rust_xdl_hash_record(
//     data: *mut *const libc::c_char,
//     top: *const libc::c_char,
//     flags: libc::c_long
// ) -> u64 {
//     let mut hash = 5381u64;
//     // let mut ptr = *data;
//
//     // if (flags as u64 & XDF_WHITESPACE_FLAGS) != 0 {
//     //     return crate::xutils::xdl_hash_record_with_whitespace(data, top, flags);
//     // }
//
//     let slice = unsafe {
//         std::slice::from_raw_parts(*data as *const u8, top.sub(*data as usize) as usize)
//     };
//
//     let mut off = 0;
//     for v in slice {
//         println!("{}", off);
//         if *v == b'\n' {
//             break;
//         }
//         let t = hash.overflowing_mul(33);
//         hash = t.0 ^ *v as u64;
//         off += 1;
//     }
//     if off < slice.len() {
//         *data = (*data).add(off + 1);
//     } else {
//         *data = (*data).add(off + 1);
//     }
//
//     // while ptr < top && *ptr as u8 != b'\n' {
//     //     hash += hash << 5;
//     //     hash ^= *ptr as u64;
//     //
//     //     ptr = ptr.add(1);
//     // }
//     // *data = if ptr < top { ptr.add(1) } else { ptr };
//
//     hash
// }

