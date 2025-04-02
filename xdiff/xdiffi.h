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

#if !defined(XDIFFI_H)
#define XDIFFI_H



struct xdalgoenv {
	isize mxcost;
	isize snake_cnt;
	isize heur_min;
};

struct xdchange {
	struct xdchange *next;
	isize i1, i2;
	isize chg1, chg2;
	bool ignore;
};



int xdl_recs_cmp(struct xd_file_context *ctx1, isize off1, isize lim1,
		 struct xd_file_context *ctx2, isize off2, isize lim2,
		 isize *kvdf, isize *kvdb, bool need_min, struct xdalgoenv *xenv);
int xdl_do_diff(xpparam_t const *xpp, struct xdpair *pair);
int xdl_change_compact(struct xd_file_context *ctx, struct xd_file_context *ctx_out, u64 flags);
int xdl_build_script(struct xdpair *pair, struct xdchange **xscr);
void xdl_free_script(struct xdchange *xscr);
int xdl_emit_diff(struct xdpair *pair, struct xdchange *xscr, xdemitcb_t *ecb,
		  xdemitconf_t const *xecfg);
int xdl_do_patience_diff(xpparam_t const *xpp, struct xdpair *pair);
int xdl_do_histogram_diff(xpparam_t const *xpp, struct xdpair *pair);

#endif /* #if !defined(XDIFFI_H) */
