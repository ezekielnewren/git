/*
 * Copyright (C) 2010, Google Inc.
 * and other copyright owners as documented in JGit's IP log.
 *
 * This program and the accompanying materials are made available
 * under the terms of the Eclipse Distribution License v1.0 which
 * accompanies this distribution, is reproduced below, and is
 * available at http://www.eclipse.org/org/documents/edl-v10.php
 *
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or
 * without modification, are permitted provided that the following
 * conditions are met:
 *
 * - Redistributions of source code must retain the above copyright
 *   notice, this list of conditions and the following disclaimer.
 *
 * - Redistributions in binary form must reproduce the above
 *   copyright notice, this list of conditions and the following
 *   disclaimer in the documentation and/or other materials provided
 *   with the distribution.
 *
 * - Neither the name of the Eclipse Foundation, Inc. nor the
 *   names of its contributors may be used to endorse or promote
 *   products derived from this software without specific prior
 *   written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND
 * CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
 * INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES
 * OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR
 * CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
 * SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT
 * NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
 * LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
 * CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT,
 * STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF
 * ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */

#include "xinclude.h"

#define MAX_CHAIN_LENGTH 64

#define LINE_END(n) (line##n + count##n - 1)
#define ONE_INDEXED 1

struct record {
	size_t ptr, cnt;
	struct record *next;
};

DEFINE_IVEC_TYPE(struct record, record);
DEFINE_IVEC_TYPE(struct record*, record_ptr);

struct histindex {
	struct IVec_record_ptr record;   /* an occurrence */
	struct IVec_record_ptr line_map; /* map of line to record chain */
	struct IVec_record record_storage;
	struct IVec_usize next_ptr;
	size_t table_bits;
	size_t ptr_shift;
	size_t cnt;
	bool has_common;

	xdfenv_t *env;
};

struct region {
	size_t begin1, end1;
	size_t begin2, end2;
};

#define LINE_MAP(i, a) (i->line_map.ptr[(a) - i->ptr_shift])

#define NEXT_PTR(index, line_number) \
	(index->next_ptr.ptr[(line_number) - index->ptr_shift])

#define CNT(index, ptr) \
	((LINE_MAP(index, ptr))->cnt)

