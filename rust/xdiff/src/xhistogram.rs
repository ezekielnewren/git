#![allow(non_camel_case_types)]

use std::marker::PhantomData;
use std::ops::Range;
use interop::ivec::IVec;
use crate::xdiff::{LINE_SHIFT, SENTINEL, YES};
use crate::xdiffi::classic_diff_with_range;
use crate::xtypes::{xdpair, FileContext};


const MAX_CHAIN_LENGTH: usize = 64;


#[repr(C)]
struct record {
	ptr: usize,
    cnt: usize,
	next: *mut record,
}

impl Default for record {
	fn default() -> Self {
		Self {
			ptr: 0,
			cnt: 0,
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
	record_storage: IVec<record>,
	record: IVec<*mut record>,
	line_map: IVec<*mut record>,
	next_ptrs: IVec<usize>,
	ptr_shift: usize,
	cnt: usize,
	has_common: bool,
}


#[repr(C)]
struct region {
	range1: Range<usize>,
	range2: Range<usize>,
}


fn scan_a(index: &mut histindex, pair: &mut xdpair, range1: Range<usize>) -> i32 {
    let lhs = FileContext::new(&mut pair.lhs);

    for ptr in range1.rev() {
		let mut continue_scan = false;
		let tbl_idx = lhs.minimal_perfect_hash[ptr - LINE_SHIFT] as usize;

		let mut chain_len = 0;
		for rec in RecordIter::new(index.record[tbl_idx]) {
			continue_scan = false;
			let mph1 = lhs.minimal_perfect_hash[rec.ptr - LINE_SHIFT];
			let mph2 = lhs.minimal_perfect_hash[ptr - LINE_SHIFT];
			if mph1 == mph2 {
				/*
				 * ptr is identical to another element. Insert
				 * it onto the front of the existing element
				 * chain.
				 */
				index.next_ptrs[ptr - index.ptr_shift] = rec.ptr;
				rec.ptr = ptr;
				rec.cnt = rec.cnt + 1;
				index.line_map[ptr - index.ptr_shift] = rec;
				continue_scan = true;
				break;
			}

			chain_len += 1;
			if rec.next.is_null() {
				break;
			}
		}

		if continue_scan {
			continue;
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
		rec.ptr = ptr;
		rec.cnt = 1;
		rec.next = index.record[tbl_idx];
		index.record[tbl_idx] = rec;
		index.line_map[ptr - index.ptr_shift] = rec;
	}

	0
}

fn record_equal(pair: &xdpair, i1: usize, i2: usize) -> bool {
	let mph1 = unsafe { (*pair.lhs.minimal_perfect_hash)[i1 - LINE_SHIFT] };
	let mph2 = unsafe { (*pair.rhs.minimal_perfect_hash)[i2 - LINE_SHIFT] };
	mph1 == mph2
}


unsafe fn try_lcs(index: &mut histindex, pair: &mut xdpair, lcs: &mut region, b_ptr: usize,
	range1: Range<usize>, range2: Range<usize>,
) -> usize {
	let rhs = FileContext::new(&mut pair.rhs);

	let mut b_next = b_ptr + 1;
	let tbl_idx = rhs.minimal_perfect_hash[b_ptr - LINE_SHIFT] as usize;
	let mut range_a = Range::default();
	let mut range_b = Range::default();
	let mut np;
	let mut rc;
	let mut should_break;

	for rec in RecordIter::new(index.record[tbl_idx]) {
		if (*rec).cnt > index.cnt {
			if !index.has_common {
				index.has_common = record_equal(pair, (*rec).ptr, b_ptr);
			}
			continue;
		}

		range_a.start = (*rec).ptr;
		if !record_equal(pair, range_a.start, b_ptr) {
			continue;
		}

		index.has_common = true;
		loop {
			should_break = false;
			np = index.next_ptrs[range_a.start - index.ptr_shift];
			range_b.start = b_ptr;
			range_a.end = range_a.start;
			range_b.end = range_b.start;
			rc = (*rec).cnt;

			while range1.start < range_a.start && range2.start < range_b.start
				&& record_equal(pair, range_a.start - 1, range_b.start - 1) {
				range_a.start -= 1;
				range_b.start -= 1;
				if 1 < rc {
					let t_rec: *mut record = index.line_map[range_a.start - index.ptr_shift];
					let cnt = (*t_rec).cnt;
					rc = std::cmp::min(rc, cnt);
				}
			}
			while range_a.end < range1.end - 1 && range_b.end < range2.end - 1
				&& record_equal(pair, range_a.end + 1, range_b.end + 1) {
				range_a.end += 1;
				range_b.end += 1;
				if 1 < rc {
					let t_rec: *mut record = index.line_map[range_a.end - index.ptr_shift];
					let cnt = (*t_rec).cnt;
					rc = std::cmp::min(rc, cnt);
				}
			}

			if b_next <= range_b.end {
				b_next = range_b.end + 1;
			}
			if lcs.range1.end - lcs.range1.start < range_a.end - range_a.start || rc < index.cnt {
				lcs.range1.start = range_a.start;
				lcs.range2.start = range_b.start;
				lcs.range1.end = range_a.end;
				lcs.range2.end = range_b.end;
				index.cnt = rc;
			}

			if np == 0 {
				break;
			}

			while np <= range_a.end {
				np = index.next_ptrs[np - index.ptr_shift];
				if np == 0 {
					should_break = true;
					break;
				}
			}

			if should_break {
				break;
			}

			range_a.start = np;
		}
	}

	b_next
}


unsafe fn find_lcs(pair: &mut xdpair, lcs: &mut region,
	range1: Range<usize>, range2: Range<usize>,
) -> i32 {
	let fudge = ((*pair.lhs.record).len() + (*pair.rhs.record).len()) * 10;

	let mut index = histindex {
		record_storage: IVec::with_capacity(fudge),
		record: IVec::zero(pair.minimal_perfect_hash_size),
		line_map: IVec::zero(range1.len()),
		next_ptrs: IVec::zero(range1.len()),
		ptr_shift: range1.start,
		cnt: 0,
		has_common: false,
	};

	if scan_a(&mut index, pair, range1.clone()) != 0 {
		return -1;
	}

	index.cnt = MAX_CHAIN_LENGTH + 1;

	let mut b_ptr = range2.start;
	while b_ptr < range2.end {
		b_ptr = try_lcs(&mut index, pair, lcs, b_ptr, range1.clone(), range2.clone());
	}

	if index.has_common && MAX_CHAIN_LENGTH < index.cnt {
		1
	} else {
		0
	}
}


#[no_mangle]
unsafe extern "C" fn histogram_diff(flags: u64, pair: *mut xdpair,
	mut range1: Range<usize>, mut range2: Range<usize>,
) -> i32 {
	let pair = xdpair::from_raw_mut(pair);

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


#[no_mangle]
unsafe extern "C" fn xdl_do_histogram_diff(flags: u64, pair: *mut xdpair) -> i32 {
	let pair = xdpair::from_raw_mut(pair);

	let mut range1 = Range::default();
	let mut range2 = Range::default();

	range1.start = LINE_SHIFT + pair.delta_start;
	range1.end = LINE_SHIFT + (*pair.lhs.record).len() - pair.delta_end;
	range2.start = LINE_SHIFT + pair.delta_start;
	range2.end = LINE_SHIFT + (*pair.rhs.record).len() - pair.delta_end;

	histogram_diff(flags, pair, range1, range2)
}

