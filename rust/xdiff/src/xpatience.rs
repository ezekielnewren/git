#![allow(non_camel_case_types)]

use interop::ivec::IVec;
use crate::xdiff::xpparam_t;
use crate::xtypes::xdpair;

const NON_UNIQUE: usize = usize::MAX;

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
    nr: usize,
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


/* The argument "pass" is 1 for the first file, 2 for the second. */
#[no_mangle]
unsafe extern "C" fn insert_record(
    xpp: *const xpparam_t, pair: *mut xdpair,
	line: usize, map: *mut hashmap, pass: i32
) {
    let xpp = &*xpp;
    let pair = xdpair::from_raw_mut(pair);
    let map = &mut *map;

	let mph_vec = &mut *if pass == 1 {
        pair.lhs.minimal_perfect_hash
    } else {
        pair.rhs.minimal_perfect_hash
    };
	let mph = mph_vec[line - 1];

	/*
	 * After xdl_prepare_env() (or more precisely, due to
	 * xdl_classify_record()), the "ha" member of the records (AKA lines)
	 * is _not_ the hash anymore, but a linearized version of it.  In
	 * other words, the "ha" member is guaranteed to start with 0 and
	 * the second record's ha can only be 0 or 1, etc.
	 *
	 * So we multiply ha by 2 in the hope that the hashing was
	 * "unique enough".
	 */
	let mut index = (mph << 1) as usize % map.entries.capacity();

	while map.entries[index].line1 != 0 {
		if map.entries[index].minimal_perfect_hash != mph {
            index += 1;
			if index >= map.entries.capacity() {
				index = 0;
            }
			continue;
		}
		if pass == 2 {
			map.has_matches = true;
        }
		if pass == 1 || map.entries[index].line2 != 0 {
			map.entries[index].line2 = NON_UNIQUE;
        } else {
			map.entries[index].line2 = line;
        }
		return;
	}
	if pass == 2 {
		return;
    }
	map.entries[index].line1 = line;
	map.entries[index].minimal_perfect_hash = mph;
	map.entries[index].anchor = is_anchor(xpp, (*pair.lhs.record)[line - 1].as_ptr());
	if map.first.is_null() {
		map.first = &mut map.entries[index];
    }
	if !map.last.is_null() {
        (*map.last).next = &mut map.entries[index];
		map.entries[index].previous = map.last;
	}
	map.last = &mut map.entries[index];
    map.nr += 1;
}

