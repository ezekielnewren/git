#![allow(non_camel_case_types)]

use interop::ivec::IVec;
use crate::xrecord::xrecord_t;
use crate::xtypes::ConsiderLine::*;


const XDL_KPDIS_RUN: u64 = 4;
const XDL_MAX_EQLIMIT: u64 = 1024;
const XDL_SIMSCAN_WINDOW: u64 = 100;

#[repr(C)]
pub struct xdfenv_t {
	pub xdf1: xdfile_t,
	pub xdf2: xdfile_t,
	pub minimal_perfect_hash_size: usize,
}


#[repr(C)]
pub struct xdfile_t {
	pub record: IVec<xrecord_t>,
	pub rchg_vec: IVec<u8>,
	pub rindex: IVec<isize>,
	pub hash: IVec<u64>,
	pub dstart: isize,
	pub dend: isize,
	pub rchg: *mut u8,
}


impl Default for xdfile_t {
	fn default() -> Self {
		Self {
			record: IVec::new(),
			rchg_vec: IVec::new(),
			rindex: IVec::new(),
			hash: IVec::new(),
			dstart: 0,
			dend: 0,
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
			out.rindex.test_invariants();
			out.rchg_vec.test_invariants();
			out.hash.test_invariants();
		}
		out
	}
}


