/*
 *  LibXDiff by Davide Libenzi ( File Differential Library )
 *  Copyright (C) 2003	Davide Libenzi
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


static int xdl_emit_record(struct xd_file_context *ctx, long ri, char const *pre, struct xdemitcb *ecb) {
	struct xrecord *record = &ctx->record->ptr[ri];
	long psize = strlen(pre);

	if (xdl_emit_diffrec(record->ptr, record->size, pre, psize, ecb) < 0) {
		return -1;
	}

	return 0;
}




static isize def_ff(u8 const* rec, isize len, u8* buf, isize sz) {
	if (len > 0 &&
			(isalpha((unsigned char)*rec) || /* identifier? */
			 *rec == '_' || /* also identifier? */
			 *rec == '$')) { /* identifiers from VMS and other esoterico */
		if (len > sz)
			len = sz;
		while (0 < len && isspace((unsigned char)rec[len - 1]))
			len--;
		memcpy(buf, rec, len);
		return len;
	}
	return -1;
}

static long match_func_rec(struct xd_file_context *ctx, struct xdemitconf const *xecfg, long ri,
			   char *buf, long sz)
{
	struct xrecord *record = &ctx->record->ptr[ri];
	if (!xecfg->find_func)
		return def_ff(record->ptr, record->size, buf, sz);
	return xecfg->find_func(record->ptr, record->size, buf, sz, xecfg->find_func_priv);
}

static int is_func_rec(struct xd_file_context *ctx, struct xdemitconf const *xecfg, long ri)
{
	char dummy[1];
	return match_func_rec(ctx, xecfg, ri, dummy, sizeof(dummy)) >= 0;
}

struct func_line {
	long len;
	char buf[80];
};

static long get_func_line(struct xdpair *pair, struct xdemitconf const *xecfg,
			  struct func_line *func_line, long start, long limit)
{
	long l, size, step = (start > limit) ? -1 : 1;
	char *buf, dummy[1];

	buf = func_line ? func_line->buf : dummy;
	size = func_line ? sizeof(func_line->buf) : sizeof(dummy);

	for (l = start; l != limit && 0 <= l && l < (isize) pair->lhs.record->length; l += step) {
		long len = match_func_rec(&pair->lhs, xecfg, l, buf, size);
		if (len >= 0) {
			if (func_line)
				func_line->len = len;
			return l;
		}
	}
	return -1;
}

static bool is_empty_rec(struct xd_file_context *ctx, long ri) {
	struct xrecord *record = &ctx->record->ptr[ri];
	for (usize i = 0; i < record->size; i++) {
		if (!XDL_ISSPACE(record->ptr[i])) {
			return false;
		}
	}
	return true;
}

