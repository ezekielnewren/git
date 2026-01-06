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
#include "compat/ivec.h"


typedef struct s_xdlclass {
	struct s_xdlclass *next;
	xrecord_t rec;
	size_t idx;
} xdlclass_t;

DEFINE_IVEC_TYPE(xdlclass_t, xdlclass);
DEFINE_IVEC_TYPE(xdlclass_t*, xdlclass_ptr);

typedef struct s_xdlclassifier {
	struct IVec_xdlclass node;
	struct IVec_xdlclass_ptr rchash;
	uint32_t hbits;
	size_t hsize;
	size_t count;
	uint64_t flags;
} xdlclassifier_t;


static void xdl_init_classifier(xdlclassifier_t *cf, size_t size, uint64_t flags) {
	memset(cf, 0, sizeof(xdlclassifier_t));
	IVEC_INIT(cf->node);
	IVEC_INIT(cf->rchash);

	cf->flags = flags;

	cf->hbits = xdl_hashbits((uint32_t) size + 1);
	cf->hsize = 1 << cf->hbits;

	ivec_reserve_exact(&cf->node, size);
	ivec_zero(&cf->rchash, cf->hsize);

	cf->count = 0;
}


static void xdl_free_classifier(xdlclassifier_t *cf) {
	ivec_free(&cf->node);
	ivec_free(&cf->rchash);
}


static int xdl_classify_record(xdlclassifier_t *cf, xrecord_t *rec) {
	size_t hi;
	xdlclass_t *rcrec;

	hi = XDL_HASHLONG(rec->line_hash, cf->hbits);
	for (rcrec = cf->rchash.ptr[hi]; rcrec; rcrec = rcrec->next)
		if (rcrec->rec.line_hash == rec->line_hash &&
				xdl_recmatch((const char *)rcrec->rec.ptr, (long)rcrec->rec.size,
					(const char *)rec->ptr, (long)rec->size, (long)cf->flags))
			break;

	if (!rcrec) {
		xdlclass_t *node = &cf->node.ptr[cf->node.length++];
		node->idx = cf->count++;
		node->rec = *rec;
		node->next = cf->rchash.ptr[hi];
		cf->rchash.ptr[hi] = node;
		rcrec = cf->rchash.ptr[hi];
	}

	rec->minimal_perfect_hash = (size_t)rcrec->idx;

	return 0;
}


static void xdl_free_ctx(xdfile_t *xdf)
{
	ivec_free(&xdf->changed);
	ivec_free(&xdf->record);
}


static int xdl_prepare_ctx(mmfile_t *mf, xdfile_t *xdf, uint64_t flags) {
	xrecord_t rec;
	long bsize;
	uint8_t const *blk, *cur, *top, *prev;

	IVEC_INIT(xdf->changed);
	IVEC_INIT(xdf->record);

	if ((cur = blk = xdl_mmfile_first(mf, &bsize))) {
		for (top = blk + bsize; cur < top; ) {
			prev = cur;
			rec.line_hash = xdl_hash_record(&cur, top, flags);
			rec.ptr = prev;
			rec.size = cur - prev;
			ivec_push(&xdf->record, rec);
		}
	}

	ivec_zero(&xdf->changed, xdf->record.length);

	return 0;
}


void xdl_free_env(xdfenv_t *xe) {

	xdl_free_ctx(&xe->xdf2);
	xdl_free_ctx(&xe->xdf1);
}


/*
 * Early trim initial and terminal matching records.
 */
static void xdl_trim_ends(xdfenv_t *xe)
{
	size_t lim = XDL_MIN(xe->xdf1.record.length, xe->xdf2.record.length);

	for (size_t i = 0; i < lim; i++) {
		size_t mph1 = xe->xdf1.record.ptr[i].minimal_perfect_hash;
		size_t mph2 = xe->xdf2.record.ptr[i].minimal_perfect_hash;
		if (mph1 != mph2) {
			xe->delta_start = (ssize_t)i;
			lim -= i;
			break;
		}
	}

	for (size_t i = 0; i < lim; i++) {
		size_t mph1 = xe->xdf1.record.ptr[xe->xdf1.record.length - 1 - i].minimal_perfect_hash;
		size_t mph2 = xe->xdf2.record.ptr[xe->xdf2.record.length - 1 - i].minimal_perfect_hash;
		if (mph1 != mph2) {
			xe->delta_end = i;
			break;
		}
	}
}


int xdl_prepare_env(mmfile_t *mf1, mmfile_t *mf2, xpparam_t const *xpp,
		    xdfenv_t *xe) {
	xdlclassifier_t cf;

	xe->delta_start = 0;
	xe->delta_end = 0;

	if (xdl_prepare_ctx(mf1, &xe->xdf1, xpp->flags) < 0) {

		return -1;
	}
	if (xdl_prepare_ctx(mf2, &xe->xdf2, xpp->flags) < 0) {

		xdl_free_ctx(&xe->xdf1);
		return -1;
	}
	xdl_init_classifier(&cf, xe->xdf1.record.length + xe->xdf2.record.length, xpp->flags);

	for (size_t i = 0; i < xe->xdf1.record.length; i++) {
		xrecord_t *rec = &xe->xdf1.record.ptr[i];
		xdl_classify_record(&cf, rec);
	}

	for (size_t i = 0; i < xe->xdf2.record.length; i++) {
		xrecord_t *rec = &xe->xdf2.record.ptr[i];
		xdl_classify_record(&cf, rec);
	}

	xe->mph_size = cf.count;
	xdl_free_classifier(&cf);

	xdl_trim_ends(xe);

	return 0;
}
