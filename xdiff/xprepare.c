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

#define XDL_KPDIS_RUN 4
#define XDL_MAX_EQLIMIT 1024
#define XDL_SIMSCAN_WINDOW 100


extern void xdl_parse_lines(mmfile_t const* file, struct ivec_xrecord* record);

static void xdl_file_prepare(mmfile_t *mf, struct xdfile *file_storage) {
	IVEC_INIT(file_storage->record);
	xdl_parse_lines(mf, &file_storage->record);

	IVEC_INIT(file_storage->minimal_perfect_hash);
	ivec_reserve_exact(&file_storage->minimal_perfect_hash, file_storage->record.length);
}

static void xdl_file_free(struct xdfile *file_storage) {
	ivec_free(&file_storage->minimal_perfect_hash);
	ivec_free(&file_storage->record);
}

static void xdl_prepare_ctx(struct xdfile *file_storage, struct xd_file_context *ctx) {
	ctx->record = &file_storage->record;
	ctx->minimal_perfect_hash = &file_storage->minimal_perfect_hash;

	IVEC_INIT(ctx->consider);
	ivec_zero(&ctx->consider, SENTINEL + ctx->record->length + SENTINEL);

	IVEC_INIT(ctx->rindex);

	ctx->record->length = ctx->record->length;
}


static void xdl_free_ctx(struct xd_file_context *ctx) {
	ctx->minimal_perfect_hash = NULL;
	ctx->record = NULL;
	ivec_free(&ctx->consider);
	ivec_free(&ctx->rindex);
}


void xdl_free_env(struct xdfile *fs1, struct xdfile *fs2, struct xdpair *pair) {
	xdl_file_free(fs1);
	xdl_file_free(fs2);
	xdl_free_ctx(&pair->rhs);
	xdl_free_ctx(&pair->lhs);
}


extern bool xdl_clean_mmatch(struct ivec_u8* dis, usize i, usize start, usize end);


/*
 * Try to reduce the problem complexity, discard records that have no
 * matches on the other file. Also, lines that have multiple matches
 * might be potentially discarded if they happear in a run of discardable.
 */
extern void xdl_cleanup_records(struct xdpair *pair);


/*
 * Early trim initial and terminal matching records.
 */
extern void xdl_trim_ends(struct xdpair *pair);


static void xdl_optimize_ctxs(struct xdpair *pair) {
	xdl_trim_ends(pair);
	xdl_cleanup_records(pair);
}

void* xdl_mphb_new(usize max_unique_keys, u64 flags);
void xdl_mphb_process(void* mphb, struct xdfile *file);
usize xdl_mphb_finish(void* mphb);

int xdl_prepare_env(mmfile_t *mf1, mmfile_t *mf2, xpparam_t const *xpp,
		    struct xdfile *fs1, struct xdfile *fs2, struct xdpair *pair) {
	void* mphb;

	pair->delta_start = 0;
	pair->delta_end = 0;

	xdl_file_prepare(mf1, fs1);
	xdl_file_prepare(mf2, fs2);

	xdl_prepare_ctx(fs1, &pair->lhs);
	xdl_prepare_ctx(fs2, &pair->rhs);

	mphb = xdl_mphb_new(pair->lhs.record->length + pair->rhs.record->length + 1, xpp->flags);
	xdl_mphb_process(mphb, fs1);
	xdl_mphb_process(mphb, fs2);
	pair->minimal_perfect_hash_size = xdl_mphb_finish(mphb);

	if ((xpp->flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0) {
		xdl_optimize_ctxs(pair);
	}

	return 0;
}
