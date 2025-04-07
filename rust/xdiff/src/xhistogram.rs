#![allow(non_camel_case_types)]

use std::marker::PhantomData;
use interop::ivec::IVec;
use crate::get_file_context;
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
	begin1: usize,
	end1: usize,
	begin2: usize,
	end2: usize,
}


fn scan_a(index: &mut histindex, pair: &mut xdpair, line1: usize, count1: usize) -> i32 {
    let lhs = FileContext::new(&mut pair.lhs);

    for ptr in (line1..=line1 + count1 - 1).rev() {
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
	line1: usize, count1: usize, line2: usize, count2: usize
) -> usize {
	let rhs = FileContext::new(&mut pair.rhs);

	let mut b_next = b_ptr + 1;
	let tbl_idx = rhs.minimal_perfect_hash[b_ptr - LINE_SHIFT] as usize;
	let mut range_a = 0..0;
	let mut range_b = 0..0;
	let mut np;
	let mut rc;
	let mut should_break;

	// for (; rec; rec = rec->next) {
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

			while line1 < range_a.start && line2 < range_b.start
				&& record_equal(pair, range_a.start - 1, range_b.start - 1) {
				range_a.start -= 1;
				range_b.start -= 1;
				if 1 < rc {
					let t_rec: *mut record = index.line_map[range_a.start - index.ptr_shift];
					let cnt = (*t_rec).cnt;
					rc = std::cmp::min(rc, cnt);
				}
			}
			while range_a.end < line1 + count1 - 1 && range_b.end < line2 + count2 - 1
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
			if lcs.end1 - lcs.begin1 < range_a.end - range_a.start || rc < index.cnt {
				lcs.begin1 = range_a.start;
				lcs.begin2 = range_b.start;
				lcs.end1 = range_a.end;
				lcs.end2 = range_b.end;
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


#[no_mangle]
unsafe extern "C" fn xdl_find_lcs(pair: *mut xdpair, lcs: *mut region,
				   line1: usize, count1: usize, line2: usize, count2: usize
) -> i32 {
	let pair = xdpair::from_raw_mut(pair);
	let lcs = &mut *lcs;

	find_lcs(pair, lcs, line1, count1, line2, count2)
}

unsafe fn find_lcs(pair: &mut xdpair, lcs: &mut region,
	line1: usize, count1: usize, line2: usize, count2: usize
) -> i32 {
	let fudge = ((*pair.lhs.record).len() + (*pair.rhs.record).len()) * 10;

	let mut index = histindex {
		record_storage: IVec::with_capacity(fudge),
		record: IVec::zero(pair.minimal_perfect_hash_size),
		line_map: IVec::zero(count1),
		next_ptrs: IVec::zero(count1),
		ptr_shift: line1,
		cnt: 0,
		has_common: false,
	};

	if scan_a(&mut index, pair, line1, count1) != 0 {
		return -1;
	}

	index.cnt = MAX_CHAIN_LENGTH + 1;

	let mut b_ptr = line2;
	while b_ptr <= line2 + count2 - 1 {
		b_ptr = try_lcs(&mut index, pair, lcs, b_ptr, line1, count1, line2, count2);
	}

	if index.has_common && MAX_CHAIN_LENGTH < index.cnt {
		1
	} else {
		0
	}
}


#[no_mangle]
unsafe extern "C" fn histogram_diff(flags: u64, pair: *mut xdpair,
	mut line1: usize, mut count1: usize, mut line2: usize, mut count2: usize
) -> i32 {
	let pair = xdpair::from_raw_mut(pair);

	let mut result;
	loop {
		result = -1;

		if count1 <= 0 && count2 <= 0 {
			return 0;
		}

		if count1 == 0 {
			while count2 > 0 {
				count2 -= 1;
				pair.rhs.consider[SENTINEL + line2 - LINE_SHIFT] = YES;
				line2 += 1;
			}
			return 0;
		}
		if count2 == 0 {
			while count1 > 0 {
				count1 -= 1;
				pair.lhs.consider[SENTINEL + line1 - LINE_SHIFT] = YES;
				line1 += 1;
			}
			return 0;
		}

		let mut lcs = region {
			begin1: 0,
			end1: 0,
			begin2: 0,
			end2: 0,
		};
		let lcs_found = xdl_find_lcs(pair, &mut lcs, line1, count1, line2, count2);
		if lcs_found < 0 {
			return result;
		}

		if lcs_found != 0 {
			return classic_diff_with_range(flags, pair, line1..line1 + count1, line2..line2 + count2);
		}

		if lcs.begin1 == 0 && lcs.begin2 == 0 {
			while count1 > 0 {
				count1 -= 1;
				pair.lhs.consider[SENTINEL + line1 - 1] = YES;
				line1 += 1;
			}
			while count2 > 0 {
				count2 -= 1;
				pair.rhs.consider[SENTINEL + line2 - 1] = YES;
				line2 += 1;
			}
			result = 0;
		} else {
			result = histogram_diff(flags, pair,
						line1, lcs.begin1 - line1,
						line2, lcs.begin2 - line2);
			if result != 0 {
				return result;
			}
			/*
			 * result = histogram_diff(flags, pair,
			 *            lcs.end1 + 1, LINE_END(1) - lcs.end1,
			 *            lcs.end2 + 1, LINE_END(2) - lcs.end2);
			 * but let's optimize tail recursion ourself:
			*/
			count1 = line1 + count1 - 1 - lcs.end1;
			line1 = lcs.end1 + 1;
			count2 = line2 + count2 - 1 - lcs.end2;
			line2 = lcs.end2 + 1;
			continue;
		}
		break;
	}

	result
}

