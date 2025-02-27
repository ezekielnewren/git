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

typedef struct {
	xrecord_t key;
	u64 value;
	usize next;
} xdlmphnode_t;

DEFINE_IVEC_TYPE(xdlmphnode_t, xdlmphnode_t);

typedef struct {
	ivec_usize head;
	ivec_xdlmphnode_t kv;
	u32 hbits;
	usize hsize;
	u64 count;
} xdlmph_t;


static void xdl_mph_init(xdlmph_t *mph, usize size) {
	usize default_value = INVALID_INDEX;

	IVEC_INIT(mph->head);
	IVEC_INIT(mph->kv);

	mph->hbits = xdl_hashbits(size);
	mph->hsize = 1 << mph->hbits;

	rust_ivec_resize_exact(&mph->head, mph->hsize, &default_value);
	rust_ivec_reserve(&mph->kv, size);

	mph->count = 0;
}


static void xdl_mph_free(xdlmph_t *mph) {
	rust_ivec_free(&mph->head);
	rust_ivec_free(&mph->kv);
}

static u64 xdl_mph_hash(xdlmph_t *mph, xrecord_t *key) {
	xdlmphnode_t *node;
	usize index;

	usize hi = (long) XDL_HASHLONG(key->line_hash, mph->hbits);
	for (index = mph->head.ptr[hi]; index != INVALID_INDEX;) {
		node = &mph->kv.ptr[index];
		if (node->key.line_hash == key->line_hash &&
				xdl_line_equal(node->key.ptr, node->key.size,
					key->ptr, key->size, key->flags))
			break;
		index = node->next;
	}

	if (index == INVALID_INDEX) {
		xdlmphnode_t node_new;
		index = mph->count;
		node_new.key = *key;
		node_new.value = mph->count++;
		node_new.next = mph->head.ptr[hi];
		mph->head.ptr[hi] = index;
		rust_ivec_push(&mph->kv, &node_new);
	}

	node = &mph->kv.ptr[index];

	return node->value;
}

static void xdl_count_occurrences(xdfenv_t *xe) {
	xdlmph_t mph;
	xdl_mph_init(&mph, xe->xdf1.record.length + xe->xdf2.record.length);

	for (usize i = 0; i < xe->xdf1.record.length; i++) {
		u64 minimal_perfect_hash;
		xrecord_t *rec = &xe->xdf1.record.ptr[i];
		minimal_perfect_hash = xdl_mph_hash(&mph, rec);
		if (minimal_perfect_hash == xe->occurrence.length) {
			xdloccurrence_t occ;
			occ.file1 = 0;
			occ.file2 = 0;
			rust_ivec_push(&xe->occurrence, &occ);
		}
		xe->occurrence.ptr[minimal_perfect_hash].file1 += 1;
		rust_ivec_push(&xe->xdf1.minimal_perfect_hash, &minimal_perfect_hash);
	}

	for (usize i = 0; i < xe->xdf2.record.length; i++) {
		u64 minimal_perfect_hash;
		xrecord_t *rec = &xe->xdf2.record.ptr[i];
		minimal_perfect_hash = xdl_mph_hash(&mph, rec);
		if (minimal_perfect_hash == xe->occurrence.length) {
			xdloccurrence_t occ;
			occ.file1 = 0;
			occ.file2 = 0;
			rust_ivec_push(&xe->occurrence, &occ);
		}
		xe->occurrence.ptr[minimal_perfect_hash].file2 += 1;
		rust_ivec_push(&xe->xdf2.minimal_perfect_hash, &minimal_perfect_hash);
	}

	xdl_mph_free(&mph);
}

