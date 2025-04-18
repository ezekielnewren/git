use std::cmp::Ordering;
use std::ops::Range;
use interop::ivec::IVec;
use crate::get_file_context;
use crate::xdiff::*;
use crate::xhistogram::{do_histogram_diff};
use crate::xpatience::{do_patience_diff};
use crate::xprepare::safe_2way_slice;
use crate::xtypes::*;
use crate::xutils::*;

const XDL_MAX_COST_MIN: isize = 256;
const XDL_HEUR_MIN_COST: isize = 256;
const XDL_SNAKE_CNT: isize = 20;
const XDL_K_HEUR: isize = 4;


/*
 * If a line is indented more than this, get_indent() just returns this value.
 * This avoids having to do absurd amounts of work for data that are not
 * human-readable text, and also ensures that the output of get_indent fits
 * within an int.
 */
const MAX_INDENT: isize = 200;

/*
 * If more than this number of consecutive blank rows are found, just return
 * this value. This avoids requiring O(N^2) work for pathological cases, and
 * also ensures that the output of score_split fits in an int.
 */
const MAX_BLANKS: isize = 20;


/*
 * The empirically-determined weight factors used by score_split() below.
 * Larger values means that the position is a less favorable place to split.
 *
 * Note that scores are only ever compared against each other, so multiplying
 * all of these weight/penalty values by the same factor wouldn't change the
 * heuristic's behavior. Still, we need to set that arbitrary scale *somehow*.
 * In practice, these numbers are chosen to be large enough that they can be
 * adjusted relative to each other with sufficient precision despite using
 * integer math.
 */

/* Penalty if there are no non-blank lines before the split */
const START_OF_FILE_PENALTY: isize = 1;

/* Penalty if there are no non-blank lines after the split */
const END_OF_FILE_PENALTY: isize = 21;

/* Multiplier for the number of blank lines around the split */
const TOTAL_BLANK_WEIGHT: isize = -30;

/* Multiplier for the number of blank lines after the split */
const POST_BLANK_WEIGHT: isize = 6;

/*
 * Penalties applied if the line is indented more than its predecessor
 */
const RELATIVE_INDENT_PENALTY: isize = -4;
const RELATIVE_INDENT_WITH_BLANK_PENALTY: isize = 10;

/*
 * Penalties applied if the line is indented less than both its predecessor and
 * its successor
 */
const RELATIVE_OUTDENT_PENALTY: isize = 24;
const RELATIVE_OUTDENT_WITH_BLANK_PENALTY: isize = 17;

/*
 * Penalties applied if the line is indented less than its predecessor but not
 * less than its successor
 */
const RELATIVE_DEDENT_PENALTY: isize = 23;
const RELATIVE_DEDENT_WITH_BLANK_PENALTY: isize = 17;

/*
 * We only consider whether the sum of the effective indents for splits are
 * less than (-1), equal to (0), or greater than (+1) each other. The resulting
 * value is multiplied by the following weight and combined with the penalty to
 * determine the better of two scores.
 */
const INDENT_WEIGHT: isize = 60;

/*
 * How far do we slide a hunk at most?
 */
const INDENT_HEURISTIC_MAX_SLIDING: isize = 100;


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


impl xdlgroup {

	fn new(ctx: &xd_file_context) -> Self {
		let mut g = Self {
			start: 0,
			end: 0,
		};

		while ctx.consider[SENTINEL + g.end as usize] != 0 {
			g.end += 1;
		}

		g
	}
}


/* Characteristics measured about a hypothetical split position. */
#[repr(C)]
struct split_measurement {
	/*
	 * Is the split at the end of the file (aside from any blank lines)?
	 */
	end_of_file: bool,

	/*
	 * How much is the line immediately following the split indented (or -1
	 * if the line is blank):
	 */
	indent: isize,

	/*
	 * How many consecutive lines above the split are blank?
	 */
	pre_blank: isize,

	/*
	 * How much is the nearest non-blank line above the split indented (or
	 * -1 if there is no such line)?
	 */
	pre_indent: isize,

	/*
	 * How many lines after the line following the split are blank?
	 */
	post_blank: isize,

	/*
	 * How much is the nearest non-blank line after the line following the
	 * split indented (or -1 if there is no such line)?
	 */
	post_indent: isize,
}


#[repr(C)]
struct split_score {
	/* The effective indent of this split (smaller is preferred). */
	effective_indent: isize,

	/* Penalty for this split (smaller is preferred). */
	penalty: isize,
}


