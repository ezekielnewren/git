use interop::ivec::IVec;
use crate::xdiff::*;
use crate::xtypes::{parse_lines, xd2way, xd3way, xd_file_context, xdfile, xdpair, xrecord};
use crate::xutils::{MinimalPerfectHashBuilder};


fn xdl_setup_ctx(file: &xdfile, ctx: &mut xd_file_context) {
	ctx.minimal_perfect_hash = &file.minimal_perfect_hash as *const IVec<u64> as *mut IVec<u64>;
	ctx.record = &file.record as *const IVec<xrecord> as *mut IVec<xrecord>;
	ctx.consider = unsafe { IVec::zero(SENTINEL + file.record.len() + SENTINEL) };
	ctx.rindex = IVec::new();
}


fn xdl_pair_prepare(
	lhs: &mut xdfile, rhs: &mut xdfile,
	mph_size: usize, pair: &mut xdpair
) {
	pair.lhs = xd_file_context::default();
	pair.rhs = xd_file_context::default();
	pair.delta_start = 0;
	pair.delta_end = 0;
	pair.minimal_perfect_hash_size = mph_size;

	xdl_setup_ctx(lhs, &mut pair.lhs);
	xdl_setup_ctx(rhs, &mut pair.rhs);
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
					 two_way.minimal_perfect_hash_size, &mut two_way.pair);
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
					 three_way.minimal_perfect_hash_size, &mut three_way.pair1);
	xdl_pair_prepare(&mut three_way.base, &mut three_way.side2,
					 three_way.minimal_perfect_hash_size, &mut three_way.pair2);
}
