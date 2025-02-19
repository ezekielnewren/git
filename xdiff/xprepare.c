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
	xrecord_t rec;
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




static int xdl_init_classifier(xdlclassifier_t *cf, long size, long flags);
static void xdl_free_classifier(xdlclassifier_t *cf);
static int xdl_classify_record(unsigned int pass, xdlclassifier_t *cf, xrecord_t *rec);
static void xdl_free_ctx(xdfile_t *xdf);
static int xdl_clean_mmatch(char const *dis, long i, long s, long e);
static int xdl_cleanup_records(xdlclassifier_t *cf, xdfile_t *xdf1, xdfile_t *xdf2);
static int xdl_trim_ends(xdfile_t *xdf1, xdfile_t *xdf2);
static int xdl_optimize_ctxs(xdlclassifier_t *cf, xdfile_t *xdf1, xdfile_t *xdf2);




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


static int xdl_classify_record(unsigned int pass, xdlclassifier_t *cf, xrecord_t *rec) {
	long hi;
	xdlclass_t *rcrec;

	hi = (long) XDL_HASHLONG(rec->hash, cf->hbits);
	for (rcrec = cf->rchash[hi]; rcrec; rcrec = rcrec->next)
		if (rcrec->rec.hash == rec->hash &&
				xdl_recmatch((const char *) rcrec->rec.ptr, rcrec->rec.size,
					(const char *) rec->ptr, rec->size, cf->flags))
			break;

	if (!rcrec) {
		if (!(rcrec = xdl_cha_alloc(&cf->ncha))) {

			return -1;
		}
		rcrec->idx = cf->count++;
		if (XDL_ALLOC_GROW(cf->rcrecs, cf->count, cf->alloc))
				return -1;
		cf->rcrecs[rcrec->idx] = rcrec;
		rcrec->rec = *rec;
		rcrec->len1 = rcrec->len2 = 0;
		rcrec->next = cf->rchash[hi];
		cf->rchash[hi] = rcrec;
	}

	(pass == 1) ? rcrec->len1++ : rcrec->len2++;

	rec->hash = (u64) rcrec->idx;

	return 0;
}


#ifndef NO_RUST
u64 rust_xdl_hash_record(char const **data, char const *top, long flags);
#endif