/*
 * Return the amount of indentation of the specified line, treating TAB as 8
 * columns. Return -1 if line is empty or contains only whitespace. Clamp the
 * output value at MAX_INDENT.
 */
#[no_mangle]
unsafe extern "C" fn get_indent(rec: *const xrecord) -> isize {
	let line = (*rec).as_ref();

	let mut ret = 0;
	for byte in line.iter().copied() {
		if !XDL_ISSPACE(byte) {
			return ret;
		} else if byte == b' ' {
			ret += 1;
		} else if byte == b'\t' {
			ret += 8 - ret % 8;
		}
		/* ignore other whitespace characters */

		if ret >= MAX_INDENT {
			return MAX_INDENT;
		}
	}

	/* The line contains only whitespace. */
	-1
}


/*
 * Fill m with information about a hypothetical split of xdf above line split.
 */
#[no_mangle]
unsafe extern "C" fn measure_split(ctx: *const xd_file_context, split: isize, m: *mut split_measurement) {
	let ctx = xd_file_context::from_raw(ctx);
	let record = (*ctx.record).as_slice();
	let m = &mut *m;

	if (split as usize) >= record.len() {
		m.end_of_file = true;
		m.indent = -1;
	} else {
		m.end_of_file = false;
		m.indent = get_indent(&record[split as usize]);
	}

	m.pre_blank = 0;
	m.pre_indent = -1;
	// for (i = split - 1; i >= 0; i--) {
	for i in (0..split as usize).rev() {
		m.pre_indent = get_indent(&record[i]);
		if m.pre_indent != -1 {
			break;
		}
		m.pre_blank += 1;
		if m.pre_blank == MAX_BLANKS {
			m.pre_indent = 0;
			break;
		}
	}

	m.post_blank = 0;
	m.post_indent = -1;
	for i in split as usize + 1..record.len() {
		m.post_indent = get_indent(&record[i]);
		if m.post_indent != -1 {
			break;
		}
		m.post_blank += 1;
		if m.post_blank == MAX_BLANKS {
			m.post_indent = 0;
			break;
		}
	}
}


/*
 * Compute a badness score for the hypothetical split whose measurements are
 * stored in m. The weight factors were determined empirically using the tools
 * and corpus described in
 *
 *     https://github.com/mhagger/diff-slider-tools
 *
 * Also see that project if you want to improve the weights based on, for
 * example, a larger or more diverse corpus.
 */
#[no_mangle]
unsafe extern "C" fn score_add_split(m: *const split_measurement, s: *mut split_score) {
	let m = &*m;
	let s = &mut *s;

	/*
	 * A place to accumulate penalty factors (positive makes this index more
	 * favored):
	 */
	// int post_blank, total_blank, indent, any_blanks;

	if m.pre_indent == -1 && m.pre_blank == 0 {
		s.penalty += START_OF_FILE_PENALTY;
	}

	if m.end_of_file {
		s.penalty += END_OF_FILE_PENALTY;
	}

	/*
	 * Set post_blank to the number of blank lines following the split,
	 * including the line immediately after the split:
	 */
	let post_blank = if m.indent == -1 { 1 + m.post_blank } else { 0 };
	let total_blank = m.pre_blank + post_blank;

	/* Penalties based on nearby blank lines: */
	s.penalty += TOTAL_BLANK_WEIGHT * total_blank;
	s.penalty += POST_BLANK_WEIGHT * post_blank;

	let indent = if m.indent != -1 {
		m.indent
	} else {
		m.post_indent
	};

	let any_blanks = total_blank != 0;

	/* Note that the effective indent is -1 at the end of the file: */
	s.effective_indent += indent;

	if indent == -1 {
		/* No additional adjustments needed. */
	} else if m.pre_indent == -1 {
		/* No additional adjustments needed. */
	} else if indent > m.pre_indent {
		/*
		 * The line is indented more than its predecessor.
		 */
		s.penalty += if any_blanks {
			RELATIVE_INDENT_WITH_BLANK_PENALTY
		} else {
			RELATIVE_INDENT_PENALTY
		};
	} else if indent == m.pre_indent {
		/*
		 * The line has the same indentation level as its predecessor.
		 * No additional adjustments needed.
		 */
	} else {
		/*
		 * The line is indented less than its predecessor. It could be
		 * the block terminator of the previous block, but it could
		 * also be the start of a new block (e.g., an "else" block, or
		 * maybe the previous block didn't have a block terminator).
		 * Try to distinguish those cases based on what comes next:
		 */
		if m.post_indent != -1 && m.post_indent > indent {
			/*
			 * The following line is indented more. So it is likely
			 * that this line is the start of a block.
			 */
			s.penalty += if any_blanks {
				RELATIVE_OUTDENT_WITH_BLANK_PENALTY
			} else {
				RELATIVE_OUTDENT_PENALTY
			};
		} else {
			/*
			 * That was probably the end of a block.
			 */
			s.penalty += if any_blanks {
				RELATIVE_DEDENT_WITH_BLANK_PENALTY
			} else {
				RELATIVE_DEDENT_PENALTY
			};
		}
	}
}


