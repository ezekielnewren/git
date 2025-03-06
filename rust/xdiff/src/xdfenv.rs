#![allow(non_camel_case_types)]

use std::collections::HashMap;
use interop::ivec::IVec;
use crate::gitmap::MPHB;
use crate::xdiff::{LINE_SHIFT, XDF_HISTOGRAM_DIFF, XDF_IGNORE_CR_AT_EOL, XDF_PATIENCE_DIFF, XDF_WHITESPACE_FLAGS};
use crate::xrecord::{xrecord_he, xrecord_t};
use crate::xtypes::ConsiderLine::*;
use crate::xtypes::{Occurrence};
use crate::xutils::{xdl_bogosqrt, LineReader};

const XDL_KPDIS_RUN: u64 = 4;
const XDL_MAX_EQLIMIT: u64 = 1024;
const XDL_SIMSCAN_WINDOW: u64 = 100;

#[repr(C)]
#[derive(Debug)]
pub struct xdfile_t {
	pub minimal_perfect_hash: IVec<u64>,
	pub record: IVec<xrecord_t>,
	pub rchg_vec: IVec<u8>,
	pub rindex: IVec<isize>,
	pub dstart: isize,
	pub dend: isize,
	pub rchg: *mut u8,
}


impl Default for xdfile_t {
	fn default() -> Self {
		Self {
			minimal_perfect_hash: IVec::new(),
			record: IVec::new(),
			rchg_vec: IVec::new(),
			rindex: IVec::new(),
			dstart: 0,
			dend: -1,
			rchg: std::ptr::null_mut(),
		}
	}
}


impl xdfile_t {
	pub(crate) unsafe fn from_raw<'a>(xdf: *mut Self, do_init: bool) -> &'a mut Self {
		if xdf.is_null() {
			panic!("null pointer");
		}
		if do_init {
			std::ptr::write(xdf, Self::default());
		}
		let out = &mut *xdf;

		#[cfg(debug_assertions)]
		if !do_init {
			out.record.test_invariants();
			out.minimal_perfect_hash.test_invariants();
			out.rindex.test_invariants();
			out.rchg_vec.test_invariants();
		}
		out
	}

	pub(crate) fn new(mf: &[u8], flags: u64) -> Self {
		let mut xdf = Self::default();

		let mut cur: *const u8 = std::ptr::null();
		let (mut no_eol, mut with_eol) = (0, 0);

		let mut it = LineReader::new(mf.as_ptr(), mf.len());
		while it.next(&mut cur, &mut no_eol, &mut with_eol) {
			let rec = xrecord_t::new(cur, no_eol, with_eol);
			xdf.record.push(rec);
		}

		if (flags & XDF_IGNORE_CR_AT_EOL) != 0 {
			for rec in xdf.record.as_mut_slice() {
				if rec.size_no_eol > 0 && unsafe { *rec.ptr.add(rec.size_no_eol - 1) } == b'\r' {
					rec.size_no_eol -= 1;
				}
			}
		}

		xdf.minimal_perfect_hash.reserve_exact(xdf.record.len());

		xdf.rchg_vec.resize(xdf.record.len() + 2, NO.into());

		xdf.dstart = 0;
		xdf.dend = xdf.record.len().wrapping_sub(1) as isize;

		xdf.rchg = unsafe { xdf.rchg_vec.as_mut_ptr().add(1) };

		xdf
	}

	pub(crate) fn as_ref(&self) -> &[u8] {
		if self.record.len() == 0 {
			&[]
		} else {
			let start = self.record[0].ptr;
			let last = &self.record[self.record.len() - 1];
			unsafe {
				let end = last.ptr.add(last.size_with_eol);
				std::slice::from_raw_parts(start, end.sub(start as usize) as usize)
			}
		}
	}

}

#[repr(C)]
#[derive(Default)]
pub struct xdfenv_t {
	pub xdf1: xdfile_t,
	pub xdf2: xdfile_t,
	pub minimal_perfect_hash_size: usize,
}


