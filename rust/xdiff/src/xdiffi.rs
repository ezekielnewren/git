use std::ops::Range;
use interop::ivec::IVec;
use crate::get_file_context;
use crate::xdiff::*;
use crate::xhistogram::{do_histogram_diff};
use crate::xpatience::{do_patience_diff};
use crate::xprepare::safe_2way_slice;
use crate::xtypes::*;
use crate::xutils::xdl_bogosqrt;

const XDL_MAX_COST_MIN: isize = 256;
const XDL_HEUR_MIN_COST: isize = 256;
const XDL_SNAKE_CNT: isize = 20;
const XDL_K_HEUR: isize = 4;


#[repr(C)]
pub(crate) struct xdpsplit {
    pub(crate) i1: isize,
    pub(crate) i2: isize,
    pub(crate) min_lo: bool,
    pub(crate) min_hi: bool,
}

impl xdpsplit {
	unsafe fn from_raw_mut<'a>(spl: *mut xdpsplit) -> &'a mut xdpsplit {
		if spl.is_null() {
			panic!("null pointer");
		}

		&mut *spl
	}
}

#[repr(C)]
pub(crate) struct xdalgoenv {
    pub(crate) mxcost: isize,
    pub(crate) snake_cnt: isize,
    pub(crate) heur_min: isize,
}

impl xdalgoenv {
	unsafe fn from_raw_mut<'a>(xenv: *mut xdalgoenv) -> &'a mut xdalgoenv {
		if xenv.is_null() {
			panic!("null pointer");
		}

		&mut *xenv
	}
}
#[repr(C)]
pub(crate) struct xdchange {
    pub(crate) next: *mut xdchange,
    pub(crate) i1: isize,
    pub(crate) i2: isize,
    pub(crate) chg1: isize,
    pub(crate) chg2: isize,
    pub(crate) ignore: bool,
}


/*
 * Represent a group of changed lines in an xdfile_t (i.e., a contiguous group
 * of lines that was inserted or deleted from the corresponding version of the
 * file). We consider there to be such a group at the beginning of the file, at
 * the end of the file, and between any two unchanged lines, though most such
 * groups will usually be empty.
 *
 * If the first line in a group is equal to the line following the group, then
 * the group can be slid down. Similarly, if the last line in a group is equal
 * to the line preceding the group, then the group can be slid up. See
 * group_slide_down() and group_slide_up().
 *
 * Note that loops that are testing for changed lines in xdf->rchg do not need
 * index bounding since the array is prepared with a zero at position -1 and N.
 */
#[repr(C)]
pub(crate) struct xdlgroup {
	/*
	 * The index of the first changed line in the group, or the index of
	 * the unchanged line above which the (empty) group is located.
	 */
	start: isize,

	/*
	 * The index of the first unchanged line after the group. For an empty
	 * group, end is equal to start.
	 */
	end: isize,
}


fn get_mph(ctx: &FileContext, index: usize) -> u64 {
	ctx.minimal_perfect_hash[ctx.rindex[index]]
}


