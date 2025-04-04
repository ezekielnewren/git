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

#define LINE_END(n) (line##n + count##n - 1)

struct record {
	usize ptr, cnt;
	struct record *next;
};

DEFINE_IVEC_TYPE(struct record, record);
DEFINE_IVEC_TYPE(struct record*, record_ptr);

struct histindex {
	struct ivec_record record_storage;
	struct ivec_record_ptr record;
	struct ivec_record_ptr line_map; /* map of line to record chain */
	struct ivec_usize next_ptrs;
	usize max_chain_length;
	usize ptr_shift;
	usize cnt;
	bool has_common;
};

struct region {
	usize begin1, end1;
	usize begin2, end2;
};

#define LINE_MAP(i, a) (i->line_map.ptr[(a) - i->ptr_shift])

#define NEXT_PTR(index, _ptr) \
	(index->next_ptrs.ptr[(_ptr) - index->ptr_shift])

#define MPH(pair, s, l) \
	(pair->s.minimal_perfect_hash->ptr[l - LINE_SHIFT])

#define CMP(s1, l1, s2, l2) \
	(MPH(pair, s1, l1) == MPH(pair, s2, l2))

extern i32 scanA(struct histindex *index, struct xdpair *pair, usize line1, usize count1);

static i32 try_lcs(struct histindex *index, struct xdpair *pair, struct region *lcs, usize b_ptr,
	usize line1, usize count1, usize line2, usize count2)
{
	usize b_next = b_ptr + 1;
	usize tbl_idx = MPH(pair, rhs, b_ptr);
	struct record *rec = index->record.ptr[tbl_idx];
	struct xrange range_a = {.start = 0, .end = 0};
	struct xrange range_b = {.start = 0, .end = 0};
	usize np = 0;
	usize rc = 0;
	bool should_break = false;

	for (; rec; rec = rec->next) {
		if (rec->cnt > index->cnt) {
			if (!index->has_common) {
				index->has_common = CMP(lhs, rec->ptr, rhs, b_ptr);
			}
			continue;
		}

		range_a.start = rec->ptr;
		if (!CMP(lhs, range_a.start, rhs, b_ptr)) {
			continue;
		}

		index->has_common = true;
		for (;;) {
			should_break = false;
			np = NEXT_PTR(index, range_a.start);
			range_b.start = b_ptr;
			range_a.end = range_a.start;
			range_b.end = range_b.start;
			rc = rec->cnt;

			while (line1 < range_a.start && line2 < range_b.start
				&& CMP(lhs, range_a.start - 1, rhs, range_b.start - 1)) {
				range_a.start--;
				range_b.start--;
				if (1 < rc) {
					usize cnt = LINE_MAP(index, range_a.start)->cnt;
					rc = XDL_MIN(rc, cnt);
				}
			}
			while (range_a.end < LINE_END(1) && range_b.end < LINE_END(2)
				&& CMP(lhs, range_a.end + 1, rhs, range_b.end + 1)) {
				range_a.end++;
				range_b.end++;
				if (1 < rc) {
					usize cnt = LINE_MAP(index, range_a.end)->cnt;
					rc = XDL_MIN(rc, cnt);
				}
			}

			if (b_next <= range_b.end) {
				b_next = range_b.end + 1;
			}
			if (lcs->end1 - lcs->begin1 < range_a.end - range_a.start || rc < index->cnt) {
				lcs->begin1 = range_a.start;
				lcs->begin2 = range_b.start;
				lcs->end1 = range_a.end;
				lcs->end2 = range_b.end;
				index->cnt = rc;
			}

			if (np == 0) {
				break;
			}

			while (np <= range_a.end) {
				np = NEXT_PTR(index, np);
				if (np == 0) {
					should_break = 1;
					break;
				}
			}

			if (should_break) {
				break;
			}

			range_a.start = np;
		}
	}
	return b_next;
}

static int fall_back_to_classic_diff(xpparam_t const *xpp, struct xdpair *pair,
		int line1, int count1, int line2, int count2)
{
	xpparam_t xpparam;

	memset(&xpparam, 0, sizeof(xpparam));
	xpparam.flags = xpp->flags & ~XDF_DIFF_ALGORITHM_MASK;

	return xdl_fall_back_diff(pair, &xpparam,
				  line1, count1, line2, count2);
}

static inline void free_index(struct histindex *index)
{
	ivec_free(&index->record_storage);
	ivec_free(&index->record);
	ivec_free(&index->line_map);
	ivec_free(&index->next_ptrs);
}

static int find_lcs(xpparam_t const *xpp, struct xdpair *pair,
		    struct region *lcs,
		    int line1, int count1, int line2, int count2)
{
	int b_ptr;
	int ret = -1;
	struct histindex index;
	usize fudge = (pair->lhs.record->length + pair->rhs.record->length) * 10;

	memset(&index, 0, sizeof(index));

	IVEC_INIT(index.record);
	ivec_zero(&index.record, pair->minimal_perfect_hash_size);

	IVEC_INIT(index.line_map);
	ivec_zero(&index.line_map, count1);

	IVEC_INIT(index.next_ptrs);
	ivec_zero(&index.next_ptrs, count1);

	IVEC_INIT(index.record_storage);
	ivec_reserve_exact(&index.record_storage, fudge);

	index.ptr_shift = line1;
	index.max_chain_length = 64;

	if (scanA(&index, pair, line1, count1))
		goto cleanup;

	index.cnt = index.max_chain_length + 1;

	for (b_ptr = line2; b_ptr <= LINE_END(2); )
		b_ptr = try_lcs(&index, pair, lcs, b_ptr, line1, count1, line2, count2);

	if (index.has_common && index.max_chain_length < index.cnt)
		ret = 1;
	else
		ret = 0;

cleanup:
	free_index(&index);
	return ret;
}

static int histogram_diff(xpparam_t const *xpp, struct xdpair *pair,
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
			pair->rhs.consider.ptr[SENTINEL + line2++ - 1] = YES;
		return 0;
	} else if (!count2) {
		while(count1--)
			pair->lhs.consider.ptr[SENTINEL + line1++ - 1] = YES;
		return 0;
	}

	memset(&lcs, 0, sizeof(lcs));
	lcs_found = find_lcs(xpp, pair, &lcs, line1, count1, line2, count2);
	if (lcs_found < 0)
		goto out;
	else if (lcs_found)
		result = fall_back_to_classic_diff(xpp, pair, line1, count1, line2, count2);
	else {
		if (lcs.begin1 == 0 && lcs.begin2 == 0) {
			while (count1--)
				pair->lhs.consider.ptr[SENTINEL + line1++ - 1] = YES;
			while (count2--)
				pair->rhs.consider.ptr[SENTINEL + line2++ - 1] = YES;
			result = 0;
		} else {
			result = histogram_diff(xpp, pair,
						line1, lcs.begin1 - line1,
						line2, lcs.begin2 - line2);
			if (result)
				goto out;
			/*
			 * result = histogram_diff(xpp, pair,
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

int xdl_do_histogram_diff(xpparam_t const *xpp, struct xdpair *pair) {
	int result = -1;
	usize end1 = pair->lhs.record->length - pair->delta_end;
	usize end2 = pair->rhs.record->length - pair->delta_end;

	result = histogram_diff(xpp, pair,
		LINE_SHIFT + pair->delta_start, LINE_SHIFT + (end1 - 1) - pair->delta_start,
		LINE_SHIFT + pair->delta_start, LINE_SHIFT + (end2 - 1) - pair->delta_start);

	return result;
}