fn clean_mmatch(dis: &mut Vec<u8>, i: isize, mut s: isize, mut e: isize) -> bool {
	/*
	 * Limits the window the is examined during the similar-lines
	 * scan. The loops below stops when dis[i - r] == 1 (line that
	 * has no match), but there are corner cases where the loop
	 * proceed all the way to the extremities by causing huge
	 * performance penalties in case of big files.
	 */
	if i - s > XDL_SIMSCAN_WINDOW as isize {
		s = i - XDL_SIMSCAN_WINDOW as isize;
	}
	if e - i > XDL_SIMSCAN_WINDOW as isize {
		e = i + XDL_SIMSCAN_WINDOW as isize;
	}

	/*
	 * Scans the lines before 'i' to find a run of lines that either
	 * have no match (dis[j] == 0) or have multiple matches (dis[j] > 1).
	 * Note that we always call this function with dis[i] > 1, so the
	 * current line (i) is already a multimatch line.
	 */
	let mut rdis0 = 0;
	let mut rpdis0 = 1;
	let mut r = 1;
	// for (r = 1, rdis0 = 0, rpdis0 = 1; (i - r) >= s; r++) {
	while i - r >= s {
		if dis[(i - r) as usize] == NO {
			rdis0 += 1;
		} else if dis[(i - r) as usize] == TOO_MANY {
			rpdis0 += 1;
		} else {
			break;
		}
		r += 1;
	}
	/*
	 * If the run before the line 'i' found only multimatch lines, we
	 * return 0 and hence we don't make the current line (i) discarded.
	 * We want to discard multimatch lines only when they appear in the
	 * middle of runs with nomatch lines (dis[j] == 0).
	 */
	if rdis0 == 0 {
		return false;
	}
	// for (r = 1, rdis1 = 0, rpdis1 = 1; (i + r) <= e; r++) {
	let mut rdis1 = 0;
	let mut rpdis1 = 1;
	r = 1;
	while i + r <= e {
		if dis[(i + r) as usize] == NO {
			rdis1 += 1;
		} else if dis[(i + r) as usize] == TOO_MANY {
			rpdis1 += 1;
		} else {
			break;
		}

		r += 1;
	}
	/*
	 * If the run after the line 'i' found only multimatch lines, we
	 * return 0 and hence we don't make the current line (i) discarded.
	 */
	if rdis1 == 0 {
		return false;
	}
	rdis1 += rdis0;
	rpdis1 += rpdis0;

	rpdis1 * XDL_KPDIS_RUN < (rpdis1 + rdis1)
}


impl xdfenv_t {

	// fn cleanup_records(&mut self, occurrence: &mut Vec<Occurrence>) {
	// 	let mut dis1 = Vec::<u8>::new();
	// 	let mut dis2 = Vec::<u8>::new();
	//
	// 	let end1 = self.xdf1.record.len() - self.delta_end as usize;
	// 	let end2 = self.xdf2.record.len() - self.delta_end as usize;
	//
	// 	dis1.resize(self.xdf1.rchg_vec.len(), NO.into());
	// 	dis2.resize(self.xdf2.rchg_vec.len(), NO.into());
	//
	// 	let mlim1 = std::cmp::min(XDL_MAX_EQLIMIT, xdl_bogosqrt(self.xdf1.record.len() as u64)) as usize;
	// 	for i in self.delta_start as usize..end1 {
	// 		let mph = self.xdf1.minimal_perfect_hash[i];
	// 		let nm = occurrence[mph as usize].file1;
	// 		dis1[i] = if nm == 0 {
	// 			NO
	// 		} else if nm >= mlim1 {
	// 			TOO_MANY
	// 		} else {
	// 			YES
	// 		}.into();
	// 	}
	//
	// 	let mlim2 = std::cmp::min(XDL_MAX_EQLIMIT, xdl_bogosqrt(self.xdf2.record.len() as u64)) as usize;
	// 	for i in self.delta_start as usize..end2 {
	// 		let mph = self.xdf2.minimal_perfect_hash[i];
	// 		let nm = occurrence[mph as usize].file1;
	// 		dis2[i] = if nm == 0 {
	// 			NO
	// 		} else if nm >= mlim2 {
	// 			TOO_MANY
	// 		} else {
	// 			YES
	// 		}.into();
	// 	}
	//
	// 	for i in self.delta_start as usize..end1 {
	// 		if dis1[i] == YES ||
	// 			(dis1[i] == TOO_MANY && !clean_mmatch(&mut dis1, i as isize, self.delta_start, end1 as isize - 1)) {
	// 			self.xdf1.rindex.push(i as isize);
	// 		} else {
	// 			self.xdf1.rchg_vec[i + LINE_SHIFT] = YES.into();
	// 		}
	// 	}
	//
	// 	for i in self.delta_start as usize..end2 {
	// 		if dis2[i] == YES ||
	// 			(dis2[i] == TOO_MANY && !clean_mmatch(&mut dis2, i as isize, self.delta_start, end2 as isize - 1)) {
	// 			self.xdf2.rindex.push(i as isize);
	// 		} else {
	// 			self.xdf2.rchg_vec[i + LINE_SHIFT] = YES.into();
	// 		}
	// 	}
	// }
	//
	// pub(crate) fn trim_ends(&mut self) {
	// 	let mph1 = &self.xdf1.minimal_perfect_hash.as_slice();
	// 	let mph2 = &self.xdf2.minimal_perfect_hash.as_slice();
	// 	let lim = std::cmp::min(mph1.len(), mph2.len());
	//
	// 	for i in 0..lim {
	// 		if mph1[i] != mph2[i] {
	// 			self.delta_start = i as isize;
	// 			break;
	// 		}
	// 	}
	//
	// 	for i in 0..lim {
	// 		if mph1[mph1.len() - 1 - i] != mph2[mph2.len() - 1 - i] {
	// 			self.delta_end = i as isize;
	// 			break;
	// 		}
	// 	}
	// }


