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



void xdl_file_prepare(mmfile_t *mf, struct xdfile *file) {
	struct xlinereader reader;

	xd_trace2_region_enter("xdiff", "xdl_file_prepare");

	IVEC_INIT(file->record);
	IVEC_INIT(file->minimal_perfect_hash);

	xdl_linereader_init(&reader, (u8 const *) mf->ptr, mf->size);
	while (true) {
		struct xrecord rec_new;
		if (!xdl_linereader_next(&reader, &rec_new.ptr, &rec_new.size_no_eol, &rec_new.size_with_eol))
			break;
		ivec_push(&file->record, &rec_new);
	}
	ivec_shrink_to_fit(&file->record);

	xd_trace2_region_leave("xdiff", "xdl_file_prepare");
}

void xdl_file_free(struct  xdfile *file) {
	ivec_free(&file->minimal_perfect_hash);
	ivec_free(&file->record);
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
	for (r = 1, rdis1 = 0, rpdis1 = 1; (i + r) < e; r++) {
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
	usize end1 = pair->lhs.record->length - pair->delta_end;
	usize end2 = pair->rhs.record->length - pair->delta_end;

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
	for (i = pair->delta_start; i < end1; i++) {
		u64 mph = pair->lhs.minimal_perfect_hash->ptr[i];
		nm = occurrence.ptr[mph].file2;
		dis1.ptr[i] = (nm == 0) ? NO: (nm >= mlim) ? TOO_MANY: YES;
	}

	if ((mlim = xdl_bogosqrt(pair->rhs.record->length)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = pair->delta_start; i < end2; i++) {
		u64 mph = pair->rhs.minimal_perfect_hash->ptr[i];
		nm = occurrence.ptr[mph].file1;
		dis2.ptr[i] = (nm == 0) ? NO: (nm >= mlim) ? TOO_MANY: YES;
	}

	for (i = pair->delta_start; i < end1; i++) {
		if (dis1.ptr[i] == YES ||
		    (dis1.ptr[i] == TOO_MANY && !xdl_clean_mmatch((char const *) dis1.ptr, i, pair->delta_start, end1))) {
			ivec_push(&pair->lhs.rindex, &i);
		} else
			pair->lhs.consider.ptr[SENTINEL + i] = YES;
	}
	ivec_shrink_to_fit(&pair->lhs.rindex);

	for (i = pair->delta_start; i < end2; i++) {
		if (dis2.ptr[i] == YES ||
		    (dis2.ptr[i] == TOO_MANY && !xdl_clean_mmatch((char const *) dis2.ptr, i, pair->delta_start, end2))) {
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
			pair->delta_start = i;
			break;
		}
	}

	for (usize i = 0; i < lim; i++) {
		u64 mph1 = pair->lhs.minimal_perfect_hash->ptr[pair->lhs.minimal_perfect_hash->length - 1 - i];
		u64 mph2 = pair->rhs.minimal_perfect_hash->ptr[pair->lhs.minimal_perfect_hash->length - 1 - i];
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


static void xdl_setup_ctx(struct xdfile *file, struct xd_file_context *ctx) {
	ctx->minimal_perfect_hash = &file->minimal_perfect_hash;

	ctx->record = &file->record;

	IVEC_INIT(ctx->consider);
	ivec_zero(&ctx->consider, SENTINEL + ctx->record->length + SENTINEL);

	IVEC_INIT(ctx->rindex);
}

static void xdl_pair_prepare(struct xdfile *lhs, struct xdfile *rhs, usize mph_size, u64 flags, struct xdpair *pair) {
	pair->delta_start = 0;
	pair->delta_end = 0;
	pair->minimal_perfect_hash_size = mph_size;

	xdl_setup_ctx(lhs, &pair->lhs);
	xdl_setup_ctx(rhs, &pair->rhs);


	if ((flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0) {
		xdl_optimize_ctxs(pair);
	}
}

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

static void xdl_hash_records(struct xdfile *file, u64 flags) {
	for (usize i = 0; i < file->record.length; i++) {
		struct xrecord *rec = &file->record.ptr[i];
		rec->line_hash = xdl_line_hash(rec->ptr, rec->size_no_eol, flags);
	}
}

void xdl_2way_prepare(mmfile_t *mf1, mmfile_t *mf2, u64 flags, struct xd2way *two_way) {
	struct xdl_minimal_perfect_hash_builder mphb;
	usize max_unique_keys = 0;

	xd_trace2_region_enter("xdiff", "xdl_2way_prepare");

	xdl_file_prepare(mf1, &two_way->lhs);
	xdl_file_prepare(mf2, &two_way->rhs);

	xdl_hash_records(&two_way->lhs, flags);
	xdl_hash_records(&two_way->rhs, flags);

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

	xdl_file_prepare(orig, &three_way->base);
	xdl_file_prepare(mf1, &three_way->side1);
	xdl_file_prepare(mf2, &three_way->side2);

	xdl_hash_records(&three_way->base, flags);
	xdl_hash_records(&three_way->side1, flags);
	xdl_hash_records(&three_way->side2, flags);

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

