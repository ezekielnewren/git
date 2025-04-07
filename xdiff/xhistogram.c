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

struct region {
	usize begin1, end1;
	usize begin2, end2;
};

static i32 fall_back_to_classic_diff(u64 flags, struct xdpair *pair,
		usize line1, usize count1, usize line2, usize count2)
{
	xpparam_t xpparam;

	memset(&xpparam, 0, sizeof(xpparam));
	xpparam.flags = flags & ~XDF_DIFF_ALGORITHM_MASK;

	return xdl_fall_back_diff(pair, &xpparam,
				  line1, count1, line2, count2);
}

extern i32 xdl_find_lcs(struct xdpair *pair, struct region *lcs,
		    usize line1, usize count1, usize line2, usize count2);

static i32 histogram_diff(u64 flags, struct xdpair *pair,
	usize line1, usize count1, usize line2, usize count2)
{
	struct region lcs;
	i32 lcs_found;
	i32 result;
	while (1) {
		result = -1;

		if (count1 <= 0 && count2 <= 0) {
			return 0;
		}

		if (count1 == 0) {
			while (count2 > 0) {
				count2 -= 1;
				pair->rhs.consider.ptr[SENTINEL + line2 - LINE_SHIFT] = YES;
				line2 += 1;
			}
			return 0;
		}
		if (count2 == 0) {
			while (count1 > 0) {
				count1 -= 1;
				pair->lhs.consider.ptr[SENTINEL + line1 - LINE_SHIFT] = YES;
				line1 += 1;
			}
			return 0;
		}

		lcs.begin1 = 0;
		lcs.end1 = 0;
		lcs.begin2 = 0;
		lcs.end2 = 0;
		lcs_found = xdl_find_lcs(pair, &lcs, line1, count1, line2, count2);
		if (lcs_found < 0) {
			return result;
		}

		if (lcs_found != 0) {
			return fall_back_to_classic_diff(flags, pair, line1, count1, line2, count2);
		}

		if (lcs.begin1 == 0 && lcs.begin2 == 0) {
			while (count1 > 0) {
				count1 -= 1;
				pair->lhs.consider.ptr[SENTINEL + line1 - 1] = YES;
				line1 += 1;
			}
			while (count2 > 0) {
				count2 -= 1;
				pair->rhs.consider.ptr[SENTINEL + line2 - 1] = YES;
				line2 += 1;
			}
			result = 0;
		} else {
			result = histogram_diff(flags, pair,
						line1, lcs.begin1 - line1,
						line2, lcs.begin2 - line2);
			if (result != 0) {
				return result;
			}
			/*
			 * result = histogram_diff(flags, pair,
			 *            lcs.end1 + 1, LINE_END(1) - lcs.end1,
			 *            lcs.end2 + 1, LINE_END(2) - lcs.end2);
			 * but let's optimize tail recursion ourself:
			*/
			count1 = line1 + count1 - 1 - lcs.end1;
			line1 = lcs.end1 + 1;
			count2 = line2 + count2 - 1 - lcs.end2;
			line2 = lcs.end2 + 1;
			continue;
		}
		break;
	}

	return result;
}

int xdl_do_histogram_diff(u64 flags, struct xdpair *pair) {
	int result = -1;
	usize end1 = pair->lhs.record->length - pair->delta_end;
	usize end2 = pair->rhs.record->length - pair->delta_end;

	result = histogram_diff(flags, pair,
		LINE_SHIFT + pair->delta_start, LINE_SHIFT + (end1 - 1) - pair->delta_start,
		LINE_SHIFT + pair->delta_start, LINE_SHIFT + (end2 - 1) - pair->delta_start);

	return result;
}
