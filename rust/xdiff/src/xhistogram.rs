#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use interop::ivec::IVec;
use crate::xdfenv::xdfenv_t;
use crate::xdiff::{INVALID_INDEX, LINE_SHIFT};

const MAX_CHAIN_LENGTH: usize = 64;

struct record {
	ptr: usize,
    cnt: usize,
	next: usize,
}

struct histindex {
	record_storage: IVec<record>,
	record_chain: IVec<usize>,
	line_map: IVec<usize>,
	next_ptrs: IVec<usize>,
	ptr_shift: usize,
	cnt: usize,
	has_common: bool,
}

struct region {
	begin1: usize,
    end1: usize,
	begin2: usize,
    end2: usize,
}

fn mph_equal_by_line_number(env: &mut xdfenv_t, lhs: usize, rhs: usize) -> bool {
	let mph1 = env.xdf1.minimal_perfect_hash[lhs - LINE_SHIFT];
	let mph2 = env.xdf2.minimal_perfect_hash[rhs - LINE_SHIFT];
	mph1 == mph2
}

fn scanA(index: &mut histindex, env: &mut xdfenv_t, start1: usize, end1: usize) -> i32 {
	for ptr in (start1..end1).rev() {
		let tbl_idx = env.xdf1.minimal_perfect_hash[ptr - LINE_SHIFT] as usize;
		let mut rec_cur_idx = index.record_chain[tbl_idx];

		let mut continue_scan = false;
		let mut chain_len = 0;
		while rec_cur_idx != INVALID_INDEX {
			let rec_cur = &mut index.record_storage[rec_cur_idx];
			let mph1 = env.xdf1.minimal_perfect_hash[rec_cur.ptr - LINE_SHIFT];
			let mph2 = env.xdf1.minimal_perfect_hash[ptr - LINE_SHIFT];
			if mph1 == mph2 {
				/*
				 * ptr is identical to another element. Insert
				 * it onto the front of the existing element
				 * chain.
				 */
				index.next_ptrs[ptr - index.ptr_shift] = rec_cur.ptr;
				rec_cur.ptr = ptr;
				rec_cur.cnt = rec_cur.cnt + 1;
				index.line_map[ptr - index.ptr_shift] = rec_cur_idx;
				continue_scan = true;
				break;
			}

			rec_cur_idx = rec_cur.next;
			chain_len += 1;
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
		let rec_new_idx = index.record_storage.len();
		let rec_new = record {
			ptr,
			cnt: 1,
			next: index.record_chain[tbl_idx],
		};
		index.record_storage.push(rec_new);
		index.record_chain[tbl_idx] = rec_new_idx;
		index.line_map[ptr - index.ptr_shift] = rec_new_idx;
	}

	0
}

fn try_lcs(index: &mut histindex, env: &mut xdfenv_t, lcs: &mut region, b_ptr: usize,
	start1: usize, end1: usize, start2: usize, end2: usize) -> usize
{
	let mut b_next = b_ptr + 1;
	let tbl_idx = env.xdf2.minimal_perfect_hash[b_ptr - LINE_SHIFT];
	let mut should_break;
	let mut _as;
	let mut ae;
	let mut bs;
	let mut be;
	let mut np;
	let mut rc;

	let mut rec_cur_idx = index.record_chain[tbl_idx as usize];
	while rec_cur_idx != INVALID_INDEX {
		let rec_cur = &index.record_storage[rec_cur_idx];
		if rec_cur.cnt > index.cnt {
			if !index.has_common {
				index.has_common = mph_equal_by_line_number(env, rec_cur.ptr, b_ptr);
			}
			rec_cur_idx = rec_cur.next;
			continue;
		}

		_as = rec_cur.ptr;
		if !mph_equal_by_line_number(env, _as, b_ptr) {
			rec_cur_idx = rec_cur.next;
			continue;
		}

		index.has_common = true;
		loop {
			should_break = false;
			np = index.next_ptrs[_as - index.ptr_shift];
			bs = b_ptr;
			ae = _as;
			be = bs;
			rc = rec_cur.cnt;

			while start1 < _as && start2 < bs
				&& mph_equal_by_line_number(env, _as - 1, bs - 1) {
				_as -= 1;
				bs -= 1;
				if 1 < rc {
					let rec_t_idx = index.line_map[_as - index.ptr_shift];
					let rec_t = &index.record_storage[rec_t_idx];
					let cnt = rec_t.cnt;
					rc = std::cmp::min(rc, cnt);
				}
			}
			while ae + 1 < end1 && be + 1 < end2
				&& mph_equal_by_line_number(env, ae + 1, be + 1) {
				ae += 1;
				be += 1;
				if 1 < rc {
					let rec_t_idx = index.line_map[ae - index.ptr_shift];
					let rec_t = &index.record_storage[rec_t_idx];
					let cnt = rec_t.cnt;
					rc = std::cmp::min(rc, cnt);
				}
			}

			if b_next <= be {
				b_next = be + 1;
			}
			if lcs.end1 - lcs.begin1 < ae - _as || rc < index.cnt {
				lcs.begin1 = _as;
				lcs.begin2 = bs;
				lcs.end1 = ae;
				lcs.end2 = be;
				index.cnt = rc;
			}

			if np == 0 || np == INVALID_INDEX {
				break;
			}

			while np <= ae {
				np = index.next_ptrs[np - index.ptr_shift];
				if np == 0 || np == INVALID_INDEX {
					should_break = true;
					break;
				}
			}

			if should_break {
				break;
			}

			_as = np;
		}

		rec_cur_idx = rec_cur.next;
	}
	b_next
}

fn find_lcs(env: &mut xdfenv_t,
		    lcs: &mut region,
		    start1: usize, end1: usize, start2: usize, end2: usize
) -> i32 {
	let mut ret;
	let mut index = histindex {
		record_storage: IVec::default(),
		record_chain: IVec::default(),
		line_map: IVec::default(),
		next_ptrs: IVec::default(),
		ptr_shift: 0,
		cnt: 0,
		has_common: false,
	};

	let table_size = env.xdf1.record.len();
	index.record_chain.resize_exact(env.minimal_perfect_hash_size, INVALID_INDEX);
	index.line_map.resize_exact(table_size, INVALID_INDEX);
	index.next_ptrs.resize_exact(table_size, INVALID_INDEX);

	index.ptr_shift = start1;

	ret = scanA(&mut index, env, start1, end1);
	if ret != 0 {
		drop(index);
		return ret;
	}

	index.cnt = MAX_CHAIN_LENGTH + 1;

	let mut b_ptr = start2;
	while b_ptr + 1 <= end2 {
		b_ptr = try_lcs(&mut index, env, lcs, b_ptr, start1, end1, start2, end2);
	}

	if index.has_common && MAX_CHAIN_LENGTH < index.cnt {
		ret = 1;
	} else {
		ret = 0;
	}

	drop(index);
	ret
}

fn histogram_diff(env: &mut xdfenv_t,
				  mut start1: usize, end1: usize, mut start2: usize, end2: usize
) -> i32 {
	loop {
		let mut result = -1;

		if start1 >= end1 && start2 >= end2 {
			return 0;
		}

		if start1 == end1 {
			for i in start2..end2 {
				env.xdf2.rchg_vec[i - 1 + LINE_SHIFT] = 1;
			}
			return 0;
		}
		if start2 == end2 {
			for i in start1..end1 {
				env.xdf1.rchg_vec[i - 1 + LINE_SHIFT] = 1;
			}
			return 0;
		}

		let mut lcs = region {
			begin1: 0,
			end1: 0,
			begin2: 0,
			end2: 0,
		};
		let lcs_found = find_lcs(env, &mut lcs, start1, end1, start2, end2);
		if lcs_found < 0 {
			return result;
		} else if lcs_found > 0 {
			unimplemented!();
			// result = fall_back_to_classic_diff(xpp, env, start1, end1 - start1, start2, end2 - start1);
		} else {
			if lcs.begin1 == 0 && lcs.begin2 == 0 {
				for i in start1..end1 {
					env.xdf1.rchg_vec[i - 1 + LINE_SHIFT] = 1;
				}
				for i in start2..end2 {
					env.xdf2.rchg_vec[i - 1 + LINE_SHIFT] = 1;
				}
			} else {
				result = histogram_diff(env,
							start1, lcs.begin1,
							start2, lcs.begin2);
				if result != 0 {
					return result;
				}
				/*
				 * result = histogram_diff(xpp, env,
				 *            lcs.end1 + 1, end1,
				 *            lcs.end2 + 1, end2);
				 * but let's optimize tail recursion ourself:
				*/
				start1 = lcs.end1 + 1;
				start2 = lcs.end2 + 1;

				continue;
			}
		}
		return result;
	}
}

pub(crate) fn xdl_do_histogram_diff(env: &mut xdfenv_t) -> i32 {
	let end1 = env.xdf1.record.len();
	let end2 = env.xdf2.record.len();

	histogram_diff(env,
						  (env.delta_start + 1) as usize, end1 + LINE_SHIFT,
						  (env.delta_start + 1) as usize, end2 + LINE_SHIFT)
}


#[cfg(test)]
mod tests {
	use std::path::Path;
	use interop::ivec::IVec;
	use crate::mock::helper::read_test_file;
	use crate::xdfenv::xdfenv_t;
	use crate::xdiff::{XDF_HISTOGRAM_DIFF, XDF_IGNORE_WHITESPACE_CHANGE, XDF_INDENT_HEURISTIC};
	use crate::xhistogram::xdl_do_histogram_diff;
	use crate::xtypes::Occurrence;

	#[test]
	fn test_histogram_diff() {
		let _wd = std::env::current_dir().unwrap();

		let mut _flags = XDF_HISTOGRAM_DIFF;
		_flags |= XDF_INDENT_HEURISTIC;
		_flags |= XDF_IGNORE_WHITESPACE_CHANGE;

		let tv_name = ["salutations"];

		let t = Path::new("xhistogram");
		for tv in tv_name {
			let path = t.join(format!("{}{}", tv, "-before"));
			let before = read_test_file(&path).unwrap();

			let path = t.join(format!("{}{}", tv, "-after"));
			let after = read_test_file(&path).unwrap();

			let path = t.join(format!("{}{}", tv, "-expect"));
			let _expect = read_test_file(&path).unwrap();

			let mut xe = xdfenv_t::new(before.as_slice(), after.as_slice(), _flags);

			xdl_do_histogram_diff(&mut xe);
		}
	}
}