#[no_mangle]
unsafe extern "C" fn score_cmp(s1: *const split_score, s2: *const split_score) -> isize {
	let s1 = &*s1;
	let s2 = &*s2;

	let cmp_indents: Ordering = s1.effective_indent.cmp(&s2.effective_indent);

	INDENT_WEIGHT * (cmp_indents as isize) + (s1.penalty - s2.penalty)
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
 * Move g to describe the next (possibly empty) group in xdf and return 0. If g
 * is already at the end of the file, do nothing and return -1.
 */
#[no_mangle]
unsafe extern "C" fn group_next(ctx: *const xd_file_context, g: *mut xdlgroup) -> i32 {
	let ctx = xd_file_context::from_raw(ctx);
	let g: &mut xdlgroup = &mut *g;

	if g.end as usize == (*ctx.record).len() {
		return -1;
	}

	g.start = g.end + 1;
	g.end = g.start;
	while ctx.consider[SENTINEL + g.end as usize] != NO {
		g.end += 1;
	}

	0
}


/*
 * Move g to describe the previous (possibly empty) group in xdf and return 0.
 * If g is already at the beginning of the file, do nothing and return -1.
 */
#[no_mangle]
unsafe extern "C" fn group_previous(ctx: *const xd_file_context, g: *mut xdlgroup) -> i32 {
	let ctx = xd_file_context::from_raw(ctx);
	let g = &mut *g;

	if g.start == 0 {
		return -1;
	}

	g.end = g.start - 1;
	g.start = g.end;
	while ctx.consider[SENTINEL + g.start as usize - 1] != NO {
		g.start -= 1;
	}

	0
}


/*
 * If g can be slid toward the end of the file, do so, and if it bumps into a
 * following group, expand this group to include it. Return 0 on success or -1
 * if g cannot be slid down.
 */
#[no_mangle]
unsafe extern "C" fn group_slide_down(ctx: *mut xd_file_context, g: *mut xdlgroup) -> i32 {
	let ctx = xd_file_context::from_raw_mut(ctx);
	let g = &mut *g;
	let mph = (*ctx.minimal_perfect_hash).as_slice();

	if (g.end as usize) < mph.len() && mph[g.start as usize] == mph[g.end as usize] {
		ctx.consider[SENTINEL + g.start as usize] = NO;
		g.start += 1;
		ctx.consider[SENTINEL + g.end as usize] = YES;
		g.end += 1;

		while ctx.consider[SENTINEL + g.end as usize] != NO {
			g.end += 1;
		}

		return 0;
	}

	-1
}

/*
 * If g can be slid toward the beginning of the file, do so, and if it bumps
 * into a previous group, expand this group to include it. Return 0 on success
 * or -1 if g cannot be slid up.
 */
#[no_mangle]
unsafe extern "C" fn group_slide_up(ctx: *mut xd_file_context, g: *mut xdlgroup) -> i32 {
	let ctx = xd_file_context::from_raw_mut(ctx);
	let g = &mut *g;
	let mph = (*ctx.minimal_perfect_hash).as_slice();

	if g.start > 0 && mph[g.start as usize - 1] == mph[g.end as usize - 1] {
		g.start -= 1;
		ctx.consider[SENTINEL + g.start as usize] = YES;
		g.end -= 1;
		ctx.consider[SENTINEL + g.end as usize] = NO;

		while ctx.consider[SENTINEL + g.start as usize - 1] != NO {
			g.start -= 1;
		}

		return 0;
	}

	-1
}


/*
 * Move back and forward change groups for a consistent and pretty diff output.
 * This also helps in finding joinable change groups and reducing the diff
 * size.
 */
#[no_mangle]
unsafe extern "C" fn xdl_change_compact(ctx: *mut xd_file_context, ctx_out: *mut xd_file_context, flags: u64) -> i32 {
	let ctx = xd_file_context::from_raw_mut(ctx);
	let ctx_out = xd_file_context::from_raw_mut(ctx_out);

	let mut g = xdlgroup::new(ctx);
	let mut go = xdlgroup::new(ctx_out);

	let mut earliest_end: isize;
	let mut end_matching_other: isize;
	let mut groupsize: isize;

	loop {
		/*
		 * If the group is empty in the to-be-compacted file, skip it:
		 */
		if g.end != g.start {
			/*
			 * Now shift the change up and then down as far as possible in
			 * each direction. If it bumps into any other changes, merge
			 * them.
			 */
			loop {
				groupsize = g.end - g.start;

				/*
				 * Keep track of the last "end" index that causes this
				 * group to align with a group of changed lines in the
				 * other file. -1 indicates that we haven't found such
				 * a match yet:
				 */
				end_matching_other = -1;

				/* Shift the group backward as much as possible: */
				while group_slide_up(ctx, &mut g) == 0 {
					if group_previous(ctx_out, &mut go) != 0 {
						panic!("group sync broken sliding up");
					}
				}

				/*
				 * This is this highest that this group can be shifted.
				 * Record its end index:
				 */
				earliest_end = g.end;

				if go.end > go.start {
					end_matching_other = g.end;
				}

				/* Now shift the group forward as far as possible: */
				loop {
					if group_slide_down(ctx, &mut g) != 0 {
						break;
					}
					if group_next(ctx_out, &mut go) != 0 {
						panic!("group sync broken sliding down");
					}

					if go.end > go.start {
						end_matching_other = g.end;
					}
				}

				if groupsize == g.end - g.start {
					break;
				}
			}

			/*
			 * If the group can be shifted, then we can possibly use this
			 * freedom to produce a more intuitive diff.
			 *
			 * The group is currently shifted as far down as possible, so
			 * the heuristics below only have to handle upwards shifts.
			 */

			if g.end == earliest_end {
				/* no shifting was possible */
			} else if end_matching_other != -1 {
				/*
				 * Move the possibly merged group of changes back to
				 * line up with the last group of changes from the
				 * other file that it can align with.
				 */
				while go.end == go.start {
					if group_slide_up(ctx, &mut g) != 0 {
						panic!("match disappeared");
					}
					if group_previous(ctx_out, &mut go) != 0 {
						panic!("group sync broken sliding to match");
					}
				}
			} else if (flags & XDF_INDENT_HEURISTIC) != 0 {
				/*
				 * Indent heuristic: a group of pure add/delete lines
				 * implies two splits, one between the end of the
				 * "before" context and the start of the group, and
				 * another between the end of the group and the
				 * beginning of the "after" context. Some splits are
				 * aesthetically better and some are worse. We compute
				 * a badness "score" for each split, and add the scores
				 * for the two splits to define a "score" for each
				 * position that the group can be shifted to. Then we
				 * pick the shift with the lowest score.
				 */
				let mut best_shift = -1;
				let mut best_score = split_score {
					effective_indent: 0,
					penalty: 0,
				};

				let mut shift = earliest_end;
				if g.end - groupsize - 1 > shift {
					shift = g.end - groupsize - 1;
				}
				if g.end - INDENT_HEURISTIC_MAX_SLIDING > shift {
					shift = g.end - INDENT_HEURISTIC_MAX_SLIDING;
				}
				while shift <= g.end {
					let mut m = split_measurement {
						end_of_file: false,
						indent: 0,
						pre_blank: 0,
						pre_indent: 0,
						post_blank: 0,
						post_indent: 0,
					};
					let mut score = split_score {
						effective_indent: 0,
						penalty: 0,
					};

					measure_split(ctx, shift, &mut m);
					score_add_split(&m, &mut score);
					measure_split(ctx, shift - groupsize, &mut m);
					score_add_split(&m, &mut score);
					if best_shift == -1 ||
					    score_cmp(&score, &best_score) <= 0 {
						best_score.effective_indent = score.effective_indent;
						best_score.penalty = score.penalty;
						best_shift = shift;
					    }

					shift += 1;
				}

				while g.end > best_shift {
					if group_slide_up(ctx, &mut g) != 0 {
						panic!("best shift unreached");
					}
					if group_previous(ctx_out, &mut go) != 0 {
						panic!("group sync broken sliding to blank line");
					}
				}
			}
		}

		/* Move past the just-processed group: */
		if group_next(ctx, &mut g) != 0 {
			break;
		}
		if group_next(ctx_out, &mut go) != 0 {
			panic!("group sync broken moving to next group");
		}
	}

	if !group_next(ctx_out, &mut go) != 0 {
		panic!("group sync broken at end of file");
	}

	0
}


#[cfg(test)]
mod tests {

	#[test]
	fn compile_this_file() {

	}

}

