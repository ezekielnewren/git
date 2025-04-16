#![allow(non_camel_case_types)]

use std::marker::PhantomData;
use std::ops::Range;
use interop::ivec::IVec;
use crate::get_file_context;
use crate::xdiff::{LINE_SHIFT, SENTINEL, YES};
use crate::xdiffi::classic_diff_with_range;
use crate::xtypes::{xdpair, FileContext};


const MAX_CHAIN_LENGTH: usize = 64;


#[repr(C)]
struct record {
	line_number: usize,
    count: usize,
	next: *mut record,
}

impl Default for record {
	fn default() -> Self {
		Self {
			line_number: 0,
			count: 0,
			next: std::ptr::null_mut(),
		}
	}
}


struct RecordIter<'a> {
	cur: *mut record,
	_marker: PhantomData<&'a record>,
}

impl<'a> Iterator for RecordIter<'a> {
	type Item = &'a mut record;

	fn next(&mut self) -> Option<Self::Item> {
		if self.cur.is_null() {
			return None;
		}
		let t = self.cur;
		self.cur = unsafe { (*self.cur).next };
		Some(unsafe { &mut *t })
	}
}

impl<'a> RecordIter<'a> {
	fn new(start: *mut record) -> Self {
		Self {
			cur: start,
			_marker: PhantomData,
		}
	}
}


#[repr(C)]
struct histindex {
	record_storage: Vec<record>,
	record: IVec<*mut record>,
	line_map: IVec<*mut record>,
	next_line_numbers: IVec<usize>,
	line_number_shift: usize,
	count: usize,
	has_common: bool,
}


#[repr(C)]
struct region {
	range1: Range<usize>,
	range2: Range<usize>,
}


fn scan_a(index: &mut histindex, pair: &mut xdpair, range1: Range<usize>) -> i32 {
    let lhs = FileContext::new(&mut pair.lhs);

    'outer: for line_number in range1.rev() {
		let mph1 = lhs.minimal_perfect_hash[line_number - LINE_SHIFT] as usize;

		let mut chain_len = 0;
		for rec in RecordIter::new(index.record[mph1]) {
			let mph2 = lhs.minimal_perfect_hash[rec.line_number - LINE_SHIFT] as usize;
			if mph1 == mph2 {
				/*
				 * line_number is identical to another element. Insert
				 * it onto the front of the existing element
				 * chain.
				 */
				index.next_line_numbers[line_number - index.line_number_shift] = rec.line_number;
				rec.line_number = line_number;
				rec.count += 1;
				index.line_map[line_number - index.line_number_shift] = rec;
				continue 'outer;
			}

			chain_len += 1;
		}

		if chain_len == MAX_CHAIN_LENGTH {
			return -1;
		}

		/*
		 * This is the first time we have ever seen this particular
		 * element in the sequence. Construct a new chain for it.
		 */
		let last = index.record_storage.len();
		index.record_storage.push(record::default());
		let rec = &mut index.record_storage[last];
		rec.line_number = line_number;
		rec.count = 1;
		rec.next = index.record[mph1];
		index.record[mph1] = rec;
		index.line_map[line_number - index.line_number_shift] = rec;
	}

	0
}


fn try_lcs(index: &mut histindex, pair: &mut xdpair, lcs: &mut region, b_line_number: usize,
				  range1: Range<usize>, range2: Range<usize>,
) -> usize {
	let rhs = FileContext::new(&mut pair.rhs);

	let mut b_next = b_line_number + 1;
	let b_line_number_mph = rhs.minimal_perfect_hash[b_line_number - LINE_SHIFT] as usize;
	let mut range_a = Range::default();
	let mut range_b = Range::default();

	for rec in RecordIter::new(index.record[b_line_number_mph]) {
		if rec.count > index.count {
			if !index.has_common {
				index.has_common = pair.equal_by_line_number(rec.line_number, b_line_number);
			}
			continue;
		}

		range_a.start = rec.line_number;
		if !pair.equal_by_line_number(range_a.start, b_line_number) {
			continue;
		}

		index.has_common = true;
		'middle_loop: loop {
			let mut next_line = index.next_line_numbers[range_a.start - index.line_number_shift];
			range_b.start = b_line_number;
			range_a.end = range_a.start;
			range_b.end = range_b.start;
			let mut record_count = rec.count;

			while range1.start < range_a.start && range2.start < range_b.start
				&& pair.equal_by_line_number(range_a.start - 1, range_b.start - 1) {
				range_a.start -= 1;
				range_b.start -= 1;
				if 1 < record_count {
					let t_rec: *mut record = index.line_map[range_a.start - index.line_number_shift];
					let count = unsafe { (*t_rec).count };
					record_count = std::cmp::min(record_count, count);
				}
			}
			while range_a.end < range1.end - 1 && range_b.end < range2.end - 1
				&& pair.equal_by_line_number(range_a.end + 1, range_b.end + 1) {
				range_a.end += 1;
				range_b.end += 1;
				if 1 < record_count {
					let t_rec: *mut record = index.line_map[range_a.end - index.line_number_shift];
					let count = unsafe { (*t_rec).count };
					record_count = std::cmp::min(record_count, count);
				}
			}

			if b_next <= range_b.end {
				b_next = range_b.end + 1;
			}
			if lcs.range1.end - lcs.range1.start < range_a.end - range_a.start || record_count < index.count {
				lcs.range1.start = range_a.start;
				lcs.range2.start = range_b.start;
				lcs.range1.end = range_a.end;
				lcs.range2.end = range_b.end;
				index.count = record_count;
			}

			if next_line == 0 {
				break;
			}

			while next_line <= range_a.end {
				next_line = index.next_line_numbers[next_line - index.line_number_shift];
				if next_line == 0 {
					break 'middle_loop;
				}
			}

			range_a.start = next_line;
		}
	}

	b_next
}


