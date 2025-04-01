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


typedef struct s_xdlclass {
	struct s_xdlclass *next;
	unsigned long ha;
	char const *line;
	long size;
	long idx;
	long len1, len2;
} xdlclass_t;

typedef struct s_xdlclassifier {
	unsigned int hbits;
	long hsize;
	xdlclass_t **rchash;
	chastore_t ncha;
	xdlclass_t **rcrecs;
	long alloc;
	long count;
	long flags;
} xdlclassifier_t;



static int xdl_init_classifier(xdlclassifier_t *cf, long size, long flags) {
	cf->flags = flags;

	cf->hbits = xdl_hashbits((unsigned int) size);
	cf->hsize = 1 << cf->hbits;

	if (xdl_cha_init(&cf->ncha, sizeof(xdlclass_t), size / 4 + 1) < 0) {

		return -1;
	}
	if (!XDL_CALLOC_ARRAY(cf->rchash, cf->hsize)) {

		xdl_cha_free(&cf->ncha);
		return -1;
	}

	cf->alloc = size;
	if (!XDL_ALLOC_ARRAY(cf->rcrecs, cf->alloc)) {

		xdl_free(cf->rchash);
		xdl_cha_free(&cf->ncha);
		return -1;
	}

	cf->count = 0;

	return 0;
}


static void xdl_free_classifier(xdlclassifier_t *cf) {

	xdl_free(cf->rcrecs);
	xdl_free(cf->rchash);
	xdl_cha_free(&cf->ncha);
}


static int xdl_classify_record(unsigned int pass, xdlclassifier_t *cf,
			       struct xrecord *rec, u64 *mph) {
	long hi;
	xdlclass_t *rcrec;

	u64 line_hash = xdl_line_hash(rec->ptr, rec->size, cf->flags);
	hi = (long) XDL_HASHLONG(line_hash, cf->hbits);
	for (rcrec = cf->rchash[hi]; rcrec; rcrec = rcrec->next)
		if (rcrec->ha == line_hash &&
				xdl_line_equal((u8 const*) rcrec->line, rcrec->size,
					rec->ptr, rec->size, cf->flags))
			break;

	if (!rcrec) {
		if (!(rcrec = xdl_cha_alloc(&cf->ncha))) {

			return -1;
		}
		rcrec->idx = cf->count++;
		if (XDL_ALLOC_GROW(cf->rcrecs, cf->count, cf->alloc))
				return -1;
		cf->rcrecs[rcrec->idx] = rcrec;
		rcrec->line = (char const*) rec->ptr;
		rcrec->size = rec->size;
		rcrec->ha = line_hash;
		rcrec->len1 = rcrec->len2 = 0;
		rcrec->next = cf->rchash[hi];
		cf->rchash[hi] = rcrec;
	}

	(pass == 1) ? rcrec->len1++ : rcrec->len2++;

	*mph = (unsigned long) rcrec->idx;

	return 0;
}


extern void xdl_parse_lines(mmfile_t const* file, struct ivec_xrecord* record);