// pub struct xdfile_t<'a> {
// 	id: usize,
// 	slice: &'a [u8],
// 	// #[cfg(debug_assertions)]0
// 	// view: &'a str,
// 	_match_limit: usize,
// 	line_vec: Vec<xrecord_t>,
// 	pub index: Vec<usize>,
// 	_consider: Vec<ConsiderLine>,
// }
//
// impl<'a> xdfile_t<'a> {
//
// 	pub fn new(slice: &'a [u8], flags: u64, id: usize) -> Self {
// 		let mut xdf = Self {
// 			id,
// 			slice,
// 			// #[cfg(debug_assertions)]
// 			// view: unsafe { std::str::from_utf8_unchecked(slice) },
// 			_match_limit: XDL_MAX_EQLIMIT as usize,
// 			index: Vec::new(),
// 			line_vec: Vec::new(),
// 			_consider: Vec::new(),
// 		};
//
// 		for (line, _eol_len) in LineReader::new(xdf.slice) {
// 			xdf.line_vec.push(xrecord_t::new(line, flags));
// 			xdf._consider.push(NO);
// 		}
// 		xdf._match_limit = std::cmp::min(xdf.line_vec.len().isqrt(), XDL_MAX_EQLIMIT as usize);
// 		xdf
// 	}
//
// 	pub fn as_ref(&self) -> &[u8] {
// 		self.slice
// 	}
//
// 	pub fn lines(&self) -> &[xrecord_t] {
// 		self.line_vec.as_slice()
// 	}
//
// 	pub fn consider_mut(&mut self, index: usize) -> &mut ConsiderLine {
// 		&mut self._consider[index]
// 	}
//
// 	pub fn consider(&self, index: usize) -> &ConsiderLine {
// 		&self._consider[index]
// 	}
//
// 	pub fn match_limit(&self) -> usize {
// 		self._match_limit
// 	}
//
// 	fn count_occurrences(&mut self, line_frequency: &mut XdiffHashMap<xrecord_t, Occurrence>) {
// 		for (_line_idx, line) in self.line_vec.iter().enumerate() {
// 			let occ: &mut Occurrence = get_or_default(line_frequency, line);
// 			occ.increment(self.id);
// 		}
// 	}
//
// 	fn xdl_clean_mmatch(&mut self, line_idx: usize, scope: &Range<usize>) -> bool {
// 		let mut r: usize;
// 		let mut rdis0: usize;
// 		let mut rpdis0: usize;
// 		let mut rdis1: usize;
// 		let mut rpdis1: usize;
//
// 		let mut s = scope.start;
// 		let mut e = scope.end;
//
// 		/*
//          * Limits the window the is examined during the similar-lines
//          * scan. The loops below stops when dis[i - r] == 1 (line that
//          * has no match), but there are corner cases where the loop
//          * proceed all the way to the extremities by causing huge
//          * performance penalties in case of big files.
//          */
// 		if line_idx - s > XDL_SIMSCAN_WINDOW as usize {
// 			s = line_idx - XDL_SIMSCAN_WINDOW as usize;
// 		}
// 		if e - line_idx > XDL_SIMSCAN_WINDOW as usize {
// 			e = line_idx + XDL_SIMSCAN_WINDOW as usize;
// 		}
//
// 		/*
//          * Scans the lines before 'i' to find a run of lines that either
//          * have no match (dis[j] == 0) or have multiple matches (dis[j] > 1).
//          * Note that we always call this function with dis[i] > 1, so the
//          * current line (i) is already a multimatch line.
//          */
// 		r = 1;
// 		rdis0 = 0;
// 		rpdis0 = 1;
// 		while line_idx - r >= s {
// 			if self._consider[line_idx - r] == ConsiderLine::NONE_MATCH {
// 				rdis0 += 1;
// 			} else if self._consider[line_idx - r] == ConsiderLine::TOO_MANY {
// 				rpdis0 += 1;
// 			} else {
// 				break;
// 			}
//
// 			r += 1;
// 		}
// 		/*
//          * If the run before the line 'i' found only multimatch lines, we
//          * return 0 and hence we don't make the current line (i) discarded.
//          * We want to discard multimatch lines only when they appear in the
//          * middle of runs with nomatch lines (dis[j] == 0).
//          */
// 		if rdis0 == 0 {
// 			return false;
// 		}
// 		r = 1;
// 		rdis1 = 0;
// 		rpdis1 = 1;
// 		while line_idx +r <= e {
// 			// let t = *dis.add(line_idx +r);
// 			if self._consider[line_idx + r] == ConsiderLine::NONE_MATCH {
// 				rdis1 += 1;
// 			} else if self._consider[line_idx + r] == ConsiderLine::TOO_MANY {
// 				rpdis1 += 1;
// 			} else {
// 				break;
// 			}
//
// 			r += 1;
// 		}
// 		/*
//          * If the run after the line 'i' found only multimatch lines, we
//          * return 0 and hence we don't make the current line (i) discarded.
//          */
// 		if rdis1 == 0 {
// 			return false;
// 		}
// 		rdis1 += rdis0;
// 		rpdis1 += rpdis0;
//
// 		(rpdis1 * XDL_KPDIS_RUN as usize) < rpdis1 + rdis1
// 	}
//
//
// 	/*
//      * Try to reduce the problem complexity, discard records that have no
//      * matches on the other file. Also, lines that have multiple matches
//      * might be potentially discarded if they appear in a run of discardable.
//      */
// 	fn xdl_cleanup_records(&mut self, line_frequency: &XdiffHashMap<xrecord_t, Occurrence>, scope: &Range<usize>) {
// 		use ConsiderLine::*;
//
// 		/*
//          * first pass of deciding whether a line should be considered
//          * If there are no matches then NONE_MATCH, if the number of
//          * matches is >= match_limit i.e. at most XDL_MAX_EQLIMIT then
//          * TOO_MANY otherwise, do we consider the line? YES
//          */
// 		for (line_idx, line) in self.line_vec.iter().enumerate() {
// 			let occurrence = line_frequency.get(line);
// 			let number_of_matches = occurrence.map_or(0, |v| v.get(self.id));
//
// 			self._consider[line_idx] = if number_of_matches == 0 {
// 				NONE_MATCH
// 			} else if number_of_matches >= self.match_limit() {
// 				TOO_MANY
// 			} else {
// 				YES
// 			};
// 		}
//
// 		/*
//          * second pass of deciding whether a line should be considered
//          * Only look at the lines we should consider i.e. YES or
//          * TOO_MANY if xdl_clean_mmatch says so.
//          */
// 		for line_idx in 0..self.line_vec.len() {
// 			// we might change TOO_MANY to YES based on the context of the other lines
// 			if self._consider[line_idx] == YES || self._consider[line_idx] == TOO_MANY && !self.xdl_clean_mmatch(line_idx, &scope) {
// 				self._consider[line_idx] = YES;
// 				self.index.push(line_idx);
// 			} else {
// 				self._consider[line_idx] = NO;
// 			}
// 		}
// 	}
//
//
//
// }
//
//
//
//
// // pub struct xdfenv_t<'a> {
// // 	pub xdf1: xdfile_t<'a>,
// // 	pub xdf2: xdfile_t<'a>,
// // 	pub lines_that_differ: Range<usize>,
// // }
//
// impl<'a> xdfenv_t<'a> {
// 	pub fn new(file1: &'a [u8], file2: &'a [u8], flags: u64) -> Self {
// 		let mut xe = Self {
// 			xdf1: xdfile_t::new(file1, flags, 0),
// 			xdf2: xdfile_t::new(file2, flags, 1),
// 			lines_that_differ: Range::default(),
// 		};
// 		if (flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) != 0 {
// 			let mut line_frequency: XdiffHashMap<xrecord_t, Occurrence> = XdiffHashMap::default();
// 			line_frequency.reserve(xe.xdf1.line_vec.len()+xe.xdf2.line_vec.len());
// 			xe.xdf1.count_occurrences(&mut line_frequency);
// 			xe.xdf2.count_occurrences(&mut line_frequency);
// 			xe.xdl_optimize_ctxs(&line_frequency);
// 		}
// 		xe
// 	}
//
// 	pub fn line_equal(&self, file1_line_idx: usize, file2_line_idx: usize) -> bool {
// 		let mph1 = self.xdf1.line_vec[file1_line_idx].minimal_perfect_hash;
// 		let mph2 = self.xdf2.line_vec[file2_line_idx].minimal_perfect_hash;
// 		mph1 == mph2
// 	}
//
// 	/*
//      * Early trim initial and terminal matching records.
//      */
// 	fn xdl_trim_ends(&mut self) {
// 		self.lines_that_differ = INVALID_INDEX..INVALID_INDEX;
//
// 		// find the first line that differs between the 2 files
// 		let lim = std::cmp::min(self.xdf1.line_vec.len(), self.xdf2.line_vec.len());
// 		for i in 0..lim {
// 			if !self.line_equal(i, i) {
// 				self.lines_that_differ.start = i;
// 				break;
// 			}
// 		}
//
// 		// find the last line that differs between the 2 files
// 		for i in (0..lim).rev() {
// 			if !self.line_equal(i, i) {
// 				self.lines_that_differ.end = i;
// 				break;
// 			}
// 		}
// 	}
//
// 	fn xdl_optimize_ctxs(&mut self, line_frequency: &XdiffHashMap<xrecord_t, Occurrence>) {
// 		self.xdl_trim_ends();
// 		self.xdf1.xdl_cleanup_records(line_frequency, &self.lines_that_differ);
// 		self.xdf2.xdl_cleanup_records(line_frequency, &self.lines_that_differ);
// 	}
// }
