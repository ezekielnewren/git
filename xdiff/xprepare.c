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




static void xdl_prepare_ctx(mmfile_t *mf, xpparam_t const *xpp,
			   struct xd_file_context *ctx) {
	struct xlinereader reader;

	IVEC_INIT(ctx->file_storage.record);
	xdl_linereader_init(&reader, (u8 const *) mf->ptr, mf->size);
	while (true) {
		struct xrecord rec_new;
		if (!xdl_linereader_next(&reader, &rec_new.ptr, &rec_new.size_no_eol, &rec_new.size_with_eol))
			break;
		rec_new.ha = xdl_line_hash(rec_new.ptr, rec_new.size_no_eol, xpp->flags);
		ivec_push(&ctx->file_storage.record, &rec_new);
	}
	ivec_shrink_to_fit(&ctx->file_storage.record);
	ctx->record = &ctx->file_storage.record;

	IVEC_INIT(ctx->file_storage.minimal_perfect_hash);
	ctx->minimal_perfect_hash = &ctx->file_storage.minimal_perfect_hash;
	ivec_reserve_exact(&ctx->file_storage.minimal_perfect_hash, ctx->file_storage.record.length);

	IVEC_INIT(ctx->consider);
	ivec_zero(&ctx->consider, SENTINEL + ctx->record->length + SENTINEL);

	IVEC_INIT(ctx->rindex);

	ctx->record->length = ctx->record->length;
	ctx->dstart = 0;
	ctx->dend = ctx->record->length - 1;
}


static void xdl_free_ctx(struct xd_file_context *ctx) {
	ivec_free(&ctx->file_storage.minimal_perfect_hash);
	ivec_free(&ctx->file_storage.record);
	ivec_free(&ctx->consider);
	ivec_free(&ctx->rindex);
}


void xdl_free_env(struct xdpair *pair) {

	xdl_free_ctx(&pair->rhs);
	xdl_free_ctx(&pair->lhs);
}


static int xdl_clean_mmatch(char const *dis, long i, long s, long e) {
	long r, rdis0, rpdis0, rdis1, rpdis1;

	/*
	 * Limits the window the is examined during the similar-lines
	 * scan. The loops below stops when dis[i - r] == 1 (line that
	 * has no match), but there are corner cases where the loop
	 * proceed all the way to the extremities by causing huge
	 * performance penalties in case of big files.
	 */
	if (i - s > XDL_SIMSCAN_WINDOW)
		s = i - XDL_SIMSCAN_WINDOW;
	if (e - i > XDL_SIMSCAN_WINDOW)
		e = i + XDL_SIMSCAN_WINDOW;

	/*
	 * Scans the lines before 'i' to find a run of lines that either
	 * have no match (dis[j] == 0) or have multiple matches (dis[j] > 1).
	 * Note that we always call this function with dis[i] > 1, so the
	 * current line (i) is already a multimatch line.
	 */
	for (r = 1, rdis0 = 0, rpdis0 = 1; (i - r) >= s; r++) {
		if (!dis[i - r])
			rdis0++;
		else if (dis[i - r] == 2)
			rpdis0++;
		else
			break;
	}
	/*
	 * If the run before the line 'i' found only multimatch lines, we
	 * return 0 and hence we don't make the current line (i) discarded.
	 * We want to discard multimatch lines only when they appear in the
	 * middle of runs with nomatch lines (dis[j] == 0).
	 */
	if (rdis0 == 0)
		return 0;
	for (r = 1, rdis1 = 0, rpdis1 = 1; (i + r) <= e; r++) {
		if (!dis[i + r])
			rdis1++;
		else if (dis[i + r] == 2)
			rpdis1++;
		else
			break;
	}
	/*
	 * If the run after the line 'i' found only multimatch lines, we
	 * return 0 and hence we don't make the current line (i) discarded.
	 */
	if (rdis1 == 0)
		return 0;
	rdis1 += rdis0;
	rpdis1 += rpdis0;

	return rpdis1 * XDL_KPDIS_RUN < (rpdis1 + rdis1);
}


/*
 * Try to reduce the problem complexity, discard records that have no
 * matches on the other file. Also, lines that have multiple matches
 * might be potentially discarded if they happear in a run of discardable.
 */
