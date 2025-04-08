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


/*
 * This function has to be called for each recursion into the inter-hunk
 * parts, as previously non-unique lines can become unique when being
 * restricted to a smaller part of the files.
 *
 * It is assumed that env has been prepared using xdl_prepare().
 */
#[no_mangle]
unsafe extern "C" fn fill_hashmap(
    xpp: *const xpparam_t, pair: *mut xdpair,
    result: *mut hashmap,
    line1: usize, count1: usize, line2: usize, count2: usize
) -> i32 {
    let xpp = &*xpp;
    let pair = xdpair::from_raw_mut(pair);
    let result = &mut *result;

	/* We know exactly how large we want the hash map */
    result.entries = IVec::zero(count1 * 2);

	/* First, fill with entries from the first file */
    for i in line1..line1 + count1 {
        insert_record(xpp, pair, i, result, 1);
    }

    /* Then search for matches in the second file */
    for i in line2..line2 + count2 {
        insert_record(xpp, pair, i, result, 2);
    }

	0
}


/*
 * Find the longest sequence with a smaller last element (meaning a smaller
 * line2, as we construct the sequence with entries ordered by line1).
 */
#[no_mangle]
unsafe extern "C" fn binary_search(sequence: *mut IVec<*mut entry>, longest: isize,
		entry: *mut entry
) -> isize {
    let sequence = IVec::from_raw_mut(sequence);
    let entry = &mut *entry;

	let mut left: isize = -1isize;
	let mut right: isize = longest;

	while left + 1 < right {
		let middle = left + (right - left) / 2;
		/* by construction, no two entries can be equal */
		if (*sequence[middle as usize]).line2 > entry.line2 {
			right = middle;
        } else {
			left = middle;
        }
	}
	/* return the index in "sequence", _not_ the sequence length */
	left
}


