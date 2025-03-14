/*
 *  LibXDiff by Davide Libenzi ( File Differential Library )
 *  Copyright (C) 2003  Davide Libenzi
 *
 *  This library is free software; you can redistribute it and/or
 *  modify it under the terms of the GNU Lesser General Public
 *  License as published by the Free Software Foundation; either
 *  version 2.1 of the License, or (at your option) any later version.
 *
 *  This library is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 *  Lesser General Public License for more details.
 *
 *  You should have received a copy of the GNU Lesser General Public
 *  License along with this library; if not, see
 *  <http://www.gnu.org/licenses/>.
 *
 *  Davide Libenzi <davidel@xmailserver.org>
 *
 */

#include "xinclude.h"
#include "ivec.h"

#define XDL_MAX_EQLIMIT 1024

extern void xdl_file_prepare(mmfile_t *mf, u64 flags, struct xdfile *file);

static void xdl_file_free(struct  xdfile *file) {
	ivec_free(&file->minimal_perfect_hash);
	ivec_free(&file->record);
}

extern void xdl_optimize_ctxs(struct xdpair *pair);

extern void xdl_setup_ctx(struct xdfile *file, struct xd_file_context *ctx);
extern void xdl_pair_prepare(struct xdfile *lhs, struct xdfile *rhs, usize mph_size, u64 flags, struct xdpair *pair);

static void xdl_free_file_context(struct xd_file_context *ctx) {
	ctx->minimal_perfect_hash = NULL;
	ctx->record = NULL;
	ivec_free(&ctx->consider);
	ivec_free(&ctx->rindex);
}

static void xdl_free_pair(struct xdpair *pair) {
	xdl_free_file_context(&pair->lhs);
	xdl_free_file_context(&pair->rhs);
}

void xdl_2way_prepare(mmfile_t *mf1, mmfile_t *mf2, u64 flags, struct xd2way *two_way) {
	struct xdl_minimal_perfect_hash_builder mphb;
	usize max_unique_keys = 0;

	xd_trace2_region_enter("xdiff", "xdl_2way_prepare");

	xdl_file_prepare(mf1, flags, &two_way->lhs);
	xdl_file_prepare(mf2, flags, &two_way->rhs);

	max_unique_keys += two_way->lhs.record.length;
	max_unique_keys += two_way->rhs.record.length;
	xdl_mphb_init(&mphb, max_unique_keys, flags);

	xdl_mphb_ingest(&mphb, &two_way->lhs);
	xdl_mphb_ingest(&mphb, &two_way->rhs);
	two_way->minimal_perfect_hash_size = xdl_mphb_finish(&mphb);

	xdl_pair_prepare(&two_way->lhs, &two_way->rhs,
		two_way->minimal_perfect_hash_size, flags, &two_way->pair);

	xd_trace2_region_leave("xdiff", "xdl_2way_prepare");
}

void xdl_2way_free(struct xd2way *two_way) {
	xdl_file_free(&two_way->lhs);
	xdl_file_free(&two_way->rhs);
	xdl_free_pair(&two_way->pair);
}

void xdl_3way_prepare(mmfile_t *orig, mmfile_t *mf1, mmfile_t *mf2,
	u64 flags, struct xd3way *three_way) {
	struct xdl_minimal_perfect_hash_builder mphb;
	usize max_unique_keys = 0;

	xd_trace2_region_enter("xdiff", "xdl_3way_prepare");

	xdl_file_prepare(orig, flags, &three_way->base);
	xdl_file_prepare(mf1,  flags, &three_way->side1);
	xdl_file_prepare(mf2,  flags, &three_way->side2);

	max_unique_keys += three_way->base.record.length;
	max_unique_keys += three_way->side1.record.length;
	max_unique_keys += three_way->side2.record.length;
	xdl_mphb_init(&mphb, max_unique_keys, flags);

	xdl_mphb_ingest(&mphb, &three_way->base);
	xdl_mphb_ingest(&mphb, &three_way->side1);
	xdl_mphb_ingest(&mphb, &three_way->side2);
	three_way->minimal_perfect_hash_size = xdl_mphb_finish(&mphb);

	xdl_pair_prepare(&three_way->base, &three_way->side1,
		three_way->minimal_perfect_hash_size, flags, &three_way->pair1);
	xdl_pair_prepare(&three_way->base, &three_way->side2,
		three_way->minimal_perfect_hash_size, flags, &three_way->pair2);

	xd_trace2_region_leave("xdiff", "xdl_3way_prepare");
}

void xdl_3way_free(struct xd3way *three_way) {
	xdl_file_free(&three_way->base);
	xdl_file_free(&three_way->side1);
	xdl_file_free(&three_way->side2);
	xdl_free_pair(&three_way->pair1);
	xdl_free_pair(&three_way->pair2);
}
