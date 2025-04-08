#![allow(non_camel_case_types)]

use std::marker::PhantomData;
use interop::ivec::IVec;
use crate::get_file_context;
use crate::xdiff::*;
use crate::xtypes::*;
use crate::xdiffi::classic_diff_with_range;

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
	entries: IVec<entry>,
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
	map.entries[index].anchor = is_anchor(xpp, lhs.record[line - 1].as_ref());
	if map.first.is_null() {
		map.first = &mut map.entries[index];
    }
	if !map.last.is_null() {
        unsafe { (*map.last).next = &mut map.entries[index] };
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
fn fill_hashmap(
    xpp: &xpparam_t, pair: &mut xdpair,
    result: &mut hashmap,
    line1: usize, count1: usize, line2: usize, count2: usize
) -> i32 {
	/* We know exactly how large we want the hash map */
    result.entries = unsafe { IVec::zero(count1 * 2) };

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


/*
 * The idea is to start with the list of common unique lines sorted by
 * the order in file1.  For each of these pairs, the longest (partial)
 * sequence whose last element's line2 is smaller is determined.
 *
 * For efficiency, the sequences are kept in a list containing exactly one
 * item per sequence length: the sequence with the smallest last
 * element (in terms of line2).
 */
#[no_mangle]
unsafe extern "C" fn find_longest_common_sequence(map: *mut hashmap, res: *mut *mut entry) -> i32 {
    let map = &mut *map;

    let mut sequence = IVec::zero(map.entries.len());

	let mut longest = 0isize;

	/*
	 * If not -1, this entry in sequence must never be overridden.
	 * Therefore, overriding entries before this has no effect, so
	 * do not do that either.
	 */
	let mut anchor_i = -1;

    for entry in EntryNextIter::new(map.first) {
		if entry.line2 == 0 || entry.line2 == NON_UNIQUE {
			continue;
		}
		let mut i = binary_search(&mut sequence, longest, entry);
		if i < 0 {
			entry.previous = std::ptr::null_mut();
		} else {
			entry.previous = sequence[i as usize];
		}
		i += 1;
		if i <= anchor_i {
			continue;
		}
		sequence[i as usize] = entry;
		if entry.anchor {
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
	let mut entry = sequence[(longest - 1) as usize];
    (*entry).next = std::ptr::null_mut();
	while !(*entry).previous.is_null() {
        (*(*entry).previous).next = entry;
		entry = (*entry).previous;
	}
	*res = entry;

	0
}


#[no_mangle]
unsafe extern "C" fn walk_common_sequence(xpp: *const xpparam_t, pair: *mut xdpair,
                                          mut first: *mut entry,
                                          mut line1: usize, count1: usize, mut line2: usize, count2: usize
) -> i32 {
	let pair = xdpair::from_raw_mut(pair);

	let end1 = line1 + count1;
    let end2 = line2 + count2;
	let mut next1;
    let mut next2;

	loop {
		/* Try to grow the line ranges of common lines */
		if !first.is_null() {
			next1 = (*first).line1;
			next2 = (*first).line2;
			while next1 > line1 && next2 > line2 &&
				pair.equal_by_line_number(next1 - 1, next2 - 1) {
				next1 -= 1;
				next2 -= 1;
			}
		} else {
			next1 = end1;
			next2 = end2;
		}
		while line1 < next1 && line2 < next2 &&
            pair.equal_by_line_number(line1, line2) {
			line1 += 1;
			line2 += 1;
		}

		/* Recurse */
		if next1 > line1 || next2 > line2 {
			if patience_diff(xpp, pair,
					line1, next1 - line1,
					line2, next2 - line2) != 0 {
				return -1;
			}
		}

		if first.is_null() {
			return 0;
        }

		while !(*first).next.is_null() &&
			(*(*first).next).line1 == (*first).line1 + 1 &&
			(*(*first).next).line2 == (*first).line2 + 1 {
			first = (*first).next;
        }

		line1 = (*first).line1 + 1;
		line2 = (*first).line2 + 1;

		first = (*first).next;
	}
}


#[no_mangle]
unsafe extern "C" fn patience_diff(xpp: *const xpparam_t, pair: *mut xdpair,
		line1: usize, count1: usize, line2: usize, count2: usize
) -> i32 {
	let xpp = &*xpp;
	let pair = &mut *pair;

	let mut map = hashmap::default();
	let mut result;

	/* trivial case: one side is empty */
	if count1 == 0 {
		for i in line2..line2 + count2 {
			pair.rhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
		}
		return 0;
	} else if count2 == 0 {
		for i in line1..line1 + count1 {
			pair.lhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
		}
		return 0;
	}

	if fill_hashmap(xpp, pair, &mut map,
			line1, count1, line2, count2) != 0 {
		return -1;
	}

	/* are there any matching lines at all? */
	if !map.has_matches {
		for i in line1..line1 + count1 {
			pair.lhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
		}
		for i in line2..line2 + count2 {
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
		result = walk_common_sequence(xpp, pair, first,
			line1, count1, line2, count2);
	} else {
		result = classic_diff_with_range(xpp.flags, pair,
			line1..line1 + count1, line2..line2 + count2);
	}

	result
}


#[no_mangle]
unsafe extern "C" fn xdl_do_patience_diff(xpp: *const xpparam_t, pair: *mut xdpair) -> i32 {
	let xpp = &*xpp;
	let pair = xdpair::from_raw_mut(pair);

	patience_diff(xpp, pair, LINE_SHIFT, (*pair.lhs.record).len(), LINE_SHIFT, (*pair.rhs.record).len())
}

