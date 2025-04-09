use std::hash::Hasher;
use std::ops::Range;
use xxhash_rust::xxh3::{xxh3_64, Xxh3Default};
use interop::ivec::IVec;
use crate::xdiff::{mmfile, XDF_IGNORE_CR_AT_EOL, XDF_WHITESPACE_FLAGS};
use crate::xdiffi::classic_diff;
use crate::xprepare::{safe_2way_prepare, safe_2way_slice, safe_3way_prepare};
use crate::xtypes::{parse_lines, xd2way, xd3way, xd_file_context, xdfile, xdpair, xrange, xrecord, FileContext};
use crate::xutils::{chunked_iter_equal, LineReader, MinimalPerfectHashBuilder, WhitespaceIter};

pub mod xtypes;
pub mod xutils;
pub mod xdiff;
pub mod xprepare;
pub mod maps;
#[cfg(test)]
pub(crate) mod mock;
pub(crate) mod xdiffi;
pub(crate) mod xhistogram;
pub(crate) mod xpatience;

#[no_mangle]
unsafe extern "C" fn xdl_mphb_new(max_unique_keys: usize, flags: u64) -> *mut libc::c_void {
    let a = Box::new(MinimalPerfectHashBuilder::new(max_unique_keys, flags));
    let b = Box::into_raw(a);
    b as *mut libc::c_void
}

#[no_mangle]
unsafe extern "C" fn xdl_mphb_process(mphb: *mut libc::c_void, file: *mut xdfile) {
    let mphb: &mut MinimalPerfectHashBuilder = &mut *(mphb as *mut MinimalPerfectHashBuilder);
    let file = xdfile::from_raw_mut(file);

    mphb.process(file);
}

#[no_mangle]
unsafe extern "C" fn xdl_mphb_finish(mphb: *mut libc::c_void) -> usize {
    let mphb = Box::from_raw(mphb as *mut MinimalPerfectHashBuilder);
    mphb.finish()
}


#[no_mangle]
unsafe extern "C" fn xdl_parse_lines(file: *const mmfile, record: *mut IVec<xrecord>) {
    let file = mmfile::from_raw(file);
    let record = IVec::from_raw_mut(record);

    parse_lines(file, record);
}

#[no_mangle]
unsafe extern "C" fn xdl_do_classic_diff(flags: u64, pair: *mut xdpair) -> i32 {
    let pair = xdpair::from_raw_mut(pair);

    classic_diff(flags, pair)
}


#[no_mangle]
unsafe extern "C" fn xdl_line_hash(ptr: *const u8, size: usize, flags: u64) -> u64 {
    let line = unsafe {
        std::slice::from_raw_parts(ptr, size)
    };
    if (flags & XDF_WHITESPACE_FLAGS) == 0 {
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

    if (flags & XDF_WHITESPACE_FLAGS) == 0 {
        line1 == line2
    } else {
        let lhs = WhitespaceIter::new(line1, flags);
        let rhs = WhitespaceIter::new(line2, flags);
        chunked_iter_equal(lhs, rhs)
    }
}


#[no_mangle]
unsafe extern "C" fn xdl_2way_slice(lhs: *mut xd_file_context, lhs_range: xrange,
                                    rhs: *mut xd_file_context, rhs_range: xrange,
                                    mph_size: usize, two_way: *mut xd2way) {
    /* initialize memory of two_way */
    std::ptr::write(two_way, xd2way::default());
    let two_way = &mut *two_way;

    let lhs = &mut *lhs;
    let lhs = FileContext::new(lhs);
    let rhs = &mut *rhs;
    let rhs = FileContext::new(rhs);

    safe_2way_slice(&lhs, lhs_range.into(), &rhs, rhs_range.into(), mph_size, two_way);
}


#[no_mangle]
unsafe extern "C" fn xdl_2way_prepare(mf1: *const mmfile, mf2: *const mmfile, flags: u64, two_way: *mut xd2way) {
    /* initialize memory of two_way */
    std::ptr::write(two_way, xd2way::default());
    let two_way = &mut *two_way;

    let file1 = mmfile::from_raw(mf1);
    let file2 = mmfile::from_raw(mf2);

    safe_2way_prepare(file1, file2, flags, two_way);
}


#[no_mangle]
unsafe extern "C" fn xdl_2way_free(two_way: *mut xd2way) {
    std::ptr::drop_in_place(two_way);
}


#[no_mangle]
unsafe extern "C" fn xdl_3way_prepare(
    base: *const mmfile, side1: *const mmfile, side2: *const mmfile,
    flags: u64, three_way: *mut xd3way
) {
    /* initialize memory of three_way */
    std::ptr::write(three_way, xd3way::default());
    let three_way = &mut *three_way;

    let base = mmfile::from_raw(base);
    let side1 = mmfile::from_raw(side1);
    let side2 = mmfile::from_raw(side2);

    safe_3way_prepare(base, side1, side2, flags, three_way);
}

#[no_mangle]
unsafe extern "C" fn xdl_3way_free(three_way: *mut xd3way) {
    std::ptr::drop_in_place(three_way);
}

