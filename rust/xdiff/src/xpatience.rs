#![allow(non_camel_case_types)]

use interop::ivec::IVec;
use crate::xdiff::xpparam_t;

#[repr(C)]
struct entry {
    minimal_perfect_hash: u64,
    /*
     * 0 = unused entry, 1 = first line, 2 = second, etc.
     * line2 is NON_UNIQUE if the line is not unique
     * in either the first or the second file.
     */
    line1: usize,
    line2: usize,

    /*
     * "next" & "previous" are used for the longest common
     * sequence;
     * initially, "next" reflects only the order in file1.
     */
    next: *mut entry,
    previous: *mut entry,

    /*
     * If 1, this entry can serve as an anchor. See
     * Documentation/diff-options.adoc for more information.
     */
    anchor: bool,
}

impl Default for entry {
    fn default() -> Self {
        Self {
            minimal_perfect_hash: 0,
            line1: 0,
            line2: 0,
            next: std::ptr::null_mut(),
            previous: std::ptr::null_mut(),
            anchor: true,
        }
    }
}


/*
 * This is a hash mapping from line hash to line numbers in the first and
 * second file.
 */
#[repr(C)]
struct hashmap {
	entries: IVec<entry>,
	first: *mut entry,
    last: *mut entry,
	/* were common records found? */
	has_matches: bool,
}

#[no_mangle]
unsafe extern "C" fn is_anchor(xpp: *const xpparam_t, line: *const u8) -> bool {
    let xpp = &*xpp;
    for i in 0..xpp.anchors_nr {
        if 0 == libc::strncmp(line as *const libc::c_char, *xpp.anchors.add(i), libc::strlen(*xpp.anchors.add(i))) {
            return true;
        }
    }

    false
}
