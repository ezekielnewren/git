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


static void xdl_free_ctx(xdfile_t *xdf);
static int xdl_clean_mmatch(char const *dis, long i, long s, long e);
static int xdl_trim_ends(xdfile_t *xdf1, xdfile_t *xdf2);

#ifdef WITH_RUST
extern int rust_xdl_prepare_ctx(mmfile_t *mf, xdfile_t *xdf, u64 flags);
#endif
static int c_xdl_prepare_ctx(mmfile_t *mf, xdfile_t *xdf, u64 flags) {
	struct xlinereader_t reader;

	IVEC_INIT(xdf->minimal_perfect_hash);
	IVEC_INIT(xdf->record);
	IVEC_INIT(xdf->rindex);
	IVEC_INIT(xdf->rchg_vec);

	rust_ivec_reserve_exact(&xdf->record, mf->size >> 4);

	xdl_linereader_init(&reader, (u8 const *) mf->ptr, mf->size);
	while (true) {
		xrecord_t *rec;
		if (xdf->record.length >= xdf->record.capacity)
			rust_ivec_reserve(&xdf->record, 1);
		rec = &xdf->record.ptr[xdf->record.length++];
		if (!xdl_linereader_next(&reader, &rec->ptr, &rec->size_no_eol, &rec->size_with_eol)) {
			xdf->record.length--;
			break;
		}
	}

	if ((flags & XDF_IGNORE_CR_AT_EOL) != 0) {
		for (usize i = 0; i < xdf->record.length; i++) {
			xrecord_t *rec = &xdf->record.ptr[i];
			if (rec->size_no_eol > 0 && rec->ptr[rec->size_no_eol - 1] == '\r')
				rec->size_no_eol--;
		}
	}

	xdf->rchg_vec.capacity = xdf->record.length + 2;
	XDL_CALLOC_ARRAY(xdf->rchg_vec.ptr, xdf->rchg_vec.capacity);
	xdf->rchg_vec.length = xdf->rchg_vec.capacity;

	rust_ivec_reserve_exact(&xdf->minimal_perfect_hash, xdf->record.length);

	xdf->rchg = xdf->rchg_vec.ptr + 1;
	xdf->dstart = 0;
	xdf->dend = xdf->record.length - 1;

	return 0;
}


