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


static int xdl_append_merge(struct xdmerge **merge, int mode,
			    long i0, long chg0,
			    long i1, long chg1,
			    long i2, long chg2)
{
	struct xdmerge *m = *merge;
	if (m && (i1 <= m->i1 + m->chg1 || i2 <= m->i2 + m->chg2)) {
		if (mode != m->mode)
			m->mode = 0;
		m->chg0 = i0 + chg0 - m->i0;
		m->chg1 = i1 + chg1 - m->i1;
		m->chg2 = i2 + chg2 - m->i2;
	} else {
		m = xdl_malloc(sizeof(struct xdmerge));
		if (!m)
			return -1;
		m->next = NULL;
		m->mode = mode;
		m->i0 = i0;
		m->chg0 = chg0;
		m->i1 = i1;
		m->chg1 = chg1;
		m->i2 = i2;
		m->chg2 = chg2;
		if (*merge)
			(*merge)->next = m;
		*merge = m;
	}
	return 0;
}

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

static bool line_contains_alnum(u8 const* ptr, usize size) {
	while (size--) {
		if (isalnum(ptr++)) {
			return true;
		}
	}
	return false;
}

static bool lines_contain_alnum(struct xdpair *pair, usize i, usize chg) {
	for (; chg; chg--, i++) {
		if (line_contains_alnum(pair->rhs.record->ptr[i].ptr,
				pair->rhs.record->ptr[i].size)) {
			return true;
		}
	}
	return false;
}

/*
 * This function merges m and m->next, marking everything between those hunks
 * as conflicting, too.
 */
static void xdl_merge_two_conflicts(struct xdmerge *m)
{
	struct xdmerge *next_m = m->next;
	m->chg1 = next_m->i1 + next_m->chg1 - m->i1;
	m->chg2 = next_m->i2 + next_m->chg2 - m->i2;
	m->next = next_m->next;
	free(next_m);
}

/*
 * If there are less than 3 non-conflicting lines between conflicts,
 * it appears simpler -- because it takes up less (or as many) lines --
 * if the lines are moved into the conflicts.
 */
static int xdl_simplify_non_conflicts(struct xdpair *pair1, struct xdmerge *m,
				      int simplify_if_no_alnum)
{
	int result = 0;

	if (!m)
		return result;
	for (;;) {
		struct xdmerge *next_m = m->next;
		int begin, end;

		if (!next_m)
			return result;

		begin = m->i1 + m->chg1;
		end = next_m->i1;

		if (m->mode != 0 || next_m->mode != 0 ||
		    (end - begin > 3 &&
		     (!simplify_if_no_alnum ||
		      lines_contain_alnum(pair1, begin, end - begin)))) {
			m = next_m;
		} else {
			result++;
			xdl_merge_two_conflicts(m);
		}
	}
}

/*
 * level == 0: mark all overlapping changes as conflict
 * level == 1: mark overlapping changes as conflict only if not identical
 * level == 2: analyze non-identical changes for minimal conflict set
 * level == 3: analyze non-identical changes for minimal conflict set, but
 *             treat hunks not containing any letter or number as conflicting
 *
 * returns < 0 on error, == 0 for no conflicts, else number of conflicts
 */