	pub(crate) fn construct_mph_and_occurrences(&mut self, occurrence: Option<&mut IVec<Occurrence>>, flags: u64) {
		let capacity = self.xdf1.record.len() + self.xdf2.record.len();
		let he = xrecord_he::new(flags);
		let mut mphb = MPHB::<xrecord_t, xrecord_he>::new(capacity, &he);

		for rec in self.xdf1.record.as_slice() {
			self.xdf1.minimal_perfect_hash.push(mphb.hash(rec));
		}

		for rec in self.xdf2.record.as_slice() {
			self.xdf2.minimal_perfect_hash.push(mphb.hash(rec));
		}

		self.minimal_perfect_hash_size = mphb.finish();

		if let Some(occ) = occurrence {
			/*
			 * ORDER MATTERS!!!, counting occurrences will only work properly if
			 * the records are iterated over in the same way that the mph set
			 * was constructed
			 */
			for minimal_perfect_hash in self.xdf1.minimal_perfect_hash.as_slice() {
				if *minimal_perfect_hash == occ.len() as u64 {
					occ.push(Occurrence::default());
				}
				occ[*minimal_perfect_hash as usize].file1 += 1;
			}

			for minimal_perfect_hash in self.xdf2.minimal_perfect_hash.as_slice() {
				if *minimal_perfect_hash == occ.len() as u64 {
					occ.push(Occurrence::default());
				}
				occ[*minimal_perfect_hash as usize].file1 += 1;
			}
		}
	}
}


impl xdfenv_t {

	pub(crate) fn new(mf1: &[u8], mf2: &[u8], flags: u64) -> Self {
		let mut xe = xdfenv_t::default();
		xe.xdf1 = xdfile_t::new(mf1, flags);
		xe.xdf2 = xdfile_t::new(mf2, flags);

		let mut occurrence: IVec<Occurrence> = IVec::new();
		let mut occ = None;
		if (flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0 {
			occ = Some(&mut occurrence);
		}

		xe.construct_mph_and_occurrences(occ, flags);

		// if (flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0 {
		// 	xe.trim_ends();
		// 	xe.cleanup_records(&mut occurrence);
		// }

		xe
	}

	pub(crate) unsafe fn from_raw<'a>(xe: *mut xdfenv_t, do_init: bool) -> &'a mut xdfenv_t {
		if xe.is_null() {
			panic!("xdfenv_t is null");
		}
		if do_init {
			std::ptr::write(xe, xdfenv_t::default());
		}
		&mut *xe
	}
}

#[cfg(test)]
mod tests {
	use std::path::Path;
	use crate::mock::helper::read_test_file;
	use crate::xdfenv::{xdfenv_t, xdfile_t};
	use crate::xdiff::{XDF_HISTOGRAM_DIFF, XDF_IGNORE_WHITESPACE_CHANGE, XDF_INDENT_HEURISTIC};

	#[test]
	fn test_prepare() {
		let _wd = std::env::current_dir().unwrap();

		// let mut xpp: xpparam_t = Default::default();
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

			let mut xe = xdfenv_t::default();
			xe.xdf1 = xdfile_t::new(before.as_slice(), _flags);
			xe.xdf2 = xdfile_t::new(after.as_slice(), _flags);
		}
	}

}

