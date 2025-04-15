#![allow(non_camel_case_types)]

use std::ops::Range;
use bitvec::prelude::*;
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




/*
 * This is a hash mapping from line hash to line numbers in the first and
 * second file.
 */
struct OrderedMap {
	bitvec: BitVec,
	mph2entry: Vec<usize>,
	entry: Vec<Node>,
	first: *mut Node,
    last: *mut Node,
	/* were common records found? */
	has_matches: bool,
}

impl OrderedMap {
	fn new(capacity: usize) -> Self {
		let mut it = Self {
			bitvec: bitvec![0; capacity],
			mph2entry: Vec::with_capacity(capacity),
			entry: Vec::with_capacity(capacity),
			first: std::ptr::null_mut(),
			last: std::ptr::null_mut(),
			has_matches: false,
		};
		unsafe {
			it.mph2entry.set_len(it.mph2entry.capacity());
		}
		it
	}
}


struct PatienceContext<'a> {
	lhs: FileContext<'a>,
	rhs: FileContext<'a>,
	rhs_reverse: Vec<Vec<usize>>,
	minimal_perfect_hash_size: usize,
	pair: &'a mut xdpair,
	xpp: &'a xpparam_t,
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


impl<'a> PatienceContext<'a> {


	/*
	 * The idea is to start with the list of common unique lines sorted by
	 * the order in file1.  For each of these pairs, the longest (partial)
	 * sequence whose last element's line2 is smaller is determined.
	 *
	 * For efficiency, the sequences are kept in a list containing exactly one
	 * item per sequence length: the sequence with the smallest last
	 * element (in terms of line2).
	 */
	fn find_longest_common_sequence(&mut self, map: &mut OrderedMap, res: &mut *mut Node) -> i32 {
		let mut sequence: Vec<*mut Node> = vec![std::ptr::null_mut(); map.mph2entry.len()];

		let mut longest = 0isize;

		/*
         * If not -1, this entry in sequence must never be overridden.
         * Therefore, overriding entries before this has no effect, so
         * do not do that either.
         */
		let mut anchor_i = -1;

		for entry in map.entry.iter_mut() {
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

	/*
	 * This function has to be called for each recursion into the inter-hunk
	 * parts, as previously non-unique lines can become unique when being
	 * restricted to a smaller part of the files.
	 *
	 * It is assumed that env has been prepared using xdl_prepare().
	 */
	fn fill_hashmap(&mut self,
		map: &mut OrderedMap,
		range1: Range<usize>, range2: Range<usize>
	) -> i32 {
		/* First, fill with entries from the first file */
		for i in range1 {
			let mph = self.lhs.minimal_perfect_hash[i - LINE_SHIFT] as usize;
			if map.bitvec[mph] {
				let node = &mut map.entry[map.mph2entry[mph]];
				node.line2 = NON_UNIQUE;
				continue;
			} else {
				map.bitvec.set(mph, true);
				map.mph2entry[mph] = map.entry.len();
				map.entry.push(Node {
					line1: i,
					line2: 0,
					next: std::ptr::null_mut(),
					previous: std::ptr::null_mut(),
					anchor: is_anchor(self.xpp, self.lhs.record[i - LINE_SHIFT].as_ref()),
				});
				let node = &mut map.entry[map.mph2entry[mph]];
				if map.first.is_null() {
					map.first = node;
				}
				if !map.last.is_null() {
					unsafe { (*map.last).next = node };
					node.previous = map.last;
				}
				map.last = node;
			}
		}

		/* Then search for matches in the second file */
		for i in range2 {
			let mph = self.rhs.minimal_perfect_hash[i - LINE_SHIFT] as usize;

			if map.bitvec[mph] {
				let node: &mut Node = &mut map.entry[map.mph2entry[mph]];
				map.has_matches = true;
				if node.line2 != 0 {
					node.line2 = NON_UNIQUE;
				} else {
					node.line2 = i;
				}
			}
		}

		0
	}

	fn expand(&mut self, mut first: *mut Node,
			  range1: &mut Range<usize>, range2: &mut Range<usize>
	) -> (usize, usize) {
		/* Try to grow the line ranges of common lines */
		let mut next1;
		let mut next2;
		if !first.is_null() {
			unsafe {
				next1 = (*first).line1;
				next2 = (*first).line2;
			}
			while next1 > range1.start && next2 > range2.start {
				let mph1 = self.lhs.minimal_perfect_hash[next1 - 1 - LINE_SHIFT];
				let mph2 = self.rhs.minimal_perfect_hash[next2 - 1 - LINE_SHIFT];
				if mph1 != mph2 {
					break;
				}
				next1 -= 1;
				next2 -= 1;
			}
		} else {
			next1 = range1.end;
			next2 = range2.end;
		}
		while range1.start < next1 && range2.start < next2 {
			let mph1 = self.lhs.minimal_perfect_hash[range1.start - LINE_SHIFT];
			let mph2 = self.rhs.minimal_perfect_hash[range2.start - LINE_SHIFT];
			if mph1 != mph2 {
				break;
			}
			range1.start += 1;
			range2.start += 1;
		}

		(next1, next2)
	}

	fn walk_common_sequence(&mut self, mut first: *mut Node,
		mut range1: Range<usize>, mut range2: Range<usize>
	) -> i32 {
		loop {
			let (next1, next2) = self.expand(first, &mut range1, &mut range2);

			/* Recurse */
			if next1 > range1.start || next2 > range2.start {
				if self.patience_diff(range1.start..next1,
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

	fn patience_diff(&mut self, range1: Range<usize>, range2: Range<usize>) -> i32 {
		/* We know exactly how large we want the hash map */
		let mut map = OrderedMap::new(self.minimal_perfect_hash_size);
		let mut result;

		/* trivial case: one side is empty */
		if range1.len() == 0 {
			for i in range2 {
				self.rhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
			}
			return 0;
		} else if range2.len() == 0 {
			for i in range1 {
				self.lhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
			}
			return 0;
		}

		if self.fill_hashmap(&mut map, range1.clone(), range2.clone()) != 0 {
			return -1;
		}

		/* are there any matching lines at all? */
		if !map.has_matches {
			for i in range1 {
				self.lhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
			}
			for i in range2 {
				self.rhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
			}
			return 0;
		}

		let mut first = std::ptr::null_mut();
		result = self.find_longest_common_sequence(&mut map, &mut first);
		if result != 0 {
			return result;
		}
		if !first.is_null() {
			result = self.walk_common_sequence(first, range1, range2);
		} else {
			result = classic_diff_with_range(self.xpp.flags, self.pair, range1, range2);
		}

		result
	}
}


pub(crate) fn do_patience_diff(xpp: &xpparam_t, pair: &mut xdpair) -> i32 {
	let mut ctx = PatienceContext {
		lhs: FileContext::from_raw(&mut pair.lhs as *mut xd_file_context),
		rhs: FileContext::from_raw(&mut pair.rhs as *mut xd_file_context),
		rhs_reverse: vec![Vec::new(); pair.minimal_perfect_hash_size],
		minimal_perfect_hash_size: pair.minimal_perfect_hash_size,
		pair,
		xpp,
	};

	for (i, mph) in ctx.rhs.minimal_perfect_hash.iter().enumerate() {
		ctx.rhs_reverse[*mph as usize].push(i);
	}

	let range1 = LINE_SHIFT..LINE_SHIFT + ctx.lhs.record.len();
	let range2 = LINE_SHIFT..LINE_SHIFT + ctx.rhs.record.len();

	ctx.patience_diff(range1, range2)
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
		let mut tv = Vec::new();
		tv.push(("xhistogram/salutations-before", "xhistogram/gitdump.txt"));
		tv.push(("names_of_numbers.txt", "nato_phonetic.txt"));
		tv.push(("duplicates.txt", "duplicates.txt"));
		tv.push(("file1.txt", "file2.txt"));


		for (file1, file2) in tv {
			let file1 = read_test_file(PathBuf::from(file1).as_path()).unwrap();
			let file2 = read_test_file(PathBuf::from(file2).as_path()).unwrap();

			let xpp = xpparam_t::default();
			let mut two_way = xd2way::default();

			safe_2way_prepare(file1.as_slice(), file2.as_slice(), xpp.flags, &mut two_way);

			do_patience_diff(&xpp, &mut two_way.pair);
		}

	}

}