fn find_lcs(pair: &mut xdpair, lcs: &mut region,
	range1: Range<usize>, range2: Range<usize>,
) -> i32 {
	let (lhs, rhs) = get_file_context!(pair);
	let fudge = (lhs.record.len() + rhs.record.len()) * 10;
	drop(lhs);
	drop(rhs);

	let mut index = histindex {
		record_storage: Vec::with_capacity(fudge),
		record: unsafe { IVec::zero(pair.minimal_perfect_hash_size) },
		line_map: unsafe { IVec::zero(range1.len()) },
		next_line_numbers: unsafe { IVec::zero(range1.len()) },
		line_number_shift: range1.start,
		count: 0,
		has_common: false,
	};

	if scan_a(&mut index, pair, range1.clone()) != 0 {
		return -1;
	}

	index.count = MAX_CHAIN_LENGTH + 1;

	let mut b_line_number = range2.start;
	while b_line_number < range2.end {
		b_line_number = try_lcs(&mut index, pair, lcs, b_line_number, range1.clone(), range2.clone());
	}

	if index.has_common && MAX_CHAIN_LENGTH < index.count {
		1
	} else {
		0
	}
}


fn histogram_diff(flags: u64, pair: &mut xdpair,
	mut range1: Range<usize>, mut range2: Range<usize>,
) -> i32 {
	let mut result;
	loop {
		result = -1;

		if range1.len() <= 0 && range2.len() <= 0 {
			return 0;
		}

		if range1.len() == 0 {
			for i in range2 {
				pair.rhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
			}
			return 0;
		}
		if range2.len() == 0 {
			for i in range1 {
				pair.lhs.consider[SENTINEL + i - LINE_SHIFT] = YES;
			}
			return 0;
		}

		let mut lcs = region {
			range1: Range::default(),
			range2: Range::default(),
		};
		let lcs_found = find_lcs(pair, &mut lcs, range1.clone(), range2.clone());
		if lcs_found < 0 {
			return result;
		}

		if lcs_found != 0 {
			return classic_diff_with_range(flags, pair, range1, range2);
		}

		if lcs.range1.start == 0 && lcs.range2.start == 0 {
			for i in range1 {
				pair.lhs.consider[SENTINEL + i - 1] = YES;
			}
			for i in range2 {
				pair.rhs.consider[SENTINEL + i - 1] = YES;
			}
			result = 0;
		} else {
			result = histogram_diff(flags, pair,
						range1.start..lcs.range1.start,
						range2.start..lcs.range2.start);
			if result != 0 {
				return result;
			}
			/*
			 * result = histogram_diff(flags, pair,
			 *            lcs.end1 + 1, LINE_END(1) - lcs.end1,
			 *            lcs.end2 + 1, LINE_END(2) - lcs.end2);
			 * but let's optimize tail recursion ourself:
			*/
			range1.start = lcs.range1.end + 1;
			range2.start = lcs.range2.end + 1;
			continue;
		}
		break;
	}

	result
}


pub(crate) fn do_histogram_diff(flags: u64, pair: &mut xdpair) -> i32 {
	let mut range1 = Range::default();
	let mut range2 = Range::default();

	let (lhs, rhs) = get_file_context!(pair);
	range1.start = LINE_SHIFT + pair.delta_start;
	range1.end = LINE_SHIFT + lhs.record.len() - pair.delta_end;
	range2.start = LINE_SHIFT + pair.delta_start;
	range2.end = LINE_SHIFT + rhs.record.len() - pair.delta_end;

	histogram_diff(flags, pair, range1, range2)
}