static int xdl_do_merge(struct xd3way *three_way, struct xdchange *xscr1,
		struct xdchange *xscr2,
		xmparam_t const *xmp, struct ivec_u8* buffer)
{
	struct xdmerge *changes, *c;
	xpparam_t const *xpp = &xmp->xpp;
	const char *const ancestor_name = xmp->ancestor;
	const char *const name1 = xmp->file1;
	const char *const name2 = xmp->file2;
	int i0, i1, i2, chg0, chg1, chg2;
	int level = xmp->level;
	int style = xmp->style;
	int favor = xmp->favor;

	/*
	 * XDL_MERGE_DIFF3 does not attempt to refine conflicts by looking
	 * at common areas of sides 1 & 2, because the base (side 0) does
	 * not match and is being shown.  Similarly, simplification of
	 * non-conflicts is also skipped due to the skipping of conflict
	 * refinement.
	 *
	 * XDL_MERGE_ZEALOUS_DIFF3, on the other hand, will attempt to
	 * refine conflicts looking for common areas of sides 1 & 2.
	 * However, since the base is being shown and does not match,
	 * it will only look for common areas at the beginning or end
	 * of the conflict block.  Since XDL_MERGE_ZEALOUS_DIFF3's
	 * conflict refinement is much more limited in this fashion, the
	 * conflict simplification will be skipped.
	 */
	if (style == XDL_MERGE_DIFF3 || style == XDL_MERGE_ZEALOUS_DIFF3) {
		/*
		 * "diff3 -m" output does not make sense for anything
		 * more aggressive than XDL_MERGE_EAGER.
		 */
		if (XDL_MERGE_EAGER < level)
			level = XDL_MERGE_EAGER;
	}

	c = changes = NULL;

	while (xscr1 && xscr2) {
		if (!changes)
			changes = c;
		if (xscr1->i1 + xscr1->chg1 < xscr2->i1) {
			i0 = xscr1->i1;
			i1 = xscr1->i2;
			i2 = xscr2->i2 - xscr2->i1 + xscr1->i1;
			chg0 = xscr1->chg1;
			chg1 = xscr1->chg2;
			chg2 = xscr1->chg1;
			if (xdl_append_merge(&c, 1,
					     i0, chg0, i1, chg1, i2, chg2)) {
				xdl_cleanup_merge(changes);
				return -1;
			}
			xscr1 = xscr1->next;
			continue;
		}
		if (xscr2->i1 + xscr2->chg1 < xscr1->i1) {
			i0 = xscr2->i1;
			i1 = xscr1->i2 - xscr1->i1 + xscr2->i1;
			i2 = xscr2->i2;
			chg0 = xscr2->chg1;
			chg1 = xscr2->chg1;
			chg2 = xscr2->chg2;
			if (xdl_append_merge(&c, 2,
					     i0, chg0, i1, chg1, i2, chg2)) {
				xdl_cleanup_merge(changes);
				return -1;
			}
			xscr2 = xscr2->next;
			continue;
		}
		if (level == XDL_MERGE_MINIMAL || xscr1->i1 != xscr2->i1 ||
				xscr1->chg1 != xscr2->chg1 ||
				xscr1->chg2 != xscr2->chg2 ||
				!xdl_merge_lines_equal(three_way,
					xscr1->i2, xscr2->i2,
					xscr1->chg2)) {
			/* conflict */
			int off = xscr1->i1 - xscr2->i1;
			int ffo = off + xscr1->chg1 - xscr2->chg1;

			i0 = xscr1->i1;
			i1 = xscr1->i2;
			i2 = xscr2->i2;
			if (off > 0) {
				i0 -= off;
				i1 -= off;
			}
			else
				i2 += off;
			chg0 = xscr1->i1 + xscr1->chg1 - i0;
			chg1 = xscr1->i2 + xscr1->chg2 - i1;
			chg2 = xscr2->i2 + xscr2->chg2 - i2;
			if (ffo < 0) {
				chg0 -= ffo;
				chg1 -= ffo;
			} else
				chg2 += ffo;
			if (xdl_append_merge(&c, 0,
					     i0, chg0, i1, chg1, i2, chg2)) {
				xdl_cleanup_merge(changes);
				return -1;
			}
		}

		i1 = xscr1->i1 + xscr1->chg1;
		i2 = xscr2->i1 + xscr2->chg1;

		if (i1 >= i2)
			xscr2 = xscr2->next;
		if (i2 >= i1)
			xscr1 = xscr1->next;
	}
	while (xscr1) {
		if (!changes)
			changes = c;
		i0 = xscr1->i1;
		i1 = xscr1->i2;
		i2 = xscr1->i1 + three_way->side2.record.length - three_way->base.record.length;
		chg0 = xscr1->chg1;
		chg1 = xscr1->chg2;
		chg2 = xscr1->chg1;
		if (xdl_append_merge(&c, 1,
				     i0, chg0, i1, chg1, i2, chg2)) {
			xdl_cleanup_merge(changes);
			return -1;
		}
		xscr1 = xscr1->next;
	}
	while (xscr2) {
		if (!changes)
			changes = c;
		i0 = xscr2->i1;
		i1 = xscr2->i1 + three_way->side1.record.length - three_way->base.record.length;
		i2 = xscr2->i2;
		chg0 = xscr2->chg1;
		chg1 = xscr2->chg1;
		chg2 = xscr2->chg2;
		if (xdl_append_merge(&c, 2,
				     i0, chg0, i1, chg1, i2, chg2)) {
			xdl_cleanup_merge(changes);
			return -1;
		}
		xscr2 = xscr2->next;
	}
	if (!changes)
		changes = c;
	/* refine conflicts */
	if (style == XDL_MERGE_ZEALOUS_DIFF3) {
		xdl_refine_zdiff3_conflicts(three_way, changes);
	} else if (XDL_MERGE_ZEALOUS <= level &&
		   (xdl_refine_conflicts(three_way, changes, xpp) < 0 ||
		    xdl_simplify_non_conflicts(&three_way->pair1, changes,
					       XDL_MERGE_ZEALOUS < level) < 0)) {
		xdl_cleanup_merge(changes);
		return -1;
	}
	/* output */
	int marker_size = xmp->marker_size;
	xdl_fill_merge_buffer(three_way, name1, name2,
			      ancestor_name, favor, changes,
			      buffer, style, marker_size);
	return xdl_cleanup_merge(changes);
}

