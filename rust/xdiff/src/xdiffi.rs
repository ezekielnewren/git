use interop::ivec::IVec;
use crate::xtypes::{xd_file_context, FileContext};


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
#[no_mangle]
unsafe extern "C" fn xdl_split(ctx1: *mut xd_file_context, off1: isize, lim1: isize,
		       ctx2: *mut xd_file_context, off2: isize, lim2: isize,
		       kvd_off: isize, kvdf: *mut IVec<isize>, kvdb: *mut IVec<isize>,
		       need_min: bool, spl: *mut xdpsplit, xenv: *mut xdalgoenv) -> isize {
	let ctx1 = xd_file_context::from_raw_mut(ctx1);
	let ctx1 = FileContext::new(ctx1);
	let ctx2 = xd_file_context::from_raw_mut(ctx2);
	let ctx2 = FileContext::new(ctx2);

	let kvdf = IVec::from_raw_mut(kvdf);
	let kvdb = IVec::from_raw_mut(kvdb);

	let spl = xdpsplit::from_raw_mut(spl);
	let xenv = xdalgoenv::from_raw_mut(xenv);

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

#[cfg(test)]
mod tests {

	#[test]
	fn compile_this_file() {

	}

}

