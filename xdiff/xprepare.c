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


static bool xdl_clean_mmatch(struct ivec_u8* dis, long i, long s, long e) {
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
		if (dis->ptr[i - r] == NO)
			rdis0++;
		else if (dis->ptr[i - r] == TOO_MANY)
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
		return false;
	for (r = 1, rdis1 = 0, rpdis1 = 1; (i + r) < e; r++) {
		if (dis->ptr[i + r] == NO)
			rdis1++;
		else if (dis->ptr[i + r] == TOO_MANY)
			rpdis1++;
		else
			break;
	}
	/*
	 * If the run after the line 'i' found only multimatch lines, we
	 * return 0 and hence we don't make the current line (i) discarded.
	 */
	if (rdis1 == 0)
		return false;
	rdis1 += rdis0;
	rpdis1 += rpdis0;

	return rpdis1 * XDL_KPDIS_RUN < rpdis1 + rdis1;
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
	usize end1 = pair->lhs.minimal_perfect_hash->length - pair->delta_end;
	usize end2 = pair->rhs.minimal_perfect_hash->length - pair->delta_end;

	if (pair->lhs.minimal_perfect_hash->length != pair->lhs.record->length)
		BUG("mph size != record size");
	if (pair->rhs.minimal_perfect_hash->length != pair->rhs.record->length)
		BUG("mph size != record size");

	IVEC_INIT(dis1);
	IVEC_INIT(dis2);
	IVEC_INIT(occurrence);

	ivec_zero(&dis1, pair->lhs.minimal_perfect_hash->length + SENTINEL);
	ivec_zero(&dis2, pair->rhs.minimal_perfect_hash->length + SENTINEL);
	ivec_zero(&occurrence, pair->minimal_perfect_hash_size);

	for (usize i = 0; i < pair->lhs.minimal_perfect_hash->length; i++) {
		u64 mph = pair->lhs.minimal_perfect_hash->ptr[i];
		occurrence.ptr[mph].file1 += 1;
	}

	for (usize i = 0; i < pair->rhs.minimal_perfect_hash->length; i++) {
		u64 mph = pair->rhs.minimal_perfect_hash->ptr[i];
		occurrence.ptr[mph].file2 += 1;
	}

	if ((mlim = xdl_bogosqrt(pair->lhs.minimal_perfect_hash->length)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = pair->delta_start; i < end1; i++) {
		u64 mph = pair->lhs.minimal_perfect_hash->ptr[i];
		nm = occurrence.ptr[mph].file2;
		dis1.ptr[i] = (nm == 0) ? NO: (nm >= mlim) ? TOO_MANY: YES;
	}

	if ((mlim = xdl_bogosqrt(pair->rhs.minimal_perfect_hash->length)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = pair->delta_start; i < end2; i++) {
		u64 mph = pair->rhs.minimal_perfect_hash->ptr[i];
		nm = occurrence.ptr[mph].file1;
		dis2.ptr[i] = (nm == 0) ? NO: (nm >= mlim) ? TOO_MANY: YES;
	}

	for (i = pair->delta_start; i < end1; i++) {
		if (dis1.ptr[i] == YES ||
		    (dis1.ptr[i] == TOO_MANY && !xdl_clean_mmatch(&dis1, i, pair->delta_start, end1))) {
			ivec_push(&pair->lhs.rindex, &i);
		} else
			pair->lhs.consider.ptr[SENTINEL + i] = YES;
	}
	ivec_shrink_to_fit(&pair->lhs.rindex);

	for (i = pair->delta_start; i < end2; i++) {
		if (dis2.ptr[i] == YES ||
		    (dis2.ptr[i] == TOO_MANY && !xdl_clean_mmatch(&dis2, i, pair->delta_start, end2))) {
			ivec_push(&pair->rhs.rindex, &i);
		} else
			pair->rhs.consider.ptr[SENTINEL + i] = YES;
	}
	ivec_shrink_to_fit(&pair->rhs.rindex);

	ivec_free(&dis1);
	ivec_free(&dis2);
	ivec_free(&occurrence);
}


/*
 * Early trim initial and terminal matching records.
 */
static void xdl_trim_ends(struct xdpair *pair) {
	usize lim = XDL_MIN(pair->lhs.minimal_perfect_hash->length, pair->rhs.minimal_perfect_hash->length);

	for (usize i = 0; i < lim; i++) {
		u64 mph1 = pair->lhs.minimal_perfect_hash->ptr[i];
		u64 mph2 = pair->rhs.minimal_perfect_hash->ptr[i];
		if (mph1 != mph2) {
			pair->delta_start = i;
			lim -= i;
			break;
		}
	}

	for (usize i = 0; i < lim; i++) {
		usize i1 = pair->lhs.minimal_perfect_hash->length - 1 - i;
		usize i2 = pair->rhs.minimal_perfect_hash->length - 1 - i;
		u64 mph1 = pair->lhs.minimal_perfect_hash->ptr[i1];
		u64 mph2 = pair->rhs.minimal_perfect_hash->ptr[i2];
		if (mph1 != mph2) {
			pair->delta_end = i;
			break;
		}
	}
}


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
