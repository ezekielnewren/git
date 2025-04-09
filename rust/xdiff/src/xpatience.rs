#![allow(non_camel_case_types)]

use std::marker::PhantomData;
use std::ops::Range;
use crate::get_file_context;
use crate::xdiff::*;
use crate::xdiffi::classic_diff_with_range;
use crate::xtypes::*;

const NON_UNIQUE: usize = usize::MAX;

#[repr(C)]
#[derive(Clone)]
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


struct EntryNextIter<'a> {
    cur: *mut entry,
    _marker: PhantomData<&'a entry>,
}

impl<'a> EntryNextIter<'a> {
    fn new(start: *mut entry) -> Self {
        Self {
            cur: start,
            _marker: PhantomData,
        }
    }
}

impl<'a> Iterator for EntryNextIter<'a> {
    type Item = &'a mut entry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur.is_null() {
            return None;
        }

        let t = self.cur;
        self.cur = unsafe { (*self.cur).next };
        Some(unsafe { &mut *t })
    }
}


/*
 * This is a hash mapping from line hash to line numbers in the first and
 * second file.
 */
#[repr(C)]
struct hashmap {
    nr: usize,
	entries: Vec<entry>,
	first: *mut entry,
    last: *mut entry,
	/* were common records found? */
	has_matches: bool,
}

impl Default for hashmap {
	fn default() -> Self {
		Self {
			nr: 0,
			entries: Default::default(),
			first: std::ptr::null_mut(),
			last: std::ptr::null_mut(),
			has_matches: false,
		}
	}
}

fn is_anchor(xpp: &xpparam_t, line: &[u8]) -> bool {
    for i in 0..xpp.anchors_nr {
		let anchor = unsafe {
			let t = *xpp.anchors.add(i);
			let len = libc::strlen(t);
			std::slice::from_raw_parts(t as *const u8, len)
		};
		if line.starts_with(anchor) {
			return true;
		}
    }

    false
}


/* The argument "pass" is 1 for the first file, 2 for the second. */
fn insert_record(
    xpp: &xpparam_t, pair: &mut xdpair,
	line: usize, map: &mut hashmap, pass: i32
) {
	let (lhs, rhs) = get_file_context!(pair);

	let mph_vec = if pass == 1 {
        lhs.minimal_perfect_hash
    } else {
        rhs.minimal_perfect_hash
    };

	let mut mph = mph_vec[line - LINE_SHIFT] as usize;

	while map.entries[mph].line1 != 0 {
		if map.entries[mph].minimal_perfect_hash != mph as u64 {
            mph += 1;
			if mph >= map.entries.capacity() {
				panic!("mph went out of bounds");
				// mph = 0;
            }
			continue;
		}
		if pass == 2 {
			map.has_matches = true;
        }
		if pass == 1 || map.entries[mph].line2 != 0 {
			map.entries[mph].line2 = NON_UNIQUE;
        } else {
			map.entries[mph].line2 = line;
        }
		return;
	}
	if pass == 2 {
		return;
    }
	map.entries[mph].line1 = line;
	map.entries[mph].minimal_perfect_hash = mph as u64;
	map.entries[mph].anchor = is_anchor(xpp, lhs.record[line - 1].as_ref());
	if map.first.is_null() {
		map.first = &mut map.entries[mph];
    }
	if !map.last.is_null() {
        unsafe { (*map.last).next = &mut map.entries[mph] };
		map.entries[mph].previous = map.last;
	}
	map.last = &mut map.entries[mph];
    map.nr += 1;
}


/*
 * This function has to be called for each recursion into the inter-hunk
 * parts, as previously non-unique lines can become unique when being
 * restricted to a smaller part of the files.
 *
 * It is assumed that env has been prepared using xdl_prepare().
 */
fn fill_hashmap(
    xpp: &xpparam_t, pair: &mut xdpair,
    result: &mut hashmap,
    range1: Range<usize>, range2: Range<usize>
) -> i32 {
	/* We know exactly how large we want the hash map */
    result.entries = vec![entry::default(); std::cmp::max(range1.len(), pair.minimal_perfect_hash_size)];

	/* First, fill with entries from the first file */
    for i in range1 {
        insert_record(xpp, pair, i, result, 1);
    }

    /* Then search for matches in the second file */
    for i in range2 {
        insert_record(xpp, pair, i, result, 2);
    }

	0
}


/*
 * Find the longest sequence with a smaller last element (meaning a smaller
 * line2, as we construct the sequence with entries ordered by line1).
 */
fn binary_search(sequence: &mut Vec<*mut entry>, longest: isize,
		entry: &mut entry
) -> isize {
	let mut left: isize = -1isize;
	let mut right: isize = longest;

	while left + 1 < right {
		let middle = left + (right - left) / 2;
		/* by construction, no two entries can be equal */
		if unsafe { (*sequence[middle as usize]).line2 } > entry.line2 {
			right = middle;
        } else {
			left = middle;
        }
	}
	/* return the index in "sequence", _not_ the sequence length */
	left
}