/// See "An O(ND) Difference Algorithm and its Variations", by Eugene Myers.
/// Basically considers a "box" (off1, off2, lim1, lim2) and scan from both
/// the forward diagonal starting from (off1, off2) and the backward diagonal
/// starting from (lim1, lim2). If the K values on the same diagonal crosses
/// returns the furthest point of reach. We might encounter expensive edge cases
/// using this algorithm, so a little bit of heuristic is needed to cut the
/// search and to return a suboptimal point.
fn split(ctx1: &mut FileContext, off1: isize, lim1: isize,
		       ctx2: &mut FileContext, off2: isize, lim2: isize,
		       kvd_off: isize, kvdf: &mut Vec<isize>, kvdb: &mut Vec<isize>,
		       need_min: bool, spl: &mut xdpsplit, xenv: &mut xdalgoenv) -> isize {
	let dmin = off1 - lim2;
    let dmax = lim1 - off2;
	let fmid = off1 - off2;
    let bmid = lim1 - lim2;
	let odd = (fmid - bmid) & 1 != 0;
	let mut fmin = fmid;
    let mut fmax = fmid;
	let mut bmin = bmid;
    let mut bmax = bmid;
    let mut i1;
    let mut i2;
    let mut prev1;
    let mut dd;
    let mut v;

	/*
	 * Set initial diagonal values for both forward and backward path.
	 */
	kvdf[(kvd_off + fmid) as usize] = off1;
	kvdb[(kvd_off + bmid) as usize] = lim1;

	for ec in 1..isize::MAX {
		let mut got_snake = false;

		/*
		 * We need to extend the diagonal "domain" by one. If the next
		 * values exits the box boundaries we need to change it in the
		 * opposite direction because (max - min) must be a power of
		 * two.
		 *
		 * Also we initialize the external K value to -1 so that we can
		 * avoid extra conditions in the check inside the core loop.
		 */
		if fmin > dmin {
			fmin -= 1;
			kvdf[(kvd_off + fmin - 1) as usize] = -1;
		} else {
			fmin += 1;
		}
		if fmax < dmax {
			fmax += 1;
			kvdf[(kvd_off + fmax + 1) as usize] = -1;
		} else {
			fmax -= 1;
		}

		for d in (fmin..=fmax).rev().step_by(2) {
			if kvdf[(kvd_off + d - 1) as usize] >= kvdf[(kvd_off + d + 1) as usize] {
				i1 = kvdf[(kvd_off + d - 1) as usize] + 1;
			} else {
				i1 = kvdf[(kvd_off + d + 1) as usize];
			}
			prev1 = i1;
			i2 = i1 - d;
			while i1 < lim1 && i2 < lim2 {
				if get_mph(&ctx1, i1 as usize) != get_mph(&ctx2, i2 as usize) {
					break;
				}
				i1 += 1;
				i2 += 1;
			}
			if i1 - prev1 > (*xenv).snake_cnt {
				got_snake = true;
			}
			kvdf[(kvd_off + d) as usize] = i1;
			if odd && bmin <= d && d <= bmax && kvdb[(kvd_off + d) as usize] <= i1 {
				spl.i1 = i1;
				spl.i2 = i2;
				spl.min_lo = true;
				spl.min_hi = true;
				return ec;
			}
		}

		/*
		 * We need to extend the diagonal "domain" by one. If the next
		 * values exits the box boundaries we need to change it in the
		 * opposite direction because (max - min) must be a power of
		 * two.
		 *
		 * Also we initialize the external K value to -1 so that we can
		 * avoid extra conditions in the check inside the core loop.
		 */
		if bmin > dmin {
			bmin -= 1;
			kvdb[(kvd_off + bmin - 1) as usize] = isize::MAX;
		} else {
			bmin += 1;
		}
		if bmax < dmax {
			bmax += 1;
			kvdb[(kvd_off + bmax + 1) as usize] = isize::MAX;
		} else {
			bmax -= 1;
		}

		for d in (bmin..=bmax).rev().step_by(2) {
			if kvdb[(kvd_off + d - 1) as usize] < kvdb[(kvd_off + d + 1) as usize] {
				i1 = kvdb[(kvd_off + d - 1) as usize];
			} else {
				i1 = kvdb[(kvd_off + d + 1) as usize] - 1;
			}
			prev1 = i1;
			i2 = i1 - d;
			while i1 > off1 && i2 > off2 {
				if get_mph(&ctx1, (i1 - 1) as usize) != get_mph(&ctx2, (i2 - 1) as usize) {
					break;
				}
				i1 -= 1;
				i2 -= 1;
			}
			if prev1 - i1 > xenv.snake_cnt {
				got_snake = true;
			}
			kvdb[(kvd_off + d) as usize] = i1;
			if !odd && fmin <= d && d <= fmax && i1 <= kvdf[(kvd_off + d) as usize] {
				spl.i1 = i1;
				spl.i2 = i2;
				spl.min_lo = true;
				spl.min_hi = true;
				return ec;
			}
		}

		if need_min {
			continue;
		}

		/*
		 * If the edit cost is above the heuristic trigger and if
		 * we got a good snake, we sample current diagonals to see
		 * if some of them have reached an "interesting" path. Our
		 * measure is a function of the distance from the diagonal
		 * corner (i1 + i2) penalized with the distance from the
		 * mid diagonal itself. If this value is above the current
		 * edit cost times a magic factor (XDL_K_HEUR) we consider
		 * it interesting.
		 */
		if got_snake && ec > xenv.heur_min {
			let mut best = 0isize;
			for d in (fmin..=fmax).rev().step_by(2) {
				dd = if d > fmid { d - fmid } else { fmid - d };
				i1 = kvdf[(kvd_off + d) as usize];
				i2 = i1 - d;
				v = (i1 - off1) + (i2 - off2) - dd;

				if v > XDL_K_HEUR * ec && v > best &&
				    off1 + xenv.snake_cnt <= i1 && i1 < lim1 &&
				    off2 + xenv.snake_cnt <= i2 && i2 < lim2 {
					for k in 1..isize::MAX {
						if get_mph(&ctx1, (i1 - k) as usize) != get_mph(&ctx2, (i2 - k) as usize) {
							break;
						}
						if k == xenv.snake_cnt {
							best = v;
							spl.i1 = i1;
							spl.i2 = i2;
							break;
						}
					}
				}
			}
			if best > 0 {
				spl.min_lo = true;
				spl.min_hi = false;
				return ec;
			}

			let mut best = 0;
			for d in (bmin..=bmax).rev().step_by(2) {
				dd = if d > bmid { d - bmid } else { bmid - d };
				i1 = kvdb[(kvd_off + d) as usize];
				i2 = i1 - d;
				v = (lim1 - i1) + (lim2 - i2) - dd;

				if v > XDL_K_HEUR * ec && v > best &&
				    off1 < i1 && i1 <= lim1 - xenv.snake_cnt &&
				    off2 < i2 && i2 <= lim2 - xenv.snake_cnt {
					for k in 0..isize::MAX {
						if get_mph(&ctx1, (i1 + k) as usize) != get_mph(&ctx2, (i2 + k) as usize) {
							break;
						}
						if k == xenv.snake_cnt - 1 {
							best = v;
							spl.i1 = i1;
							spl.i2 = i2;
							break;
						}
					}
				}
			}
			if best > 0 {
				spl.min_lo = false;
				spl.min_hi = true;
				return ec;
			}
		}

		/*
		 * Enough is enough. We spent too much time here and now we
		 * collect the furthest reaching path using the (i1 + i2)
		 * measure.
		 */
		if ec >= xenv.mxcost {
			let mut fbest = -1;
			let mut fbest1 = -1;
			let mut bbest;
			let mut bbest1;

			for d in (fmin..=fmax).rev().step_by(2) {
				i1 = std::cmp::min(kvdf[(kvd_off + d) as usize], lim1);
				i2 = i1 - d;
				if lim2 < i2 {
					i1 = lim2 + d;
					i2 = lim2;
				}
				if fbest < i1 + i2 {
					fbest = i1 + i2;
					fbest1 = i1;
				}
			}

			bbest = isize::MAX;
			bbest1 = isize::MAX;
			for d in (bmin..=bmax).rev().step_by(2) {
				i1 = std::cmp::max(off1, kvdb[(kvd_off + d) as usize]);
				i2 = i1 - d;
				if i2 < off2 {
					i1 = off2 + d;
					i2 = off2;
				}
				if i1 + i2 < bbest {
					bbest = i1 + i2;
					bbest1 = i1;
				}
			}

			if (lim1 + lim2) - bbest < fbest - (off1 + off2) {
				spl.i1 = fbest1;
				spl.i2 = fbest - fbest1;
				spl.min_lo = true;
				spl.min_hi = false;
			} else {
				spl.i1 = bbest1;
				spl.i2 = bbest - bbest1;
				spl.min_lo = false;
				spl.min_hi = true;
			}
			return ec;
		}
	}

	unreachable!();
}