static int xdl_prepare_ctx(mmfile_t *mf, xdfile_t *xdf, u64 flags) {
	long bsize;
	u64 c_hash;
	u64 rust_hash;
	char const *blk, *cur, *top, *prev, *tmp;
	u8 default_value = 0;

	IVEC_INIT(xdf->record);
	IVEC_INIT(xdf->rindex);
	IVEC_INIT(xdf->hash);
	IVEC_INIT(xdf->rchg_vec);

	if ((cur = blk = xdl_mmfile_first(mf, &bsize))) {
		for (top = blk + bsize; cur < top; ) {
			xrecord_t crec;
			prev = cur;
#ifdef NO_RUST
			c_hash = xdl_hash_record(&cur, top, flags);
#else
			tmp = cur;
			rust_hash = rust_xdl_hash_record(&tmp, top, flags);
			tmp = cur;
			c_hash = xdl_hash_record(&tmp, top, flags);
			if (rust_hash != c_hash) {
				BUG("c and rust disagree on the line hash");
			}
			cur = tmp;
#endif
			crec.ptr = (u8 *) prev;
			crec.size = (long) (cur - prev);
			crec.hash = c_hash;
			crec.flags = flags;
			rust_ivec_push(&xdf->record, &crec);
		}
	}


	rust_ivec_resize_exact(&xdf->rchg_vec, xdf->record.length + 2, &default_value);

	if ((flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0) {
		rust_ivec_reserve_exact(&xdf->rindex, xdf->record.length + 1);
		rust_ivec_reserve_exact(&xdf->hash, xdf->record.length + 1);
	}

	xdf->rchg = xdf->rchg_vec.ptr + 1;
	xdf->dstart = 0;
	xdf->dend = xdf->record.length - 1;

	return 0;
}


static void xdl_free_ctx(xdfile_t *xdf) {

	rust_ivec_free(&xdf->rchg_vec);
	rust_ivec_free(&xdf->rindex);
	rust_ivec_free(&xdf->hash);
	rust_ivec_free(&xdf->record);
}


int xdl_prepare_env(mmfile_t *mf1, mmfile_t *mf2, xpparam_t const *xpp,
		    xdfenv_t *xe) {
	xdlclassifier_t cf;

	memset(&cf, 0, sizeof(cf));

	if (xdl_prepare_ctx(mf1, &xe->xdf1, xpp->flags) < 0) {

		xdl_free_classifier(&cf);
		return -1;
	}
	if (xdl_prepare_ctx(mf2, &xe->xdf2, xpp->flags) < 0) {

		xdl_free_ctx(&xe->xdf1);
		xdl_free_classifier(&cf);
		return -1;
	}

	if (xdl_init_classifier(&cf, xe->xdf1.record.length + xe->xdf2.record.length + 1, xpp->flags) < 0)
		return -1;

	for (usize i = 0; i < xe->xdf1.record.length; i++) {
		xrecord_t *rec = &xe->xdf1.record.ptr[i];
		xdl_classify_record(1, &cf, rec);
	}
	for (usize i = 0; i < xe->xdf2.record.length; i++) {
		xrecord_t *rec = &xe->xdf2.record.ptr[i];
		xdl_classify_record(2, &cf, rec);
	}

	if ((XDF_DIFF_ALG(xpp->flags) != XDF_PATIENCE_DIFF) &&
	    (XDF_DIFF_ALG(xpp->flags) != XDF_HISTOGRAM_DIFF) &&
	    xdl_optimize_ctxs(&cf, &xe->xdf1, &xe->xdf2) < 0) {

		xdl_free_ctx(&xe->xdf2);
		xdl_free_ctx(&xe->xdf1);
		xdl_free_classifier(&cf);
		return -1;
	}

	xdl_free_classifier(&cf);

	return 0;
}


void xdl_free_env(xdfenv_t *xe) {

	xdl_free_ctx(&xe->xdf2);
	xdl_free_ctx(&xe->xdf1);
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
static int xdl_cleanup_records(xdlclassifier_t *cf, xdfile_t *xdf1, xdfile_t *xdf2) {
	long i, nm, mlim;
	xrecord_t *recs;
	xdlclass_t *rcrec;
	char *dis, *dis1, *dis2;

	if (!XDL_CALLOC_ARRAY(dis, xdf1->record.length + xdf2->record.length + 2))
		return -1;
	dis1 = dis;
	dis2 = dis1 + xdf1->record.length + 1;

	if ((mlim = xdl_bogosqrt(xdf1->record.length)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = xdf1->dstart, recs = &xdf1->record.ptr[xdf1->dstart]; i <= xdf1->dend; i++, recs++) {
		rcrec = cf->rcrecs[recs->hash];
		nm = rcrec ? rcrec->len2 : 0;
		dis1[i] = (nm == 0) ? 0: (nm >= mlim) ? 2: 1;
	}

	if ((mlim = xdl_bogosqrt(xdf2->record.length)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = xdf2->dstart, recs = &xdf2->record.ptr[xdf2->dstart]; i <= xdf2->dend; i++, recs++) {
		rcrec = cf->rcrecs[recs->hash];
		nm = rcrec ? rcrec->len1 : 0;
		dis2[i] = (nm == 0) ? 0: (nm >= mlim) ? 2: 1;
	}

	for (i = xdf1->dstart, recs = &xdf1->record.ptr[xdf1->dstart];
	     i <= xdf1->dend; i++, recs++) {
		if (dis1[i] == 1 ||
		    (dis1[i] == 2 && !xdl_clean_mmatch(dis1, i, xdf1->dstart, xdf1->dend))) {
			rust_ivec_push(&xdf1->rindex, &i);
			rust_ivec_push(&xdf1->hash, &recs->hash);
		} else
			xdf1->rchg[i] = 1;
	}

	for (i = xdf2->dstart, recs = &xdf2->record.ptr[xdf2->dstart];
	     i <= xdf2->dend; i++, recs++) {
		if (dis2[i] == 1 ||
		    (dis2[i] == 2 && !xdl_clean_mmatch(dis2, i, xdf2->dstart, xdf2->dend))) {
			rust_ivec_push(&xdf2->rindex, &i);
			rust_ivec_push(&xdf2->hash, &recs->hash);
		} else
			xdf2->rchg[i] = 1;
	}

	xdl_free(dis);

	return 0;
}


/*
 * Early trim initial and terminal matching records.
 */
static int xdl_trim_ends(xdfile_t *xdf1, xdfile_t *xdf2) {
	long i, lim;
	xrecord_t *recs1, *recs2;

	recs1 = xdf1->record.ptr;
	recs2 = xdf2->record.ptr;
	for (i = 0, lim = XDL_MIN(xdf1->record.length, xdf2->record.length); i < lim;
	     i++, recs1++, recs2++)
		if (recs1->hash != recs2->hash)
			break;

	xdf1->dstart = xdf2->dstart = i;

	recs1 = xdf1->record.ptr + xdf1->record.length - 1;
	recs2 = xdf2->record.ptr + xdf2->record.length - 1;
	for (lim -= i, i = 0; i < lim; i++, recs1--, recs2--)
		if (recs1->hash != recs2->hash)
			break;

	xdf1->dend = xdf1->record.length - i - 1;
	xdf2->dend = xdf2->record.length - i - 1;

	return 0;
}


static int xdl_optimize_ctxs(xdlclassifier_t *cf, xdfile_t *xdf1, xdfile_t *xdf2) {

	if (xdl_trim_ends(xdf1, xdf2) < 0 ||
	    xdl_cleanup_records(cf, xdf1, xdf2) < 0) {

		return -1;
	}

	return 0;
}
