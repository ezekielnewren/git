
#include "xinclude.h"

#define MAX_CHAIN_LENGTH 64

struct record {
	usize ptr, cnt;
	usize next;
};

DEFINE_IVEC_TYPE(struct record, record);

struct histindex {
	ivec_record record_storage;
	ivec_usize record_chain;
	ivec_usize line_map;
	ivec_usize next_ptrs;
	usize ptr_shift;
	usize cnt;
	bool has_common;
};

struct region {
	usize begin1, end1;
	usize begin2, end2;
};

static bool mph_equal_by_line_number(xdfenv_t *env, usize lhs, usize rhs) {
	u64 mph1 = env->xdf1.minimal_perfect_hash.ptr[lhs - LINE_SHIFT];
	u64 mph2 = env->xdf2.minimal_perfect_hash.ptr[rhs - LINE_SHIFT];
	return mph1 == mph2;
}

static i32 scanA(struct histindex *index, xdfenv_t *env, usize start1, usize end1) {
	usize tbl_idx;
	usize chain_len;
	usize rec_cur_idx, rec_new_idx;
	struct record *rec_cur;
	struct record rec_new;

	for (usize i = end1; i > start1; i -= 1) {
		bool continue_scan = false;
		usize ptr = i - 1;
		tbl_idx = env->xdf1.minimal_perfect_hash.ptr[ptr - LINE_SHIFT];
		rec_cur_idx = index->record_chain.ptr[tbl_idx];

		chain_len = 0;
		while (rec_cur_idx != INVALID_INDEX) {
			u64 mph1, mph2;
			rec_cur = &index->record_storage.ptr[rec_cur_idx];
			mph1 = env->xdf1.minimal_perfect_hash.ptr[rec_cur->ptr - LINE_SHIFT];
			mph2 = env->xdf1.minimal_perfect_hash.ptr[ptr - LINE_SHIFT];
			if (mph1 == mph2) {
				/*
				 * ptr is identical to another element. Insert
				 * it onto the front of the existing element
				 * chain.
				 */
				index->next_ptrs.ptr[ptr - index->ptr_shift] = rec_cur->ptr;
				rec_cur->ptr = ptr;
				rec_cur->cnt = rec_cur->cnt + 1;
				index->line_map.ptr[ptr - index->ptr_shift] = rec_cur_idx;
				continue_scan = true;
				break;
			}

			rec_cur_idx = rec_cur->next;
			chain_len++;
		}

		if (continue_scan)
			continue;

		if (chain_len == MAX_CHAIN_LENGTH)
			return -1;

		/*
		 * This is the first time we have ever seen this particular
		 * element in the sequence. Construct a new chain for it.
		 */
		rec_new_idx = index->record_storage.length;
		rec_new.ptr = ptr;
		rec_new.cnt = 1;
		rec_new.next = index->record_chain.ptr[tbl_idx];
		rust_ivec_push(&index->record_storage, &rec_new);
		index->record_chain.ptr[tbl_idx] = rec_new_idx;
		index->line_map.ptr[ptr - index->ptr_shift] = rec_new_idx;
	}

	return 0;
}

static usize try_lcs(struct histindex *index, xdfenv_t *env, struct region *lcs, usize b_ptr,
	usize start1, usize end1, usize start2, usize end2)
{
	struct record *rec_cur;
	usize b_next = b_ptr + 1;
	usize tbl_idx = env->xdf2.minimal_perfect_hash.ptr[b_ptr - LINE_SHIFT];
	usize as, ae, bs, be, np, rc;
	bool should_break;

	for (usize rec_cur_idx = index->record_chain.ptr[tbl_idx];
		rec_cur_idx != INVALID_INDEX; rec_cur_idx = rec_cur->next) {
		rec_cur = &index->record_storage.ptr[rec_cur_idx];
		if (rec_cur->cnt > index->cnt) {
			if (!index->has_common)
				index->has_common = mph_equal_by_line_number(env, rec_cur->ptr, b_ptr);
			continue;
		}

		as = rec_cur->ptr;
		if (!mph_equal_by_line_number(env, as, b_ptr))
			continue;

		index->has_common = true;
		for (;;) {
			should_break = false;
			np = index->next_ptrs.ptr[as - index->ptr_shift];
			bs = b_ptr;
			ae = as;
			be = bs;
			rc = rec_cur->cnt;

			while (start1 < as && start2 < bs
				&& mph_equal_by_line_number(env, as - 1, bs - 1)) {
				as--;
				bs--;
				if (1 < rc) {
					usize rec_t_idx = index->line_map.ptr[as - index->ptr_shift];
					struct record *rec_t = &index->record_storage.ptr[rec_t_idx];
					usize cnt = rec_t->cnt;
					rc = XDL_MIN(rc, cnt);
				}
			}
			while (ae + 1 < end1 && be + 1 < end2
				&& mph_equal_by_line_number(env, ae + 1, be + 1)) {
				ae++;
				be++;
				if (1 < rc) {
					usize rec_t_idx = index->line_map.ptr[ae - index->ptr_shift];
					struct record *rec_t = &index->record_storage.ptr[rec_t_idx];
					usize cnt = rec_t->cnt;
					rc = XDL_MIN(rc, cnt);
				}
			}

			if (b_next <= be)
				b_next = be + 1;
			if (lcs->end1 - lcs->begin1 < ae - as || rc < index->cnt) {
				lcs->begin1 = as;
				lcs->begin2 = bs;
				lcs->end1 = ae;
				lcs->end2 = be;
				index->cnt = rc;
			}

			if (np == 0 || np == INVALID_INDEX)
				break;

			while (np <= ae) {
				np = index->next_ptrs.ptr[np - index->ptr_shift];
				if (np == 0 || np == INVALID_INDEX) {
					should_break = 1;
					break;
				}
			}

			if (should_break)
				break;

			as = np;
		}
	}
	return b_next;
}