static int xdl_prepare_ctx(mmfile_t *mf, xdfile_t *xdf, u64 flags) {
	usize no_eol, with_eol;
	u8 const* end = (u8 const*) (mf->ptr + mf->size);
	u8 default_value = 0;
	bool ignore = (flags & XDF_IGNORE_CR_AT_EOL) != 0;

	IVEC_INIT(xdf->record);
	IVEC_INIT(xdf->minimal_perfect_hash);
	IVEC_INIT(xdf->rindex);
	IVEC_INIT(xdf->rchg_vec);

	for (u8 const* cur = (u8 const*) mf->ptr; cur < end; cur += with_eol) {
		xrecord_t rec;
		xdl_line_length(cur, end, ignore, &no_eol, &with_eol);
		rec.ptr = cur;
		rec.size = with_eol;
		rec.line_hash = xdl_line_hash(cur, no_eol, flags);
		rec.flags = flags;
		rust_ivec_push(&xdf->record, &rec);
	}


	rust_ivec_resize_exact(&xdf->rchg_vec, xdf->record.length + 2, &default_value);

	if ((flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0) {
		rust_ivec_reserve_exact(&xdf->rindex, xdf->record.length + 1);
	}

	xdf->rchg = xdf->rchg_vec.ptr + 1;

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
	rust_ivec_free(&xe->occurrence);
}


static int xdl_clean_mmatch(ivec_u8 *dis, isize i, isize s, isize e) {
	isize r, rdis0, rpdis0, rdis1, rpdis1;

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
		if (!dis->ptr[i - r])
			rdis0++;
		else if (dis->ptr[i - r] == 2)
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
		if (!dis->ptr[i + r])
			rdis1++;
		else if (dis->ptr[i + r] == 2)
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
static void xdl_cleanup_records(xdfenv_t *xe) {
	isize i, nm, mlim1, mlim2;

	ivec_u8 dis1;
	ivec_u8 dis2;

	u8 default_value = NO;
	isize end1 = xe->xdf1.record.length - 1 - xe->delta_end;
	isize end2 = xe->xdf2.record.length - 1 - xe->delta_end;

	IVEC_INIT(dis1);
	rust_ivec_resize_exact(&dis1, xe->xdf1.rchg_vec.length, &default_value);

	IVEC_INIT(dis2);
	rust_ivec_resize_exact(&dis2, xe->xdf2.rchg_vec.length, &default_value);


	mlim1 = XDL_MIN(XDL_MAX_EQLIMIT, xdl_bogosqrt(xe->xdf1.record.length));
	for (i = xe->delta_start; i <= end1; i++) {
		u64 mph = xe->xdf1.minimal_perfect_hash.ptr[i];
		nm = xe->occurrence.ptr[mph].file1;
		dis1.ptr[i] = (nm == 0) ? 0: (nm >= mlim1) ? 2: 1;
	}

	mlim2 = XDL_MIN(XDL_MAX_EQLIMIT, xdl_bogosqrt(xe->xdf2.record.length));
	for (i = xe->delta_start; i <= end2; i++) {
		u64 mph = xe->xdf2.minimal_perfect_hash.ptr[i];
		nm = xe->occurrence.ptr[mph].file1;
		dis2.ptr[i] = (nm == 0) ? 0: (nm >= mlim2) ? 2: 1;
	}

	for (i = xe->delta_start; i <= end1; i++) {
		if (dis1.ptr[i] == 1 ||
		    (dis1.ptr[i] == 2 && !xdl_clean_mmatch(&dis1, i, xe->delta_start, end1))) {
			rust_ivec_push(&xe->xdf1.rindex, &i);
		} else
			xe->xdf1.rchg[i] = 1;
	}

	for (i = xe->delta_start; i <= end2; i++) {
		if (dis2.ptr[i] == 1 ||
		    (dis2.ptr[i] == 2 && !xdl_clean_mmatch(&dis2, i, xe->delta_start, end2))) {
			rust_ivec_push(&xe->xdf2.rindex, &i);
		} else
			xe->xdf2.rchg[i] = 1;
	}

	rust_ivec_free(&dis1);
	rust_ivec_free(&dis2);
}


/*
 * Early trim initial and terminal matching records.
 */
static void xdl_trim_ends(xdfenv_t *xe) {
	ivec_u64 *mph1 = &xe->xdf1.minimal_perfect_hash;
	ivec_u64 *mph2 = &xe->xdf2.minimal_perfect_hash;

	usize lim = XDL_MIN(mph1->length, mph2->length);
	for (isize i = 0; i < lim; i++) {
		if (mph1->ptr[i] != mph2->ptr[i]) {
			xe->delta_start = i;
			break;
		}
	}

	for (isize i = 0; i < lim; i++) {
		if (mph1->ptr[mph1->length - 1 - i] != mph2->ptr[mph2->length - 1 - i]) {
			xe->delta_end = i;
			break;
		}
	}
}



static void xdl_optimize_ctxs(xdfenv_t *xe) {
	xdl_trim_ends(xe);
	xdl_cleanup_records(xe);
}

#ifndef NO_RUST
extern i32 rust_xdl_prepare_env(mmfile_t *mf1, mmfile_t *mf2, u64 flags, xdfenv_t *xe);
i32 xdl_prepare_env(mmfile_t *mf1, mmfile_t *mf2, u64 flags, xdfenv_t *xe) {
	rust_xdl_prepare_env(mf1, mf2, flags, xe);

	// xdl_count_occurrences(xe);

	if ((flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0) {
		xdl_optimize_ctxs(xe);
	}

	return 0;
}
#else
i32 xdl_prepare_env(mmfile_t *mf1, mmfile_t *mf2, u64 flags, xdfenv_t *xe) {
	IVEC_INIT(xe->occurrence);
	xe->delta_start = 0;
	xe->delta_end = 0;

	if (xdl_prepare_ctx(mf1, &xe->xdf1, flags) < 0) {
		return -1;
	}

	if (xdl_prepare_ctx(mf2, &xe->xdf2, flags) < 0) {
		xdl_free_ctx(&xe->xdf1);
		return -1;
	}

	xdl_count_occurrences(xe);

	if ((flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0) {
		xdl_optimize_ctxs(xe);
	}

	return 0;
}
#endif