/// Rule: "Divide et Impera" (divide & conquer). Recursively split the box in
/// sub-boxes by calling the box splitting function. Note that the real job
/// (marking changed lines) is done in the two boundary reaching checks.
fn recs_cmp(
	ctx1: &mut FileContext, mut off1: isize, mut lim1: isize,
	ctx2: &mut FileContext, mut off2: isize, mut lim2: isize,
	kvd_off: isize, kvdf: &mut Vec<isize>, kvdb: &mut Vec<isize>,
	need_min: bool, xenv: &mut xdalgoenv
) -> i32 {

	/*
	 * Shrink the box by walking through each diagonal snake (SW and NE).
	 */
	while off1 < lim1 && off2 < lim2 {
		if get_mph(&ctx1, off1 as usize) != get_mph(&ctx2, off2 as usize) {
			break;
		}
		off1 += 1;
		off2 += 1;
	}

	while off1 < lim1 && off2 < lim2 {
		if get_mph(&ctx1, (lim1 - 1) as usize) != get_mph(&ctx2, (lim2 - 1) as usize) {
			break;
		}
		lim1 -= 1;
		lim2 -= 1;
	}

	/*
	 * If one dimension is empty, then all records on the other one must
	 * be obviously changed.
	 */
	if off1 == lim1 {
		while off2 < lim2 {
			ctx2.consider[SENTINEL + ctx2.rindex[off2 as usize]] = YES;
			off2 += 1;
		}
	} else if off2 == lim2 {
		while off1 < lim1 {
			ctx1.consider[SENTINEL + ctx1.rindex[off1 as usize]] = YES;
			off1 += 1;
		}
	} else {
		let mut spl = xdpsplit {
			i1: 0,
			i2: 0,
			min_lo: false,
			min_hi: false,
		};

		/*
		 * Divide ...
		 */
		if split(ctx1, off1, lim1, ctx2, off2, lim2, kvd_off, kvdf, kvdb,
			      need_min, &mut spl, xenv) < 0 {

			return -1;
		}

		/*
		 * ... et Impera.
		 */
		if recs_cmp(ctx1, off1, spl.i1, ctx2, off2, spl.i2,
				 kvd_off, kvdf, kvdb, spl.min_lo, xenv) < 0 ||
		    recs_cmp(ctx1, spl.i1, lim1, ctx2, spl.i2, lim2,
				 kvd_off,  kvdf, kvdb, spl.min_hi, xenv) < 0 {

			return -1;
		}
	}

	return 0;
}


