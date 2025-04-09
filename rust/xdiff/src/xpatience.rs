#![allow(non_camel_case_types)]

use std::marker::PhantomData;
use std::ops::Range;
use crate::get_file_context;
use crate::maps::{DefaultHashEq, FixedMap};
use crate::xdiff::*;
use crate::xdiffi::classic_diff_with_range;
use crate::xtypes::*;

const NON_UNIQUE: usize = usize::MAX;

#[repr(C)]
#[derive(Clone)]
struct Node {
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
    next: *mut Node,
    previous: *mut Node,

    /*
     * If 1, this entry can serve as an anchor. See
     * Documentation/diff-options.adoc for more information.
     */
    anchor: bool,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            line1: 0,
            line2: 0,
            next: std::ptr::null_mut(),
            previous: std::ptr::null_mut(),
            anchor: true,
        }
    }
}


struct EntryNextIter<'a> {
    cur: *mut Node,
    _marker: PhantomData<&'a Node>,
}

impl<'a> EntryNextIter<'a> {
    fn new(start: *mut Node) -> Self {
        Self {
            cur: start,
            _marker: PhantomData,
        }
    }
}

impl<'a> Iterator for EntryNextIter<'a> {
    type Item = &'a mut Node;

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
struct PatienceContext<'a> {
	entries: FixedMap<'a, u64, Node, DefaultHashEq<u64>>,
	first: *mut Node,
    last: *mut Node,
	/* were common records found? */
	has_matches: bool,
}

impl<'a> Default for PatienceContext<'a> {
	fn default() -> Self {
		Self {
			entries: FixedMap::with_capacity(0),
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
	line: usize, map: &mut PatienceContext, pass: i32
) {
	let (lhs, rhs) = get_file_context!(pair);

	let mph_vec = if pass == 1 {
        lhs.minimal_perfect_hash
    } else {
        rhs.minimal_perfect_hash
    };

	let mph = mph_vec[line - LINE_SHIFT];

	if let Some(node) = map.entries.get_mut(&mph) {
		if pass == 2 {
			map.has_matches = true;
        }
		if pass == 1 || node.line2 != 0 {
			node.line2 = NON_UNIQUE;
        } else {
			node.line2 = line;
        }
		return;
	}
	if pass == 2 {
		return;
    }
	let node = map.entries.insert(mph, Node {
		line1: line,
		line2: 0,
		next: std::ptr::null_mut(),
		previous: std::ptr::null_mut(),
		anchor: is_anchor(xpp, lhs.record[line - LINE_SHIFT].as_ref()),
	});
	if map.first.is_null() {
		map.first = node;
    }
	if !map.last.is_null() {
        unsafe { (*map.last).next = node };
		node.previous = map.last;
	}
	map.last = node;
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
	result: &mut PatienceContext,
	range1: Range<usize>, range2: Range<usize>
) -> i32 {
	/* We know exactly how large we want the hash map */
	let capacity = std::cmp::max(range1.len(), pair.minimal_perfect_hash_size);
	result.entries = FixedMap::with_capacity(capacity);

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
fn binary_search(sequence: &mut Vec<*mut Node>, longest: isize,
				 entry: &mut Node
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
fn find_longest_common_sequence(map: &mut PatienceContext, res: &mut *mut Node) -> i32 {
    let mut sequence: Vec<*mut Node> = vec![std::ptr::null_mut(); map.entries.len()];

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
	xpp: &xpparam_t, pair: &mut xdpair, mut first: *mut Node,
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
	let mut map = PatienceContext::default();
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


pub(crate) fn do_patience_diff(xpp: &xpparam_t, pair: &mut xdpair) -> i32 {
	let (lhs, rhs) = get_file_context!(pair);

	let range1 = LINE_SHIFT..LINE_SHIFT + lhs.record.len();
	let range2 = LINE_SHIFT..LINE_SHIFT + rhs.record.len();

	drop(lhs);
	drop(rhs);

	patience_diff(xpp, pair, range1, range2)
}


#[no_mangle]
pub(crate) unsafe extern "C" fn xdl_do_patience_diff(xpp: *const xpparam_t, pair: *mut xdpair) -> i32 {
	let xpp = &*xpp;
	let pair = xdpair::from_raw_mut(pair);

	do_patience_diff(xpp, pair)
}


#[cfg(test)]
mod tests {
	use std::path::PathBuf;
	use crate::mock::helper::read_test_file;
	use crate::xdiff::*;
	use crate::xpatience::*;
	use crate::xprepare::safe_2way_prepare;
	use crate::xtypes::xd2way;

	#[test]
	fn test_patience_diff() {
		let file1 = read_test_file(PathBuf::from("file1.txt").as_path()).unwrap();
		let file2 = read_test_file(PathBuf::from("file2.txt").as_path()).unwrap();

		let xpp = xpparam_t::default();
		let mut two_way = xd2way::default();

		safe_2way_prepare(file1.as_slice(), file2.as_slice(), xpp.flags, &mut two_way);

		do_patience_diff(&xpp, &mut two_way.pair);
	}

}
