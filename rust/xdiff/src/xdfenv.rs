#![allow(non_camel_case_types)]

use std::collections::HashMap;
use interop::ivec::IVec;
use crate::xrecord::xrecord_t;
use crate::xtypes::ConsiderLine::*;
use crate::xtypes::Occurrence;
use crate::xutils::LineReader;

const XDL_KPDIS_RUN: u64 = 4;
const XDL_MAX_EQLIMIT: u64 = 1024;
const XDL_SIMSCAN_WINDOW: u64 = 100;

#[repr(C)]
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

		for (line, eol_len) in LineReader::new(mf) {
			let rec = xrecord_t::new(line, eol_len, flags);
			xdf.record.push(rec);
		}

		xdf.rchg_vec.resize(xdf.record.len() + 2, NO.into());

		xdf.rchg = unsafe { xdf.rchg_vec.as_mut_ptr().add(1) };

		xdf
	}


}

#[repr(C)]
#[derive(Default)]
pub struct xdfenv_t {
	pub xdf1: xdfile_t,
	pub xdf2: xdfile_t,
	pub occurrence: IVec<Occurrence>,
	pub delta_start: isize,
	pub delta_end: isize,
}


impl xdfenv_t {

	pub(crate) fn new(mf1: &[u8], mf2: &[u8], flags: u64) -> Self {
		let mut xe = xdfenv_t::default();
		xe.xdf1 = xdfile_t::new(mf1, flags);
		xe.xdf2 = xdfile_t::new(mf2, flags);

		// xe.count_occurrences();

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


	pub(crate) fn count_occurrences(&mut self) {
		let mut mph = HashMap::<xrecord_t, u64>::new();
		let mut count = 0;

		for rec in self.xdf1.record.as_slice() {
			let minimal_perfect_hash;
			if let Some(v) = mph.get(rec) {
				self.occurrence[*v as usize].file1 += 1;
				minimal_perfect_hash = *v;
			} else {
				minimal_perfect_hash = count;
				self.occurrence.push(Occurrence {
					file1: 1,
					file2: 0,
				});
				mph.insert(rec.clone(), count);
				count += 1;
			}
			self.xdf1.minimal_perfect_hash.push(minimal_perfect_hash);
		}

		for rec in self.xdf2.record.as_slice() {
			let minimal_perfect_hash;
			if let Some(v) = mph.get(rec) {
				self.occurrence[*v as usize].file2 += 1;
				minimal_perfect_hash = *v;
			} else {
				minimal_perfect_hash = count;
				self.occurrence.push(Occurrence {
					file1: 0,
					file2: 1,
				});
				mph.insert(rec.clone(), count);
				count += 1;
			}
			self.xdf2.minimal_perfect_hash.push(minimal_perfect_hash);
		}
	}
}

#[cfg(test)]
mod tests {
	use std::path::Path;
	use crate::mock::helper::read_test_file;
	use crate::xdfenv::{xdfenv_t, xdfile_t};
	use crate::xdiff::{XDF_HISTOGRAM_DIFF, XDF_INDENT_HEURISTIC};

	#[test]
	fn test_prepare() {
		let _wd = std::env::current_dir().unwrap();

		// let mut xpp: xpparam_t = Default::default();
		let mut _flags = XDF_HISTOGRAM_DIFF;
		_flags |= XDF_INDENT_HEURISTIC;

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