int xdl_merge(mmfile_t *orig, mmfile_t *mf1, mmfile_t *mf2,
		xmparam_t const *xmp, mmbuffer_t *result)
{
	struct xdchange *xscr1 = NULL, *xscr2 = NULL;
	struct xd3way three_way;
	struct ivec_u8 buffer;
	int status = -1;
	xpparam_t const *xpp = &xmp->xpp;

	IVEC_INIT(buffer);
	result->ptr = NULL;
	result->size = 0;

	xdl_3way_prepare(orig, mf1, mf2, xpp->flags, &three_way);

	if (xdl_do_diff(xpp, &three_way.pair1) < 0)
		return -1;

	if (xdl_do_diff(xpp, &three_way.pair2) < 0)
		goto out; /* avoid double free of xe2 */

	if (xdl_change_compact(&three_way.pair1.lhs, &three_way.pair1.rhs, xpp->flags) < 0 ||
	    xdl_change_compact(&three_way.pair1.rhs, &three_way.pair1.lhs, xpp->flags) < 0 ||
	    xdl_build_script(&three_way.pair1, &xscr1) < 0)
		goto out;

	if (xdl_change_compact(&three_way.pair2.lhs, &three_way.pair2.rhs, xpp->flags) < 0 ||
	    xdl_change_compact(&three_way.pair2.rhs, &three_way.pair2.lhs, xpp->flags) < 0 ||
	    xdl_build_script(&three_way.pair2, &xscr2) < 0)
		goto out;

	if (!xscr1) {
		status = 0;
		ivec_extend_from_slice(&buffer, mf2->ptr, mf2->size);
	} else if (!xscr2) {
		status = 0;
		ivec_extend_from_slice(&buffer, mf1->ptr, mf1->size);
	} else {
		status = xdl_do_merge(&three_way, xscr1, xscr2, xmp, &buffer);
	}
	ivec_shrink_to_fit(&buffer);
	result->ptr = (char*) buffer.ptr;
	result->size = (long) buffer.length;
	memset(&buffer, 0, sizeof(buffer));
 out:
	xdl_free_script(xscr1);
	xdl_free_script(xscr2);
	xdl_3way_free(&three_way);

	return status;
}