static void xdl_cleanup_records(struct xdpair *pair) {
	long i, nm, mlim;
	struct ivec_u8 dis1, dis2;
	struct ivec_xoccurrence occurrence;

	IVEC_INIT(dis1);
	IVEC_INIT(dis2);
	IVEC_INIT(occurrence);

	ivec_zero(&dis1, pair->lhs.consider.length);
	ivec_zero(&dis2, pair->rhs.consider.length);
	ivec_zero(&occurrence, pair->minimal_perfect_hash_size);

	for (usize i = 0; i < pair->lhs.minimal_perfect_hash->length; i++) {
		u64 mph = pair->lhs.minimal_perfect_hash->ptr[i];
		occurrence.ptr[mph].file1 += 1;
	}

	for (usize i = 0; i < pair->rhs.minimal_perfect_hash->length; i++) {
		u64 mph = pair->rhs.minimal_perfect_hash->ptr[i];
		occurrence.ptr[mph].file2 += 1;
	}


	if ((mlim = xdl_bogosqrt(pair->lhs.record->length)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = pair->lhs.dstart; i <= pair->lhs.dend; i++) {
		u64 mph = pair->lhs.minimal_perfect_hash->ptr[i];
		nm = occurrence.ptr[mph].file2;
		dis1.ptr[i] = (nm == 0) ? NO: (nm >= mlim) ? TOO_MANY: YES;
	}

	if ((mlim = xdl_bogosqrt(pair->rhs.record->length)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = pair->rhs.dstart; i <= pair->rhs.dend; i++) {
		u64 mph = pair->rhs.minimal_perfect_hash->ptr[i];
		nm = occurrence.ptr[mph].file1;
		dis2.ptr[i] = (nm == 0) ? NO: (nm >= mlim) ? TOO_MANY: YES;
	}

	for (i = pair->lhs.dstart; i <= pair->lhs.dend; i++) {
		if (dis1.ptr[i] == YES ||
		    (dis1.ptr[i] == TOO_MANY && !xdl_clean_mmatch((char const *) dis1.ptr, i, pair->lhs.dstart, pair->lhs.dend))) {
			ivec_push(&pair->lhs.rindex, &i);
		} else
			pair->lhs.consider.ptr[SENTINEL + i] = YES;
	}
	ivec_shrink_to_fit(&pair->lhs.rindex);

	for (i = pair->rhs.dstart; i <= pair->rhs.dend; i++) {
		if (dis2.ptr[i] == YES ||
		    (dis2.ptr[i] == TOO_MANY && !xdl_clean_mmatch((char const *) dis2.ptr, i, pair->rhs.dstart, pair->rhs.dend))) {
			ivec_push(&pair->rhs.rindex, &i);
		} else
			pair->rhs.consider.ptr[SENTINEL + i] = YES;
	}
	ivec_shrink_to_fit(&pair->rhs.rindex);

	ivec_free(&dis1);
	ivec_free(&dis2);
}


/*
 * Early trim initial and terminal matching records.
 */
static void xdl_trim_ends(struct xdpair *pair) {
	usize lim = XDL_MIN(pair->lhs.record->length, pair->rhs.record->length);

	for (usize i = 0; i < lim; i++) {
		u64 mph1 = pair->lhs.minimal_perfect_hash->ptr[i];
		u64 mph2 = pair->rhs.minimal_perfect_hash->ptr[i];
		if (mph1 != mph2) {
			pair->lhs.dstart = pair->rhs.dstart = i;
			break;
		}
	}

	for (usize i = 0; i < lim; i++) {
		u64 mph1 = pair->lhs.minimal_perfect_hash->ptr[pair->lhs.minimal_perfect_hash->length - 1 - i];
		u64 mph2 = pair->rhs.minimal_perfect_hash->ptr[pair->lhs.minimal_perfect_hash->length - 1 - i];
		if (mph1 != mph2) {
			pair->lhs.dend = pair->lhs.record->length - 1 - i;
			pair->rhs.dend = pair->rhs.record->length - 1 - i;
			break;
		}
	}
}


static void xdl_optimize_ctxs(struct xdpair *pair) {
	xdl_trim_ends(pair);
	xdl_cleanup_records(pair);
}

int xdl_prepare_env(mmfile_t *mf1, mmfile_t *mf2, xpparam_t const *xpp,
		    struct xdpair *pair) {
	struct xdl_minimal_perfect_hash_builder mphb;


	xdl_prepare_ctx(mf1, xpp, &pair->lhs);
	xdl_prepare_ctx(mf2, xpp, &pair->rhs);

	xdl_mphb_init(&mphb, pair->lhs.record->length + pair->rhs.record->length, xpp->flags);
	for (usize i = 0; i < pair->lhs.record->length; i++) {
		struct xrecord *rec = &pair->lhs.record->ptr[i];
		pair->lhs.minimal_perfect_hash->ptr[i] = xdl_mphb_hash(&mphb, rec);
	}
	pair->lhs.minimal_perfect_hash->length = pair->lhs.record->length;

	for (usize i = 0; i < pair->rhs.record->length; i++) {
		struct xrecord *rec = &pair->rhs.record->ptr[i];
		pair->rhs.minimal_perfect_hash->ptr[i] = xdl_mphb_hash(&mphb, rec);
	}
	pair->rhs.minimal_perfect_hash->length = pair->rhs.record->length;

	pair->minimal_perfect_hash_size = xdl_mphb_finish(&mphb);


	if ((xpp->flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0) {
		xdl_optimize_ctxs(pair);
	}


	return 0;
}