/*
 * The idea is to start with the list of common unique lines sorted by
 * the order in file1.  For each of these pairs, the longest (partial)
 * sequence whose last element's line2 is smaller is determined.
 *
 * For efficiency, the sequences are kept in a list containing exactly one
 * item per sequence length: the sequence with the smallest last
 * element (in terms of line2).
 */
fn find_longest_common_sequence(map: &mut hashmap, res: &mut *mut entry) -> i32 {
    let mut sequence: Vec<*mut entry> = vec![std::ptr::null_mut(); map.entries.len()];

	let mut longest = 0isize;

	/*
	 * If not -1, this entry in sequence must never be overridden.
	 * Therefore, overriding entries before this has no effect, so
	 * do not do that either.
	 */
	let mut anchor_i = -1;

    for e in EntryNextIter::new(map.first) {
		if e.line2 == 0 || e.line2 == NON_UNIQUE {
			continue;
		}
		let mut i = binary_search(&mut sequence, longest, e);
		if i < 0 {
			e.previous = std::ptr::null_mut();
		} else {
			e.previous = sequence[i as usize];
		}
		i += 1;
		if i <= anchor_i {
			continue;
		}
		sequence[i as usize] = e;
		if e.anchor {
			anchor_i = i;
			longest = anchor_i + 1;
		} else if i == longest {
			longest += 1;
		}
	}

	/* No common unique lines were found */
	if longest == 0 {
		*res = std::ptr::null_mut();
		return 0;
	}

	/* Iterate starting at the last element, adjusting the "next" members */
	let mut e = sequence[(longest - 1) as usize];
	unsafe {
		(*e).next = std::ptr::null_mut();
		while !(*e).previous.is_null() {
			(*(*e).previous).next = e;
			e = (*e).previous;
		}
	}
	*res = e;

	0
}


fn walk_common_sequence(
	xpp: &xpparam_t, pair: &mut xdpair, mut first: *mut entry,
	mut range1: Range<usize>, mut range2: Range<usize>
) -> i32 {
	let mut next1;
    let mut next2;

	loop {
		/* Try to grow the line ranges of common lines */
		if !first.is_null() {
			unsafe {
				next1 = (*first).line1;
				next2 = (*first).line2;
			}
			while next1 > range1.start && next2 > range2.start &&
				pair.equal_by_line_number(next1 - 1, next2 - 1) {
				next1 -= 1;
				next2 -= 1;
			}
		} else {
			next1 = range1.end;
			next2 = range2.end;
		}
		while range1.start < next1 && range2.start < next2 &&
            pair.equal_by_line_number(range1.start, range2.start) {
			range1.start += 1;
			range2.start += 1;
		}

		/* Recurse */
		if next1 > range1.start || next2 > range2.start {
			if patience_diff(xpp, pair,
					range1.start..next1,
					range2.start..next2) != 0 {
				return -1;
			}
		}

		if first.is_null() {
			return 0;
        }

		unsafe {
			while !(*first).next.is_null() &&
				(*(*first).next).line1 == (*first).line1 + 1 &&
				(*(*first).next).line2 == (*first).line2 + 1 {
				first = (*first).next;
			}

			range1.start = (*first).line1 + 1;
			range2.start = (*first).line2 + 1;

			first = (*first).next;
		}
	}
}


fn patience_diff(xpp: &xpparam_t, pair: &mut xdpair,
		range1: Range<usize>, range2: Range<usize>
) -> i32 {
	let mut map = hashmap::default();
	let mut result;

	/* trivial case: one side is empty */
	if range1.len() == 0 {
		for i in range2 {
			pair.rhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
		}
		return 0;
	} else if range2.len() == 0 {
		for i in range1 {
			pair.lhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
		}
		return 0;
	}

	if fill_hashmap(xpp, pair, &mut map, range1.clone(), range2.clone()) != 0 {
		return -1;
	}

	/* are there any matching lines at all? */
	if !map.has_matches {
		for i in range1 {
			pair.lhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
		}
		for i in range2 {
			pair.rhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
		}
		return 0;
	}

	let mut first = std::ptr::null_mut();
	result = find_longest_common_sequence(&mut map, &mut first);
	if result != 0 {
		return result;
	}
	if !first.is_null() {
		result = walk_common_sequence(xpp, pair, first, range1, range2);
	} else {
		result = classic_diff_with_range(xpp.flags, pair, range1, range2);
	}

	result
}


#[no_mangle]
pub(crate) unsafe extern "C" fn xdl_do_patience_diff(xpp: *const xpparam_t, pair: *mut xdpair) -> i32 {
	let xpp = &*xpp;
	let pair = xdpair::from_raw_mut(pair);
	let (lhs, rhs) = get_file_context!(pair);

	let range1 = LINE_SHIFT..LINE_SHIFT + lhs.record.len();
	let range2 = LINE_SHIFT..LINE_SHIFT + rhs.record.len();

	drop(lhs);
	drop(rhs);

	patience_diff(xpp, pair, range1, range2)
}