#define MPH(env, s, l) \
	(env->xdf##s.minimal_perfect_hash.ptr[l - ONE_INDEXED])

#define CMP(i, s1, l1, s2, l2) \
	(MPH(i->env, s1, l1) == MPH(i->env, s2, l2))

#define TABLE_HASH(index, side, line) \
	XDL_HASHLONG(MPH(index->env, side, line), index->table_bits)

static int scanA(struct histindex *index, size_t line1, size_t count1)
{
	size_t ptr, tbl_idx;
	size_t chain_len;
	struct record **rec_chain, *rec;

	for (ptr = LINE_END(1); line1 <= ptr; ptr--) {
		tbl_idx = TABLE_HASH(index, 1, ptr);
		rec_chain = &index->record.ptr[tbl_idx];
		rec = *rec_chain;

		chain_len = 0;
		while (rec) {
			if (CMP(index, 1, rec->ptr, 1, ptr)) {
				/*
				 * ptr is identical to another element. Insert
				 * it onto the front of the existing element
				 * chain.
				 */
				NEXT_PTR(index, ptr) = rec->ptr;
				rec->ptr = ptr;
				rec->cnt += 1;
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
		if (index->record_storage.capacity == 0)
			ivec_reserve_exact(&index->record_storage, index->env->mph_size);
		rec = &index->record_storage.ptr[index->record_storage.length++];
		rec->ptr = ptr;
		rec->cnt = 1;
		rec->next = *rec_chain;
		*rec_chain = rec;
		LINE_MAP(index, ptr) = rec;

continue_scan:
		; /* no op */
	}

	return 0;
}

static int try_lcs(struct histindex *index, struct region *lcs, size_t b_ptr,
	size_t line1, size_t count1, size_t line2, size_t count2)
{
	size_t b_next = b_ptr + 1;
	struct record *rec = index->record.ptr[TABLE_HASH(index, 2, b_ptr)];
	size_t as, ae, bs, be, np, rc;
	bool should_break;

	for (; rec; rec = rec->next) {
		if (rec->cnt > index->cnt) {
			if (!index->has_common)
				index->has_common = CMP(index, 1, rec->ptr, 2, b_ptr);
			continue;
		}

		as = rec->ptr;
		if (!CMP(index, 1, as, 2, b_ptr))
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
				&& CMP(index, 1, as - 1, 2, bs - 1)) {
				as--;
				bs--;
				if (1 < rc)
					rc = XDL_MIN(rc, CNT(index, as));
			}
			while (ae < LINE_END(1) && be < LINE_END(2)
				&& CMP(index, 1, ae + 1, 2, be + 1)) {
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

static inline void free_index(struct histindex *index)
{
	ivec_free(&index->record);
	ivec_free(&index->line_map);
	ivec_free(&index->record_storage);
	ivec_free(&index->next_ptr);
}

static int histindex_init(struct histindex *index, xdfenv_t *env, size_t line1, size_t count1)
{
	memset(index, 0, sizeof(struct histindex));
	IVEC_INIT(index->record);
	IVEC_INIT(index->line_map);
	IVEC_INIT(index->record_storage);
	IVEC_INIT(index->next_ptr);

	index->env = env;

	index->table_bits = xdl_hashbits(count1);
	ivec_zero(&index->record, 1 << index->table_bits);

	ivec_zero(&index->line_map, count1);
	ivec_zero(&index->next_ptr, count1);

	index->ptr_shift = line1;

	return 0;
}

static int find_lcs(xdfenv_t *env,
		    struct region *lcs,
		    size_t line1, size_t count1, size_t line2, size_t count2)
{
	size_t b_ptr;
	int ret = -1;
	struct histindex index;

	histindex_init(&index, env, line1, count1);

	if (scanA(&index, line1, count1))
		goto cleanup;

	index.cnt = MAX_CHAIN_LENGTH + 1;

	for (b_ptr = line2; b_ptr <= LINE_END(2); )
		b_ptr = try_lcs(&index, lcs, b_ptr, line1, count1, line2, count2);

	if (index.has_common && MAX_CHAIN_LENGTH < index.cnt)
		ret = 1;
	else
		ret = 0;

cleanup:
	free_index(&index);
	return ret;
}

static int histogram_diff(uint64_t flags, xdfenv_t *env,
	size_t line1, size_t count1, size_t line2, size_t count2)
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
			env->xdf2.changed.ptr[line2++ - 1] = true;
		return 0;
	} else if (!count2) {
		while(count1--)
			env->xdf1.changed.ptr[line1++ - 1] = true;
		return 0;
	}

	memset(&lcs, 0, sizeof(lcs));
	lcs_found = find_lcs(env, &lcs, line1, count1, line2, count2);
	if (lcs_found < 0)
		goto out;
	else if (lcs_found)
		result = xdl_fall_back_diff(env, flags, line1, count1, line2, count2);
	else {
		if (lcs.begin1 == 0 && lcs.begin2 == 0) {
			while (count1--)
				env->xdf1.changed.ptr[line1++ - 1] = true;
			while (count2--)
				env->xdf2.changed.ptr[line2++ - 1] = true;
			result = 0;
		} else {
			result = histogram_diff(flags, env,
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

int xdl_do_histogram_diff(xdfenv_t *env, uint64_t flags)
{
	size_t start = ONE_INDEXED + env->delta_start;
	size_t end1 = ONE_INDEXED + env->xdf1.record.length - env->delta_end;
	size_t end2 = ONE_INDEXED + env->xdf2.record.length - env->delta_end;

	return histogram_diff(flags, env,
		start, end1 - start,
		start, end2 - start);
}
