use std::hash::Hasher;
use xxhash_rust::xxh3::{xxh3_64, Xxh3Default};
use crate::xdiff::{mmfile, XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE_WITHIN};
use crate::xtypes::xdfile;
use crate::xutils::{chunked_iter_equal, LineReader, WhitespaceIter};

pub mod xtypes;
pub mod xutils;
pub mod xdiff;


#[no_mangle]
unsafe extern "C" fn xdl_file_prepare(mf: *const mmfile, flags: u64, file: *mut xdfile) {
    let mf = mmfile::from_raw(mf);
    let file = xdfile::from_raw_mut(file, true);

    for record in LineReader::new(mf) {
        file.record.push(record);
    }
    file.record.shrink_to_fit();

    if (flags & XDF_IGNORE_CR_AT_EOL) != 0 {
        for rec in file.record.as_mut_slice() {
            if rec.size_no_eol > 0 && rec.as_ref()[rec.size_no_eol - 1] == b'\r' {
                rec.size_no_eol -= 1;
            }
        }
    }
}

#[no_mangle]
unsafe extern "C" fn xdl_line_hash(ptr: *const u8, size_no_eol: usize, flags: u64) -> u64 {
    let line = unsafe {
        std::slice::from_raw_parts(ptr, size_no_eol)
    };
    if (flags & XDF_IGNORE_WHITESPACE_WITHIN) == 0 {
        xxh3_64(line)
    } else {
        let mut state = Xxh3Default::new();
        for run in WhitespaceIter::new(line, flags) {
            state.write(run);
        }
        state.finish()
    }
}

#[no_mangle]
unsafe extern "C" fn xdl_line_equal(line1: *const u8, size1: usize, line2: *const u8, size2: usize, flags: u64) -> bool {
    let line1 = unsafe {
        std::slice::from_raw_parts(line1, size1)
    };
    let line2 = unsafe {
        std::slice::from_raw_parts(line2, size2)
    };

    let lhs = WhitespaceIter::new(line1, flags);
    let rhs = WhitespaceIter::new(line2, flags);
    chunked_iter_equal(lhs, rhs)
}