const XDL_KPDIS_RUN: usize = 4;
const XDL_MAX_EQLIMIT: u64 = 1024;
const XDL_SIMSCAN_WINDOW: usize = 100;


fn clean_mmatch(dis: &mut IVec<u8>, i: usize, mut start: usize, mut end: usize) -> bool {
	/*
	 * Limits the window the is examined during the similar-lines
	 * scan. The loops below stops when dis[i - r] == 1 (line that
	 * has no match), but there are corner cases where the loop
	 * proceed all the way to the extremities by causing huge
	 * performance penalties in case of big files.
	 */
	if i - start > XDL_SIMSCAN_WINDOW {
		start = i - XDL_SIMSCAN_WINDOW;
	}
	if end - i > XDL_SIMSCAN_WINDOW {
		end = i + XDL_SIMSCAN_WINDOW;
	}

	/*
	 * Scans the lines before 'i' to find a run of lines that either
	 * have no match (dis[j] == 0) or have multiple matches (dis[j] > 1).
	 * Note that we always call this function with dis[i] > 1, so the
	 * current line (i) is already a multimatch line.
	 */
	let mut rdis0 = 0;
	let mut rpdis0 = 1;
	for i0 in (start..i).rev() {
		if dis[i0] == NO {
			rdis0 += 1;
		} else if dis[i0] == TOO_MANY {
			rpdis0 += 1;
		} else {
			break;
		}
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
	let mut rdis1 = 0;
	let mut rpdis1 = 1;
	for i1 in i + 1..end {
		if dis[i1] == NO {
			rdis1 += 1;
		} else if dis[i1] == TOO_MANY {
			rpdis1 += 1;
		} else {
			break;
		}
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

#[repr(C)]
#[derive(Default)]
struct Occurrence {
	file1: usize,
	file2: usize,
}

/// Try to reduce the problem complexity, discard records that have no
/// matches on the other file. Also, lines that have multiple matches
/// might be potentially discarded if they appear in a run of discardable.
fn cleanup_records(pair: &mut xdpair) {
	let (lhs, rhs) = get_file_context!(pair);

	let end1 = lhs.record.len() - pair.delta_end;
	let end2 = rhs.record.len() - pair.delta_end;

	/*
	 * record.length for dis1, dis2 because this function
	 * and xdl_clean_mmatch() does bounds checking,
	 * everywhere else uses the sentinel values to stop
	 * iteration
	 */
	let mut dis1 = IVec::new();
	dis1.resize(lhs.record.len(), NO);
	let mut dis2 = IVec::new();
	dis2.resize(rhs.record.len(), NO);
	let mut occurrence = Vec::new();
	occurrence.resize_with(pair.minimal_perfect_hash_size, || Occurrence::default());

	for mph in lhs.minimal_perfect_hash {
		occurrence[*mph as usize].file1 += 1;
	}

	for mph in rhs.minimal_perfect_hash {
		occurrence[*mph as usize].file2 += 1;
	}

	let mlim1 = std::cmp::min(xdl_bogosqrt(lhs.record.len() as u64), XDL_MAX_EQLIMIT) as usize;
	for i in pair.delta_start..end1 {
		let mph = lhs.minimal_perfect_hash[i];
		let nm = occurrence[mph as usize].file2;
		dis1[i] = if nm == 0 {
			NO
		} else if nm >= mlim1 {
			TOO_MANY
		} else {
			YES
		};
	}

	let mlim2 = std::cmp::min(xdl_bogosqrt(rhs.record.len() as u64), XDL_MAX_EQLIMIT) as usize;
	for i in pair.delta_start..end2 {
		let mph = rhs.minimal_perfect_hash[i];
		let nm = occurrence[mph as usize].file1;
		dis2[i] = if nm == 0 {
			NO
		} else if nm >= mlim2 {
			TOO_MANY
		} else {
			YES
		};
	}

	for i in pair.delta_start..end1 {
		if dis1[i] == YES ||
			(dis1[i] == TOO_MANY && !clean_mmatch(&mut dis1, i, pair.delta_start, end1)) {
			lhs.rindex.push(i);
		} else {
			lhs.consider[SENTINEL + i] = YES;
		}
	}
	lhs.rindex.shrink_to_fit();

	for i in pair.delta_start..end2 {
		if dis2[i] == YES ||
			(dis2[i] == TOO_MANY && !clean_mmatch(&mut dis2, i, pair.delta_start, end2)) {
			rhs.rindex.push(i);
		} else {
			rhs.consider[SENTINEL + i] = YES;
		}
	}
	rhs.rindex.shrink_to_fit();
}

fn trim_ends(pair: &mut xdpair) {
	let (lhs, rhs) = get_file_context!(pair);

	let mut lim = std::cmp::min(lhs.record.len(), rhs.record.len());

	for i in 0..lim {
		let mph1 = lhs.minimal_perfect_hash[i];
		let mph2 = rhs.minimal_perfect_hash[i];
		if mph1 != mph2 {
			lim -= i;
			pair.delta_start = i;
			break;
		}
	}

	for i in 0..lim {
		let mph1 = lhs.minimal_perfect_hash[lhs.minimal_perfect_hash.len() - 1 - i];
		let mph2 = rhs.minimal_perfect_hash[rhs.minimal_perfect_hash.len() - 1 - i];
		if mph1 != mph2 {
			pair.delta_end = i;
			break;
		}
	}
}

fn optimize_ctxs(pair: &mut xdpair) {
	trim_ends(pair);
	cleanup_records(pair);
}


pub(crate) fn classic_diff(flags: u64, pair: &mut xdpair) -> i32 {
	optimize_ctxs(pair);

	let mut xenv = xdalgoenv {
		mxcost: 0,
		snake_cnt: 0,
		heur_min: 0,
	};

	/*
	 * Allocate and setup K vectors to be used by the differential
	 * algorithm.
	 *
	 * One is to store the forward path and one to store the backward path.
	 */
	let ndiags = pair.lhs.rindex.len() + pair.rhs.rindex.len() + 3;
	let mut kvdf = vec![0isize; ndiags];
	let mut kvdb = vec![0isize; 2 * ndiags + 2 - ndiags];

	let kvd_off = (pair.rhs.rindex.len() + 1) as isize;

	xenv.mxcost = xdl_bogosqrt(ndiags as u64) as isize;
	if xenv.mxcost < XDL_MAX_COST_MIN {
		xenv.mxcost = XDL_MAX_COST_MIN;
	}
	xenv.snake_cnt = XDL_SNAKE_CNT;
	xenv.heur_min = XDL_HEUR_MIN_COST;

	let mut lhs = FileContext::new(&mut pair.lhs);
	let mut rhs = FileContext::new(&mut pair.rhs);

	let a = lhs.rindex.len() as isize;
	let b = rhs.rindex.len() as isize;

	recs_cmp(&mut lhs, 0, a, &mut rhs, 0, b,
			   kvd_off, &mut kvdf, &mut kvdb, (flags & XDF_NEED_MINIMAL) != 0,
			   &mut xenv)
}


/// The indexedness of the ranges are based on the LINE_SHIFT constant.
/// When this function was written LINE_SHIFT had a value of 1,
/// which means the ranges use 1-based indexing.
pub(crate) fn classic_diff_with_range(flags: u64, pair: &mut xdpair, mut range1: Range<usize>, mut range2: Range<usize>) -> i32 {
	let (lhs, rhs) = get_file_context!(pair);

	let mut two_way = xd2way::default();
	let r1 = range1.start - LINE_SHIFT..range1.end - LINE_SHIFT;
	let r2 = range2.start - LINE_SHIFT..range2.end - LINE_SHIFT;
	safe_2way_slice(&lhs, r1, &rhs, r2, pair.minimal_perfect_hash_size, &mut two_way);

	let result = classic_diff(flags, &mut two_way.pair);

	range1.start += SENTINEL - LINE_SHIFT;
	range1.end += SENTINEL - LINE_SHIFT;
	let dst = &mut lhs.consider[range1.clone()];
	let src = &two_way.pair.lhs.consider.as_slice()[SENTINEL..SENTINEL + range1.len()];
	dst.copy_from_slice(src);

	range2.start += SENTINEL - LINE_SHIFT;
	range2.end += SENTINEL - LINE_SHIFT;
	let dst = &mut rhs.consider[range2.clone()];
	let src = &two_way.pair.rhs.consider.as_slice()[SENTINEL..SENTINEL + range2.len()];
	dst.copy_from_slice(src);

	result
}


pub(crate) fn do_diff(xpp: &xpparam_t, pair: &mut xdpair) -> i32 {
	if (xpp.flags & XDF_PATIENCE_DIFF) != 0 {
		return do_patience_diff(xpp, pair);
	}

	if (xpp.flags & XDF_HISTOGRAM_DIFF) != 0 {
		return do_histogram_diff(xpp.flags, pair);
	}

	classic_diff(xpp.flags, pair)
}


/*
 * Initialize g to point at the first group in xdf.
 */
#[no_mangle]
unsafe extern "C" fn group_init(ctx: *const xd_file_context, g: *mut xdlgroup) {
	let ctx = xd_file_context::from_raw(ctx);
	let g = &mut *g;

	g.start = 0;
	g.end = 0;
	while ctx.consider[SENTINEL + g.end as usize] != 0 {
		g.end += 1;
	}
}



#[cfg(test)]
mod tests {

	#[test]
	fn compile_this_file() {

	}

}

