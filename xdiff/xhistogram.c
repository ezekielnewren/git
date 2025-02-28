
#include "xinclude.h"

#define LINE_END(n) (line##n + count##n - 1)
#define LINE_END_PTR(n) (*line##n + *count##n - 1)

#define MAX_CHAIN_LENGTH 64

struct record {
	usize ptr, cnt;
	struct record *next;
};

DEFINE_IVEC_TYPE(struct record, record);
DEFINE_IVEC_TYPE(struct record*, record_ptr);

struct histindex {
	ivec_record record_storage;
	ivec_record_ptr record_chain;
	ivec_record_ptr line_map;
	ivec_usize next_ptrs;
	u32 table_bits;
	usize ptr_shift;
	usize cnt;
	bool has_common;
};

struct region {
	unsigned int begin1, end1;
	unsigned int begin2, end2;
};

#define LINE_MAP(i, a) (i->line_map.ptr[(a) - i->ptr_shift])

#define NEXT_PTR(index, ptr_off) \
	(index->next_ptrs.ptr[(ptr_off) - index->ptr_shift])

#define CNT(index, ptr) \
	((LINE_MAP(index, ptr))->cnt)

#define MPH(env, s, l) \
	(env->xdf##s.minimal_perfect_hash.ptr[l - 1])

#define CMP(env, s1, l1, s2, l2) \
	(MPH(env, s1, l1) == MPH(env, s2, l2))

#define TABLE_HASH(index, env, side, line) \
	XDL_HASHLONG((MPH(env, side, line)), index->table_bits)

static int scanA(struct histindex *index, xdfenv_t *env, int line1, int count1)
{
	unsigned int ptr, tbl_idx;
	unsigned int chain_len;
	struct record **rec_chain, *rec;
	struct record new_rec;

	for (ptr = LINE_END(1); line1 <= ptr; ptr--) {
		tbl_idx = TABLE_HASH(index, env, 1, ptr);
		rec_chain = &index->record_chain.ptr[tbl_idx];
		rec = *rec_chain;

		chain_len = 0;
		while (rec) {
			if (CMP(env, 1, rec->ptr, 1, ptr)) {
				/*
				 * ptr is identical to another element. Insert
				 * it onto the front of the existing element
				 * chain.
				 */
				NEXT_PTR(index, ptr) = rec->ptr;
				rec->ptr = ptr;
				/* cap rec->cnt at MAX_CNT */
				rec->cnt = rec->cnt + 1;
				LINE_MAP(index, ptr) = rec;
				goto continue_scan;
			}

			rec = rec->next;
			chain_len++;
		}

		if (chain_len == MAX_CHAIN_LENGTH)
			return -1;

		/*
		 * This is the first time we have ever seen this particular
		 * element in the sequence. Construct a new chain for it.
		 */
		new_rec.ptr = ptr;
		new_rec.cnt = 1;
		new_rec.next = *rec_chain;
		rust_ivec_push(&index->record_storage, &new_rec);
		rec = &index->record_storage.ptr[index->record_storage.length - 1];
		*rec_chain = rec;
		LINE_MAP(index, ptr) = rec;

continue_scan:
		; /* no op */
	}

	return 0;
}

static int try_lcs(struct histindex *index, xdfenv_t *env, struct region *lcs, int b_ptr,
	int line1, int count1, int line2, int count2)
{
	unsigned int b_next = b_ptr + 1;
	struct record *rec = index->record_chain.ptr[TABLE_HASH(index, env, 2, b_ptr)];
	unsigned int as, ae, bs, be, np, rc;
	int should_break;

	for (; rec; rec = rec->next) {
		if (rec->cnt > index->cnt) {
			if (!index->has_common)
				index->has_common = CMP(env, 1, rec->ptr, 2, b_ptr);
			continue;
		}

		as = rec->ptr;
		if (!CMP(env, 1, as, 2, b_ptr))
			continue;

		index->has_common = 1;
		for (;;) {
			should_break = 0;
			np = NEXT_PTR(index, as);
			bs = b_ptr;
			ae = as;
			be = bs;
			rc = rec->cnt;

			while (line1 < as && line2 < bs
				&& CMP(env, 1, as - 1, 2, bs - 1)) {
				as--;
				bs--;
				if (1 < rc)
					rc = XDL_MIN(rc, CNT(index, as));
			}
			while (ae < LINE_END(1) && be < LINE_END(2)
				&& CMP(env, 1, ae + 1, 2, be + 1)) {
				ae++;
				be++;
				if (1 < rc)
					rc = XDL_MIN(rc, CNT(index, ae));
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

			if (np == 0)
				break;

			while (np <= ae) {
				np = NEXT_PTR(index, np);
				if (np == 0) {
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

static int fall_back_to_classic_diff(xpparam_t const *xpp, xdfenv_t *env,
		int line1, int count1, int line2, int count2)
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

static int find_lcs(xdfenv_t *env,
		    struct region *lcs,
		    int line1, int count1, int line2, int count2)
{
	int b_ptr;
	int ret = -1;
	struct histindex index;
	struct record default_rec_value;
	struct record* default_rec_ptr_value = NULL;
	usize line_map_size = env->xdf1.record.length + env->xdf2.record.length;
	usize default_ptr = 0;

	default_rec_value.ptr = 0;
	default_rec_value.cnt = 0;
	default_rec_value.next = NULL;

	memset(&index, 0, sizeof(index));

	IVEC_INIT(index.record_storage);
	IVEC_INIT(index.record_chain);
	IVEC_INIT(index.line_map);
	IVEC_INIT(index.next_ptrs);


	index.table_bits = xdl_hashbits(count1);

	rust_ivec_resize_exact(&index.record_storage, env->xdf1.record.length*10, &default_rec_value);
	rust_ivec_resize_exact(&index.record_chain, env->xdf1.record.length*10, &default_rec_ptr_value);

	rust_ivec_resize_exact(&index.line_map, line_map_size*10, &default_rec_ptr_value);
	rust_ivec_resize_exact(&index.next_ptrs, line_map_size*10, &default_ptr);

	index.ptr_shift = line1;

	if (scanA(&index, env, line1, count1))
		goto cleanup;

	index.cnt = MAX_CHAIN_LENGTH + 1;

	for (b_ptr = line2; b_ptr <= LINE_END(2); )
		b_ptr = try_lcs(&index, env, lcs, b_ptr, line1, count1, line2, count2);

	if (index.has_common && MAX_CHAIN_LENGTH < index.cnt)
		ret = 1;
	else
		ret = 0;

cleanup:
	free_index(&index);
	return ret;
}

static int histogram_diff(xpparam_t const *xpp, xdfenv_t *env,
	int line1, int count1, int line2, int count2)
{
	struct region lcs;
	int lcs_found;
	int result;
redo:
	result = -1;

	if (count1 <= 0 && count2 <= 0)
		return 0;

	if (!count1) {
		while(count2--)
			env->xdf2.rchg[line2++ - 1] = 1;
		return 0;
	} else if (!count2) {
		while(count1--)
			env->xdf1.rchg[line1++ - 1] = 1;
		return 0;
	}

	memset(&lcs, 0, sizeof(lcs));
	lcs_found = find_lcs(env, &lcs, line1, count1, line2, count2);
	if (lcs_found < 0)
		goto out;
	else if (lcs_found)
		result = fall_back_to_classic_diff(xpp, env, line1, count1, line2, count2);
	else {
		if (lcs.begin1 == 0 && lcs.begin2 == 0) {
			while (count1--)
				env->xdf1.rchg[line1++ - 1] = 1;
			while (count2--)
				env->xdf2.rchg[line2++ - 1] = 1;
			result = 0;
		} else {
			result = histogram_diff(xpp, env,
						line1, lcs.begin1 - line1,
						line2, lcs.begin2 - line2);
			if (result)
				goto out;
			/*
			 * result = histogram_diff(xpp, env,
			 *            lcs.end1 + 1, LINE_END(1) - lcs.end1,
			 *            lcs.end2 + 1, LINE_END(2) - lcs.end2);
			 * but let's optimize tail recursion ourself:
			*/
			count1 = LINE_END(1) - lcs.end1;
			line1 = lcs.end1 + 1;
			count2 = LINE_END(2) - lcs.end2;
			line2 = lcs.end2 + 1;
			goto redo;
		}
	}
out:
	return result;
}

int xdl_do_histogram_diff(xpparam_t const *xpp, xdfenv_t *env) {
	isize end1 = env->xdf1.record.length - 1;
	isize end2 = env->xdf2.record.length - 1;

	return histogram_diff(xpp, env,
		env->delta_start + 1, end1 - env->delta_start + 1,
		env->delta_start + 1, end2 - env->delta_start + 1);
}
