use std::hash::Hasher;
use xxhash_rust::xxh3::{xxh3_64, Xxh3Default};
use crate::xdiff::{mmfile, XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE_WITHIN, XDF_WHITESPACE_FLAGS};
use crate::xtypes::{parse_lines, xdfile};
use crate::xutils::{chunked_iter_equal, LineReader, WhitespaceIter};

pub mod xtypes;
pub mod xutils;
pub mod xdiff;
mod xprepare;

#[no_mangle]
unsafe extern "C" fn xdl_file_prepare(mf: *const mmfile, flags: u64, file: *mut xdfile) {
    let mf = mmfile::from_raw(mf);
    let file = xdfile::from_raw_mut(file, true);

    parse_lines(mf, (flags & XDF_IGNORE_CR_AT_EOL) != 0, &mut file.record);
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

    if (flags & XDF_IGNORE_WHITESPACE_WITHIN) == 0 {
        line1 == line2
    } else {
        let lhs = WhitespaceIter::new(line1, flags);
        let rhs = WhitespaceIter::new(line2, flags);
        chunked_iter_equal(lhs, rhs)
    }
}


