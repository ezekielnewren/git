/*
 *  LibXDiff by Davide Libenzi ( File Differential Library )
 *  Copyright (C) 2003-2006 Davide Libenzi, Johannes E. Schindelin
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

struct xdmerge {
	struct xdmerge *next;
	/*
	 * 0 = conflict,
	 * 1 = no conflict, take first,
	 * 2 = no conflict, take second.
	 * 3 = no conflict, take both.
	 */
	u8 mode;
	/*
	 * These point at the respective postimages.  E.g. <i1,chg1> is
	 * how side #1 wants to change the common ancestor; if there is no
	 * overlap, lines before i1 in the postimage of side #1 appear
	 * in the merge result as a region touched by neither side.
	 */
	usize i1, i2;
	usize chg1, chg2;
	/*
	 * These point at the preimage; of course there is just one
	 * preimage, that is from the shared common ancestor.
	 */
	usize i0;
	usize chg0;
};


extern i32 xdl_append_merge(struct xdmerge **merge, u8 mode,
			    usize i0, usize chg0,
			    usize i1, usize chg1,
			    usize i2, usize chg2);

extern usize xdl_cleanup_merge(struct xdmerge *c);

extern bool xdl_merge_lines_equal(struct xd3way *three_way, usize i1, usize i2, usize line_count);

extern void xdl_recs_copy(struct ivec_xrecord *record, usize off, usize count, bool needs_cr, bool add_nl, struct ivec_u8* dest);

extern i32 is_eol_crlf(struct ivec_xrecord *record, usize i);

extern bool is_cr_needed(struct xd3way *three_way, struct xdmerge *m);

extern void fill_conflict_hunk(struct xd3way *three_way,
			      u8 const* name1,
			      u8 const* name2,
			      u8 const* name3,
			      usize i, u64 style,
			      struct xdmerge *m, struct ivec_u8* buffer, usize marker_size);

extern void xdl_fill_merge_buffer(struct xd3way *three_way,
				 u8 const* name1,
				 u8 const* name2,
				 u8 const* ancestor_name,
				 u8 favor,
				 struct xdmerge *m, struct ivec_u8* buffer, u64 style,
				 usize marker_size);

extern void xdl_refine_zdiff3_conflicts(struct xd3way *three_way, struct xdmerge *m);

extern int xdl_refine_conflicts(struct xd3way *three_way, struct xdmerge *m, xpparam_t const *xpp);

extern bool lines_contain_alnum(struct xdpair *pair, usize i, usize chg);

extern void xdl_merge_two_conflicts(struct xdmerge *m);

extern i32 xdl_simplify_non_conflicts(struct xdpair *pair1, struct xdmerge *m,
				      bool simplify_if_no_alnum);

extern i32 xdl_do_merge(struct xd3way *three_way, struct xdchange *xscr1,
		struct xdchange *xscr2,
		struct xmparam const *xmp, struct ivec_u8* buffer);