static void xdl_prepare_ctx(mmfile_t *mf, xpparam_t const *xpp,
			   struct xd_file_context *ctx) {
	u8 const* cur = (u8 const*) mf->ptr;
	u8 const* top = (u8 const*) mf->ptr + mf->size;

	IVEC_INIT(ctx->file_storage.record);
	while (cur < top) {
		struct xrecord rec;
		rec.ptr = cur;
		while (cur < top && *cur++ != '\n') {}
		rec.size = cur - rec.ptr;
		ivec_push(&ctx->file_storage.record, &rec);
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
	for (r = 1, rdis1 = 0, rpdis1 = 1; (i + r) <= e; r++) {
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
static void xdl_cleanup_records(xdlclassifier_t *cf, struct xdpair *pair) {
	long i, nm, mlim;
	xdlclass_t *rcrec;
	struct ivec_u8 dis1, dis2;

	IVEC_INIT(dis1);
	IVEC_INIT(dis2);

	ivec_zero(&dis1, pair->lhs.record->length + SENTINEL);
	ivec_zero(&dis2, pair->rhs.record->length + SENTINEL);

	if ((mlim = xdl_bogosqrt(pair->lhs.record->length)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = pair->lhs.dstart; i <= pair->lhs.dend; i++) {
		u64 mph = pair->lhs.minimal_perfect_hash->ptr[i];
		rcrec = cf->rcrecs[mph];
		nm = rcrec ? rcrec->len2 : 0;
		dis1.ptr[i] = (nm == 0) ? NO: (nm >= mlim) ? TOO_MANY: YES;
	}

	if ((mlim = xdl_bogosqrt(pair->rhs.record->length)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = pair->rhs.dstart; i <= pair->rhs.dend; i++) {
		u64 mph = pair->rhs.minimal_perfect_hash->ptr[i];
		rcrec = cf->rcrecs[mph];
		nm = rcrec ? rcrec->len1 : 0;
		dis2.ptr[i] = (nm == 0) ? NO: (nm >= mlim) ? TOO_MANY: YES;
	}

	for (i = pair->lhs.dstart; i <= pair->lhs.dend; i++) {
		if (dis1.ptr[i] == YES ||
		    (dis1.ptr[i] == TOO_MANY && !xdl_clean_mmatch(&dis1, i, pair->lhs.dstart, pair->lhs.dend))) {
			ivec_push(&pair->lhs.rindex, &i);
		} else
			pair->lhs.consider.ptr[SENTINEL + i] = YES;
	}
	ivec_shrink_to_fit(&pair->lhs.rindex);

	for (i = pair->rhs.dstart; i <= pair->rhs.dend; i++) {
		if (dis2.ptr[i] == YES ||
		    (dis2.ptr[i] == TOO_MANY && !xdl_clean_mmatch(&dis2, i, pair->rhs.dstart, pair->rhs.dend))) {
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
	usize lim = XDL_MIN(pair->lhs.minimal_perfect_hash->length, pair->rhs.minimal_perfect_hash->length);

	for (usize i = 0; i < lim; i++) {
		u64 mph1 = pair->lhs.minimal_perfect_hash->ptr[i];
		u64 mph2 = pair->rhs.minimal_perfect_hash->ptr[i];
		if (mph1 != mph2) {
			lim -= i;
			pair->lhs.dstart = pair->rhs.dstart = i;
			break;
		}
	}

	for (usize i = 0; i < lim; i++) {
		usize i1 = pair->lhs.minimal_perfect_hash->length - 1 - i;
		usize i2 = pair->rhs.minimal_perfect_hash->length - 1 - i;
		u64 mph1 = pair->lhs.minimal_perfect_hash->ptr[i1];
		u64 mph2 = pair->rhs.minimal_perfect_hash->ptr[i2];
		if (mph1 != mph2) {
			pair->lhs.dend = (isize) i1;
			pair->rhs.dend = (isize) i2;
			break;
		}
	}
}


static void xdl_optimize_ctxs(xdlclassifier_t *cf, struct xdpair *pair) {
	xdl_trim_ends(pair);
	xdl_cleanup_records(cf, pair);
}

extern u64 link_with_rust();

int xdl_prepare_env(mmfile_t *mf1, mmfile_t *mf2, xpparam_t const *xpp,
		    struct xdpair *pair) {
	xdlclassifier_t cf;

	memset(&cf, 0, sizeof(cf));

	xdl_prepare_ctx(mf1, xpp, &pair->lhs);
	xdl_prepare_ctx(mf2, xpp, &pair->rhs);

	if (xdl_init_classifier(&cf, pair->lhs.record->length + pair->rhs.record->length + 1, xpp->flags) < 0)
		return -1;

	for (usize i = 0; i < pair->lhs.record->length; i++) {
		xdl_classify_record(1, &cf, &pair->lhs.record->ptr[i], &pair->lhs.minimal_perfect_hash->ptr[i]);
	}

	for (usize i = 0; i < pair->rhs.record->length; i++) {
		xdl_classify_record(2, &cf, &pair->rhs.record->ptr[i], &pair->rhs.minimal_perfect_hash->ptr[i]);
	}

	if ((xpp->flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0) {
		xdl_optimize_ctxs(&cf, pair);
	}

	xdl_free_classifier(&cf);

	return 0;
}
