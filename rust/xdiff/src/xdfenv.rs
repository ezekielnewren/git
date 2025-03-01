#![allow(non_camel_case_types)]

use std::collections::HashMap;
use interop::ivec::IVec;
use crate::xdiff::XDF_IGNORE_CR_AT_EOL;
use crate::xrecord::xrecord_t;
use crate::xtypes::ConsiderLine::*;
use crate::xtypes::{MinimalPerfectHashBuilder, Occurrence};
use crate::xutils::LineReader;

const XDL_KPDIS_RUN: u64 = 4;
const XDL_MAX_EQLIMIT: u64 = 1024;
const XDL_SIMSCAN_WINDOW: u64 = 100;

#[repr(C)]
#[derive(Debug)]
pub struct xdfile_t {
	pub record: IVec<xrecord_t>,
	pub minimal_perfect_hash: IVec<u64>,
	pub rchg_vec: IVec<u8>,
	pub rindex: IVec<isize>,
	pub rchg: *mut u8,
}


impl Default for xdfile_t {
	fn default() -> Self {
		Self {
			record: IVec::new(),
			minimal_perfect_hash: IVec::new(),
			rchg_vec: IVec::new(),
			rindex: IVec::new(),
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

		let ignore = (flags & XDF_IGNORE_CR_AT_EOL) != 0;
		for (line, eol_len) in LineReader::new(mf, ignore) {
			let rec = xrecord_t::new(line, eol_len, flags);
			xdf.record.push(rec);
		}

		xdf.rchg_vec.resize(xdf.record.len() + 2, NO.into());

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
	pub delta_start: isize,
	pub delta_end: isize,
}


#[no_mangle]
extern "C" fn xdl_clean_mmatch(dis: *mut IVec<u8>, i: isize, mut s: isize, mut e: isize) -> bool {
	let dis = unsafe { IVec::from_raw_mut(dis) };

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

	pub(crate) fn new(mf1: &[u8], mf2: &[u8], flags: u64, occurrence: Option<&mut IVec<Occurrence>>) -> Self {
		let mut xe = xdfenv_t::default();
		xe.xdf1 = xdfile_t::new(mf1, flags);
		xe.xdf2 = xdfile_t::new(mf2, flags);

		xe.construct_mph_and_occurrences(occurrence);

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


impl xdfenv_t {

	pub(crate) fn trim_ends(&mut self) {
		let mph1 = &self.xdf1.minimal_perfect_hash.as_slice();
		let mph2 = &self.xdf2.minimal_perfect_hash.as_slice();
		let lim = std::cmp::min(mph1.len(), mph2.len());

		for i in 0..lim {
			if mph1[i] != mph2[i] {
				self.delta_start = i as isize;
				break;
			}
		}

		for i in 0..lim {
			if mph1[mph1.len() - 1 - i] != mph2[mph2.len() - 1 - i] {
				self.delta_end = i as isize;
				break;
			}
		}
	}


	pub(crate) fn construct_mph_and_occurrences(&mut self, occurrence: Option<&mut IVec<Occurrence>>) {
		let mut mphb = MinimalPerfectHashBuilder::<xrecord_t>::default();

		for rec in self.xdf1.record.as_slice() {
			self.xdf1.minimal_perfect_hash.push(mphb.hash(rec));
		}

		for rec in self.xdf2.record.as_slice() {
			self.xdf2.minimal_perfect_hash.push(mphb.hash(rec));
		}

		self.minimal_perfect_hash_size = mphb.finish();

		/*
		 * ORDER MATTERS!!!, counting occurrences will only work properly if
		 * the records are iterated over in the same way that the mph set
		 * was constructed
		 */
		if let Some(occ) = occurrence {
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