static i32 fall_back_to_classic_diff(xpparam_t const *xpp, xdfenv_t *env,
		usize line1, usize count1, usize line2, usize count2)
{
	xpparam_t xpparam;

	memset(&xpparam, 0, sizeof(xpparam));
	xpparam.flags = xpp->flags & ~XDF_DIFF_ALGORITHM_MASK;

	return xdl_fall_back_diff(env, &xpparam,
				  line1, count1, line2, count2);
}

static inline void free_index(struct histindex *index) {
	rust_ivec_free(&index->record_storage);
	rust_ivec_free(&index->record_chain);
	rust_ivec_free(&index->line_map);
	rust_ivec_free(&index->next_ptrs);
}

static i32 find_lcs(xdfenv_t *env,
		    struct region *lcs,
		    usize start1, usize end1, usize start2, usize end2)
{
	i32 ret = -1;
	struct histindex index;
	usize table_size = env->xdf1.record.length;
	usize default_value = INVALID_INDEX;
	memset(&index, 0, sizeof(index));

	IVEC_INIT(index.record_storage);
	IVEC_INIT(index.record_chain);
	IVEC_INIT(index.line_map);
	IVEC_INIT(index.next_ptrs);

	rust_ivec_resize_exact(&index.record_chain, env->minimal_perfect_hash_size, &default_value);
	rust_ivec_resize_exact(&index.line_map, table_size, &default_value);
	rust_ivec_resize_exact(&index.next_ptrs, table_size, &default_value);

	index.ptr_shift = start1;

	if (scanA(&index, env, start1, end1)) {
		free_index(&index);
		return ret;
	}

	index.cnt = MAX_CHAIN_LENGTH + 1;

	for (usize b_ptr = start2; b_ptr + 1 <= end2; )
		b_ptr = try_lcs(&index, env, lcs, b_ptr, start1, end1, start2, end2);

	if (index.has_common && MAX_CHAIN_LENGTH < index.cnt)
		ret = 1;
	else
		ret = 0;

	free_index(&index);
	return ret;
}

static int histogram_diff(xpparam_t const *xpp, xdfenv_t *env,
	usize start1, usize end1, usize start2, usize end2)
{
	struct region lcs;
	i32 lcs_found;
	i32 result;

	while (true) {
		result = -1;

		if (start1 >= end1 && start2 >= end2)
			return 0;

		if (start1 == end1) {
			for (; start2 < end2; start2 += 1) {
				env->xdf2.rchg[start2 - 1] = YES;
			}
			return 0;
		}
		if (start2 == end2) {
			for (; start1 < end1; start1 += 1) {
				env->xdf1.rchg[start1 - 1] = YES;
			}
			return 0;
		}

		memset(&lcs, 0, sizeof(lcs));
		lcs_found = find_lcs(env, &lcs, start1, end1, start2, end2);
		if (lcs_found < 0)
			return result;
		else if (lcs_found)
			result = fall_back_to_classic_diff(xpp, env, start1, end1 - start1, start2, end2 - start1);
		else {
			if (lcs.begin1 == 0 && lcs.begin2 == 0) {
				for (; start1 < end1; start1 += 1) {
					env->xdf1.rchg[start1 - 1] = YES;
				}
				for (; start2 < end2; start2 += 1) {
					env->xdf2.rchg[start2 - 1] = YES;
				}
				result = 0;
			} else {
				result = histogram_diff(xpp, env,
							start1, lcs.begin1,
							start2, lcs.begin2);
				if (result)
					return result;
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
		break;
	}
	return result;
}


int xdl_do_histogram_diff(xpparam_t const *xpp, xdfenv_t *env) {
	isize end1 = env->xdf1.record.length;
	isize end2 = env->xdf2.record.length;

	return histogram_diff(xpp, env,
		env->delta_start + 1, end1 + LINE_SHIFT,
		env->delta_start + 1, end2 + LINE_SHIFT);
}
