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


extern i32 xdl_do_classic_diff(u64 flags, struct xdpair *pair);
extern i32 xdl_do_diff(xpparam_t const *xpp, struct xdpair *pair);
extern i32 xdl_change_compact(struct xd_file_context *ctx, struct xd_file_context *ctx_out, u64 flags);
extern i32 xdl_build_script(struct xdpair *pair, struct xdchange **xscr);
extern void xdl_free_script(struct xdchange *xscr);
int xdl_emit_diff(struct xdpair *pair, struct xdchange *xscr, struct xdemitcb *ecb,
		  struct xdemitconf const *xecfg);
extern int xdl_do_patience_diff(xpparam_t const *xpp, struct xdpair *pair);
extern i32 xdl_do_histogram_diff(u64 flags, struct xdpair *pair);

extern i32 xdl_call_hunk_func(struct xdpair *pair UNUSED, struct xdchange *xscr, struct xdemitcb *ecb,
				  struct xdemitconf const *xecfg);

#endif /* #if !defined(XDIFFI_H) */