static void xdl_free_ctx(xdfile_t *xdf) {

	rust_ivec_free(&xdf->rchg_vec);
	rust_ivec_free(&xdf->minimal_perfect_hash);
	rust_ivec_free(&xdf->rindex);
	rust_ivec_free(&xdf->record);
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

typedef struct {
	usize file1;
	usize file2;
} xdloccurrence_t;

DEFINE_IVEC_TYPE(xdloccurrence_t, xdloccurrence_t);

/*
 * Try to reduce the problem complexity, discard records that have no
 * matches on the other file. Also, lines that have multiple matches
 * might be potentially discarded if they happear in a run of discardable.
 */
static int xdl_cleanup_records(xdfenv_t *xe, ivec_xdloccurrence_t *occ) {
	long i, nm, mlim;
	ivec_u8 dis1;
	ivec_u8 dis2;

	IVEC_INIT(dis1);
	IVEC_INIT(dis2);

	dis1.capacity = dis1.length = xe->xdf1.rchg_vec.length;
	XDL_CALLOC_ARRAY(dis1.ptr, dis1.capacity);

	dis2.capacity = dis2.length = xe->xdf2.rchg_vec.length;
	XDL_CALLOC_ARRAY(dis2.ptr, dis2.capacity);

	rust_ivec_reserve_exact(&xe->xdf1.rindex, xe->xdf1.record.length);
	rust_ivec_reserve_exact(&xe->xdf2.rindex, xe->xdf2.record.length);

	if ((mlim = xdl_bogosqrt(xe->xdf1.record.length)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = xe->xdf1.dstart; i <= xe->xdf1.dend; i++) {
		u64 mph = xe->xdf1.minimal_perfect_hash.ptr[i];
		nm = occ->ptr[mph].file2;
		dis1.ptr[i] = (nm == 0) ? 0: (nm >= mlim) ? 2: 1;
	}

	if ((mlim = xdl_bogosqrt(xe->xdf2.record.length)) > XDL_MAX_EQLIMIT)
		mlim = XDL_MAX_EQLIMIT;
	for (i = xe->xdf2.dstart; i <= xe->xdf2.dend; i++) {
		u64 mph = xe->xdf2.minimal_perfect_hash.ptr[i];
		nm = occ->ptr[mph].file1;
		dis2.ptr[i] = (nm == 0) ? 0: (nm >= mlim) ? 2: 1;
	}

	for (i = xe->xdf1.dstart; i <= xe->xdf1.dend; i++) {
		if (dis1.ptr[i] == 1 ||
		    (dis1.ptr[i] == 2 && !xdl_clean_mmatch((char const *) dis1.ptr, i, xe->xdf1.dstart, xe->xdf1.dend))) {
			rust_ivec_push(&xe->xdf1.rindex, &i);
		} else
			xe->xdf1.rchg[i] = 1;
	}

	for (i = xe->xdf2.dstart; i <= xe->xdf2.dend; i++) {
		if (dis2.ptr[i] == 1 ||
		    (dis2.ptr[i] == 2 && !xdl_clean_mmatch((char const *) dis2.ptr, i, xe->xdf2.dstart, xe->xdf2.dend))) {
			rust_ivec_push(&xe->xdf2.rindex, &i);
		} else
			xe->xdf2.rchg[i] = 1;
	}

	rust_ivec_free(&dis1);
	rust_ivec_free(&dis2);

	return 0;
}


/*
 * Early trim initial and terminal matching records.
 */
static int xdl_trim_ends(xdfile_t *xdf1, xdfile_t *xdf2) {
	long i, lim;
	u64 *mph1, *mph2;

	mph1 = xdf1->minimal_perfect_hash.ptr;
	mph2 = xdf2->minimal_perfect_hash.ptr;
	for (i = 0, lim = XDL_MIN(xdf1->record.length, xdf2->record.length); i < lim;
	     i++, mph1++, mph2++)
		if (*mph1 != *mph2)
			break;

	xdf1->dstart = xdf2->dstart = i;

	mph1 = xdf1->minimal_perfect_hash.ptr + xdf1->minimal_perfect_hash.length - 1;
	mph2 = xdf2->minimal_perfect_hash.ptr + xdf2->minimal_perfect_hash.length - 1;
	for (lim -= i, i = 0; i < lim; i++, mph1--, mph2--)
		if (*mph1 != *mph2)
			break;

	xdf1->dend = xdf1->record.length - i - 1;
	xdf2->dend = xdf2->record.length - i - 1;

	return 0;
}


static int xdl_optimize_ctxs(xdfenv_t *xe, ivec_xdloccurrence_t *occ) {

	if (xdl_trim_ends(&xe->xdf1, &xe->xdf2) < 0 ||
	    xdl_cleanup_records(xe, occ) < 0) {

		return -1;
	}

	return 0;
}

#ifdef WITH_RUST
extern void rust_xdl_construct_mph_and_occurrences(xdfenv_t *xe, u64 flags, ivec_xdloccurrence_t *occurrence);
#else
#endif
static void c_xdl_construct_mph_and_occurrences(xdfenv_t *xe, u64 flags, ivec_xdloccurrence_t *occurrence) {
	struct xdl_minimal_perfect_hash_builder_t mphb;
	xdl_mphb_init(&mphb, xe->xdf1.record.length + xe->xdf2.record.length, flags);


	for (usize i = 0; i < xe->xdf1.record.length; i++) {
		u64 mph = xdl_mphb_hash(&mphb, &xe->xdf1.record.ptr[i]);
		xe->xdf1.minimal_perfect_hash.ptr[xe->xdf1.minimal_perfect_hash.length++] = mph;
	}

	for (usize i = 0; i < xe->xdf2.record.length; i++) {
		u64 mph = xdl_mphb_hash(&mphb, &xe->xdf2.record.ptr[i]);
		xe->xdf2.minimal_perfect_hash.ptr[xe->xdf2.minimal_perfect_hash.length++] = mph;
	}

	xe->minimal_perfect_hash_size = xdl_mphb_finish(&mphb);

	if (occurrence == NULL)
		return;

	/*
	 * ORDER MATTERS!!!, counting occurrences will only work properly if
	 * the records are iterated over in the same way that the mph set
	 * was constructed
	 */
	for (usize i = 0; i < xe->xdf1.minimal_perfect_hash.length; i++) {
		u64 mph = xe->xdf1.minimal_perfect_hash.ptr[i];
		if (mph == occurrence->length) {
			xdloccurrence_t occ;
			occ.file1 = 0;
			occ.file2 = 0;
			rust_ivec_push(occurrence, &occ);
		}
		occurrence->ptr[mph].file1 += 1;
	}

	for (usize i = 0; i < xe->xdf2.minimal_perfect_hash.length; i++) {
		u64 mph = xe->xdf2.minimal_perfect_hash.ptr[i];
		if (mph == occurrence->length) {
			xdloccurrence_t occ;
			occ.file1 = 0;
			occ.file2 = 0;
			rust_ivec_push(occurrence, &occ);
		}
		occurrence->ptr[mph].file2 += 1;
	}
}




#ifdef WITH_RUST
extern int rust_xdl_prepare_env(mmfile_t *mf1, mmfile_t *mf2, ivec_xdloccurrence_t *occ_ptr, u64 flags, xdfenv_t *xe);
int xdl_prepare_env(mmfile_t *mf1, mmfile_t *mf2, u64 flags, xdfenv_t *xe) {
	ivec_xdloccurrence_t occurrences;
	ivec_xdloccurrence_t *occ_ptr;
	IVEC_INIT(occurrences);

	if ((flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0) {
		occ_ptr = &occurrences;
	} else {
		occ_ptr = NULL;
	}

	xdfenv_t xe_alt;
	c_xdl_prepare_ctx(mf1, &xe_alt.xdf1, flags);
	c_xdl_prepare_ctx(mf2, &xe_alt.xdf2, flags);

	rust_xdl_prepare_ctx(mf1, &xe->xdf1, flags);
	rust_xdl_prepare_ctx(mf2, &xe->xdf2, flags);

	rust_env_equal(&xe_alt, xe);

	c_xdl_construct_mph_and_occurrences(&xe_alt, flags, occ_ptr);
	rust_xdl_construct_mph_and_occurrences(xe, flags, occ_ptr);

	rust_env_equal(xe, &xe_alt);

	if ((flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0) {
		xdl_optimize_ctxs(xe, &occurrences);
	}

	return 0;
}
#else
int xdl_prepare_env(mmfile_t *mf1, mmfile_t *mf2, u64 flags, xdfenv_t *xe) {
	ivec_xdloccurrence_t occurrences;
	ivec_xdloccurrence_t *occ_ptr;
	IVEC_INIT(occurrences);

	if ((flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0) {
		occ_ptr = &occurrences;
	} else {
		occ_ptr = NULL;
	}

	c_xdl_prepare_ctx(mf1, &xe->xdf1, flags);
	c_xdl_prepare_ctx(mf2, &xe->xdf2, flags);

	c_xdl_construct_mph_and_occurrences(xe, flags, occ_ptr);


	if ((flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0) {
		xdl_optimize_ctxs(xe, &occurrences);
	}

	return 0;
}
#endif
