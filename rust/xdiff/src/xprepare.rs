use interop::ivec::IVec;
use crate::get_file_context;
use crate::xdiff::*;
use crate::xtypes::{parse_lines, FileContext, xd2way, xd3way, xd_file_context, xdfile, xdpair, xrecord};
use crate::xutils::{xdl_bogosqrt, MinimalPerfectHashBuilder};


const XDL_KPDIS_RUN: usize = 4;
const XDL_MAX_EQLIMIT: u64 = 1024;
const XDL_SIMSCAN_WINDOW: usize = 100;


#[no_mangle]
unsafe extern "C" fn xdl_clean_mmatch(dis: *mut IVec<u8>, i: usize, mut start: usize, mut end: usize) -> bool {
	let dis = IVec::from_raw_mut(dis);
	clean_mmatch(dis, i, start, end)
}


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
fn xdl_cleanup_records(pair: &mut xdpair) {
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

	for mph in lhs.minimal_perfect_hash.as_slice() {
		occurrence[*mph as usize].file1 += 1;
	}

	for mph in rhs.minimal_perfect_hash.as_slice() {
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

fn xdl_trim_ends(pair: &mut xdpair) {
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

fn xdl_optimize_ctxs(pair: &mut xdpair) {
	xdl_trim_ends(pair);
	xdl_cleanup_records(pair);
}

fn xdl_setup_ctx(file: &xdfile, ctx: &mut xd_file_context) {
	ctx.minimal_perfect_hash = &file.minimal_perfect_hash as *const IVec<u64> as *mut IVec<u64>;
	ctx.record = &file.record as *const IVec<xrecord> as *mut IVec<xrecord>;
	ctx.consider = unsafe { IVec::zero(SENTINEL + file.record.len() + SENTINEL) };
	ctx.rindex = IVec::new();
}

fn xdl_pair_prepare(
	lhs: &mut xdfile, rhs: &mut xdfile, mph_size: usize,
	flags: u64, pair: &mut xdpair
) {
	pair.lhs = xd_file_context::default();
	pair.rhs = xd_file_context::default();
	pair.delta_start = 0;
	pair.delta_end = 0;
	pair.minimal_perfect_hash_size = mph_size;

	xdl_setup_ctx(lhs, &mut pair.lhs);
	xdl_setup_ctx(rhs, &mut pair.rhs);

	if (flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0 {
		xdl_optimize_ctxs(pair);
	}
}

pub fn safe_2way_prepare(file1: &[u8], file2: &[u8], flags: u64, two_way: &mut xd2way) {
	parse_lines(file1, &mut two_way.lhs.record);
	parse_lines(file2, &mut two_way.rhs.record);

	let mut max_unique_keys = 0;
	max_unique_keys += two_way.lhs.record.len();
	max_unique_keys += two_way.rhs.record.len();
	let mut mphb = MinimalPerfectHashBuilder::new(max_unique_keys, flags);

	mphb.process(&mut two_way.lhs);
	mphb.process(&mut two_way.rhs);
	two_way.minimal_perfect_hash_size = mphb.finish();

	xdl_pair_prepare(&mut two_way.lhs, &mut two_way.rhs,
					 two_way.minimal_perfect_hash_size, flags, &mut two_way.pair);
}


pub fn safe_3way_prepare(
	base: &[u8], side1: &[u8], side2: &[u8],
	flags: u64, three_way: &mut xd3way
) {
	parse_lines(base,  &mut three_way.base.record);
	parse_lines(side1, &mut three_way.side1.record);
	parse_lines(side2, &mut three_way.side2.record);

	let mut max_unique_keys = 0;
	max_unique_keys += three_way.base.record.len();
	max_unique_keys += three_way.side1.record.len();
	max_unique_keys += three_way.side2.record.len();
	let mut mphb = MinimalPerfectHashBuilder::new(max_unique_keys, flags);

	mphb.process(&mut three_way.base);
	mphb.process(&mut three_way.side1);
	mphb.process(&mut three_way.side2);
	three_way.minimal_perfect_hash_size = mphb.finish();

	xdl_pair_prepare(&mut three_way.base, &mut three_way.side1,
					 three_way.minimal_perfect_hash_size, flags, &mut three_way.pair1);
	xdl_pair_prepare(&mut three_way.base, &mut three_way.side2,
					 three_way.minimal_perfect_hash_size, flags, &mut three_way.pair2);
}
