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
			       struct xrecord *rec) {
	long hi;
	char const *line;
	xdlclass_t *rcrec;

	line = rec->ptr;
	hi = (long) XDL_HASHLONG(rec->ha, cf->hbits);
	for (rcrec = cf->rchash[hi]; rcrec; rcrec = rcrec->next)
		if (rcrec->ha == rec->ha &&
				xdl_recmatch(rcrec->line, rcrec->size,
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
		rcrec->line = line;
		rcrec->size = rec->size;
		rcrec->ha = rec->ha;
		rcrec->len1 = rcrec->len2 = 0;
		rcrec->next = cf->rchash[hi];
		cf->rchash[hi] = rcrec;
	}

	(pass == 1) ? rcrec->len1++ : rcrec->len2++;

	rec->ha = (unsigned long) rcrec->idx;


	return 0;
}


static int xdl_prepare_ctx(mmfile_t *mf, xpparam_t const *xpp,
			   struct xd_file_context *ctx) {
	long bsize;
	unsigned long hav;
	char const *blk, *cur, *top, *prev;
	char *rchg;
	long *rindex;

	rindex = NULL;
	rchg = NULL;

	IVEC_INIT(ctx->file_storage.minimal_perfect_hash);
	IVEC_INIT(ctx->file_storage.record);
	ctx->minimal_perfect_hash = &ctx->file_storage.minimal_perfect_hash;
	ctx->record = &ctx->file_storage.record;
	IVEC_INIT(ctx->record_ptr);


	if ((cur = blk = xdl_mmfile_first(mf, &bsize))) {
		for (top = blk + bsize; cur < top; ) {
			struct xrecord rec_new;
			prev = cur;
			hav = xdl_hash_record(&cur, top, xpp->flags);
			rec_new.ptr = prev;
			rec_new.size = (long) (cur - prev);
			rec_new.ha = hav;
			ivec_push(&ctx->file_storage.record, &rec_new);
		}
	}
	ivec_shrink_to_fit(&ctx->file_storage.record);

	ivec_reserve_exact(&ctx->record_ptr, ctx->file_storage.record.length);
	for (usize i = 0; i < ctx->file_storage.record.length; i++) {
		struct xrecord *rec = &ctx->file_storage.record.ptr[i];
		ivec_push(&ctx->record_ptr, &rec);
	}

	if (!XDL_CALLOC_ARRAY(rchg, ctx->record->length + 2))
		goto abort;

	if ((XDF_DIFF_ALG(xpp->flags) != XDF_PATIENCE_DIFF) &&
	    (XDF_DIFF_ALG(xpp->flags) != XDF_HISTOGRAM_DIFF)) {
		if (!XDL_ALLOC_ARRAY(rindex, ctx->record->length + 1))
			goto abort;
	}

	ctx->nrec = ctx->record->length;
	ctx->recs = ctx->record_ptr.ptr;
	ctx->rchg = rchg + 1;
	ctx->rindex = rindex;
	ctx->nreff = 0;
	ctx->dstart = 0;
	ctx->dend = ctx->record->length - 1;

	return 0;

abort:
	xdl_free(rindex);
	xdl_free(rchg);
	ivec_free(&ctx->file_storage.minimal_perfect_hash);
	ivec_free(&ctx->file_storage.record);
	ivec_free(&ctx->record_ptr);
	return -1;
}


static void xdl_free_ctx(struct xd_file_context *ctx) {
	xdl_free(ctx->rindex);
	xdl_free(ctx->rchg - 1);
	ivec_free(&ctx->file_storage.minimal_perfect_hash);
	ivec_free(&ctx->file_storage.record);
	ivec_free(&ctx->record_ptr);
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
static int xdl_cleanup_records(xdlclassifier_t *cf, struct xd_file_context *lhs, struct xd_file_context *xdf2) {
	long i, nm, nreff, mlim;
	struct xrecord **recs;
	xdlclass_t *rcrec;
	char *dis, *dis1, *dis2;

	if (!XDL_CALLOC_ARRAY(dis, lhs->nrec + xdf2->nrec + 2))
		return -1;
	dis1 = dis;
	dis2 = dis1 + lhs->nrec + 1;

	if ((mlim = xdl_bogosqrt(lhs->nrec)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = lhs->dstart, recs = &lhs->recs[lhs->dstart]; i <= lhs->dend; i++, recs++) {
		rcrec = cf->rcrecs[(*recs)->ha];
		nm = rcrec ? rcrec->len2 : 0;
		dis1[i] = (nm == 0) ? 0: (nm >= mlim) ? 2: 1;
	}

	if ((mlim = xdl_bogosqrt(xdf2->nrec)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = xdf2->dstart, recs = &xdf2->recs[xdf2->dstart]; i <= xdf2->dend; i++, recs++) {
		rcrec = cf->rcrecs[(*recs)->ha];
		nm = rcrec ? rcrec->len1 : 0;
		dis2[i] = (nm == 0) ? 0: (nm >= mlim) ? 2: 1;
	}

	for (nreff = 0, i = lhs->dstart, recs = &lhs->recs[lhs->dstart];
	     i <= lhs->dend; i++, recs++) {
		if (dis1[i] == 1 ||
		    (dis1[i] == 2 && !xdl_clean_mmatch(dis1, i, lhs->dstart, lhs->dend))) {
			lhs->rindex[nreff] = i;
			nreff++;
		} else
			lhs->rchg[i] = 1;
	}
	lhs->nreff = nreff;

	for (nreff = 0, i = xdf2->dstart, recs = &xdf2->recs[xdf2->dstart];
	     i <= xdf2->dend; i++, recs++) {
		if (dis2[i] == 1 ||
		    (dis2[i] == 2 && !xdl_clean_mmatch(dis2, i, xdf2->dstart, xdf2->dend))) {
			xdf2->rindex[nreff] = i;
			nreff++;
		} else
			xdf2->rchg[i] = 1;
	}
	xdf2->nreff = nreff;

	xdl_free(dis);

	return 0;
}


/*
 * Early trim initial and terminal matching records.
 */
static int xdl_trim_ends(struct xd_file_context *lhs, struct xd_file_context *rhs) {
	long i, lim;
	struct xrecord **recs1, **recs2;

	recs1 = lhs->recs;
	recs2 = rhs->recs;
	for (i = 0, lim = XDL_MIN(lhs->nrec, rhs->nrec); i < lim;
	     i++, recs1++, recs2++)
		if ((*recs1)->ha != (*recs2)->ha)
			break;

	lhs->dstart = rhs->dstart = i;

	recs1 = lhs->recs + lhs->nrec - 1;
	recs2 = rhs->recs + rhs->nrec - 1;
	for (lim -= i, i = 0; i < lim; i++, recs1--, recs2--)
		if ((*recs1)->ha != (*recs2)->ha)
			break;

	lhs->dend = lhs->nrec - i - 1;
	rhs->dend = rhs->nrec - i - 1;

	return 0;
}


static int xdl_optimize_ctxs(xdlclassifier_t *cf, struct xd_file_context *lhs, struct xd_file_context *rhs) {

	if (xdl_trim_ends(lhs, rhs) < 0 ||
	    xdl_cleanup_records(cf, lhs, rhs) < 0) {

		return -1;
	}

	return 0;
}

int xdl_prepare_env(mmfile_t *mf1, mmfile_t *mf2, xpparam_t const *xpp,
		    struct xdpair *pair) {
	xdlclassifier_t cf;

	memset(&cf, 0, sizeof(cf));

	if (xdl_prepare_ctx(mf1, xpp, &pair->lhs) < 0) {

		xdl_free_classifier(&cf);
		return -1;
	}
	if (xdl_prepare_ctx(mf2, xpp, &pair->rhs) < 0) {

		xdl_free_ctx(&pair->lhs);
		xdl_free_classifier(&cf);
		return -1;
	}

	if (xdl_init_classifier(&cf, pair->lhs.nrec + pair->rhs.nrec + 1, xpp->flags) < 0)
		return -1;

	for (isize i = 0; i < pair->lhs.nrec; i++) {
		xdl_classify_record(1, &cf, pair->lhs.recs[i]);
	}

	for (isize i = 0; i < pair->rhs.nrec; i++) {
		xdl_classify_record(2, &cf, pair->rhs.recs[i]);
	}

	if ((XDF_DIFF_ALG(xpp->flags) != XDF_PATIENCE_DIFF) &&
	    (XDF_DIFF_ALG(xpp->flags) != XDF_HISTOGRAM_DIFF) &&
	    xdl_optimize_ctxs(&cf, &pair->lhs, &pair->rhs) < 0) {

		xdl_free_ctx(&pair->rhs);
		xdl_free_ctx(&pair->lhs);
		xdl_free_classifier(&cf);
		return -1;
	    }

	xdl_free_classifier(&cf);

	return 0;
}