i32 xdl_emit_diff(struct xdpair *pair, struct xdchange *xscr, struct xdemitcb *ecb,
		  struct xdemitconf const *xecfg) {
	long s1, s2, e1, e2, lctx;
	struct xdchange *xch, *xche;
	long funclineprev = -1;
	struct func_line func_line = { 0 };

	for (xch = xscr; xch; xch = xche->next) {
		struct xdchange *xchp = xch;
		xche = xdl_get_hunk(&xch, xecfg);
		if (!xch)
			break;

		while (true) {
			s1 = XDL_MAX(xch->i1 - xecfg->ctxlen, 0);
			s2 = XDL_MAX(xch->i2 - xecfg->ctxlen, 0);

			if (xecfg->flags & XDL_EMIT_FUNCCONTEXT) {
				long fs1, i1 = xch->i1;

				/* Appended chunk? */
				if (i1 >= (isize) pair->lhs.record->length) {
					long i2 = xch->i2;

					/*
					 * We don't need additional context if
					 * a whole function was added.
					 */
					bool should_break = false;
					while (i2 < (isize) pair->rhs.record->length) {
						if (is_func_rec(&pair->rhs, xecfg, i2)) {
							should_break = true;
							break;
						}
						i2++;
					}
					if (should_break) {
						break;
					}

					/*
					 * Otherwise get more context from the
					 * pre-image.
					 */
					i1 = pair->lhs.record->length - 1;
				}

				fs1 = get_func_line(pair, xecfg, NULL, i1, -1);
				while (fs1 > 0 && !is_empty_rec(&pair->lhs, fs1 - 1) &&
				       !is_func_rec(&pair->lhs, xecfg, fs1 - 1))
					fs1--;
				if (fs1 < 0)
					fs1 = 0;
				if (fs1 < s1) {
					s2 = XDL_MAX(s2 - (s1 - fs1), 0);
					s1 = fs1;

					/*
					 * Did we extend context upwards into an
					 * ignored change?
					 */
					while (xchp != xch &&
					       xchp->i1 + xchp->chg1 <= s1 &&
					       xchp->i2 + xchp->chg2 <= s2)
						xchp = xchp->next;

					/* If so, show it after all. */
					if (xchp != xch) {
						xch = xchp;
						continue;
					}
				}
			}
			break;
		}

		while (true) {
			lctx = xecfg->ctxlen;
			lctx = XDL_MIN(lctx, (isize) pair->lhs.record->length - (xche->i1 + xche->chg1));
			lctx = XDL_MIN(lctx, (isize) pair->rhs.record->length - (xche->i2 + xche->chg2));

			e1 = xche->i1 + xche->chg1 + lctx;
			e2 = xche->i2 + xche->chg2 + lctx;

			if (xecfg->flags & XDL_EMIT_FUNCCONTEXT) {
				long fe1 = get_func_line(pair, xecfg, NULL,
							 xche->i1 + xche->chg1,
							 pair->lhs.record->length);
				while (fe1 > 0 && is_empty_rec(&pair->lhs, fe1 - 1))
					fe1--;
				if (fe1 < 0)
					fe1 = pair->lhs.record->length;
				if (fe1 > e1) {
					e2 = XDL_MIN(e2 + (fe1 - e1), (isize) pair->rhs.record->length);
					e1 = fe1;
				}

				/*
				 * Overlap with next change?  Then include it
				 * in the current hunk and start over to find
				 * its new end.
				 */
				if (xche->next) {
					long l = XDL_MIN(xche->next->i1,
							 (isize) pair->lhs.record->length - 1);
					if (l - xecfg->ctxlen <= e1 ||
					    get_func_line(pair, xecfg, NULL, l, e1) < 0) {
						xche = xche->next;
						continue;
					}
				}
			}
			break;
		}

		/*
		 * Emit current hunk header.
		 */

		if (xecfg->flags & XDL_EMIT_FUNCNAMES) {
			get_func_line(pair, xecfg, &func_line,
				      s1 - 1, funclineprev);
			funclineprev = s1 - 1;
		}
		if (!(xecfg->flags & XDL_EMIT_NO_HUNK_HDR) &&
		    xdl_emit_hunk_hdr(s1 + 1, e1 - s1, s2 + 1, e2 - s2,
				      func_line.buf, func_line.len, ecb) < 0)
			return -1;

		/*
		 * Emit pre-context.
		 */
		for (; s2 < xch->i2; s2++)
			if (xdl_emit_record(&pair->rhs, s2, " ", ecb) < 0)
				return -1;

		for (s1 = xch->i1, s2 = xch->i2;; xch = xch->next) {
			/*
			 * Merge previous with current change atom.
			 */
			for (; s1 < xch->i1 && s2 < xch->i2; s1++, s2++)
				if (xdl_emit_record(&pair->rhs, s2, " ", ecb) < 0)
					return -1;

			/*
			 * Removes lines from the first file.
			 */
			for (s1 = xch->i1; s1 < xch->i1 + xch->chg1; s1++)
				if (xdl_emit_record(&pair->lhs, s1, "-", ecb) < 0)
					return -1;

			/*
			 * Adds lines from the second file.
			 */
			for (s2 = xch->i2; s2 < xch->i2 + xch->chg2; s2++)
				if (xdl_emit_record(&pair->rhs, s2, "+", ecb) < 0)
					return -1;

			if (xch == xche)
				break;
			s1 = xch->i1 + xch->chg1;
			s2 = xch->i2 + xch->chg2;
		}

		/*
		 * Emit post-context.
		 */
		for (s2 = xche->i2 + xche->chg2; s2 < e2; s2++)
			if (xdl_emit_record(&pair->rhs, s2, " ", ecb) < 0)
				return -1;
	}

	return 0;
}
