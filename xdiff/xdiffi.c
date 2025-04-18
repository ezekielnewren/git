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

#define XDL_MAX_COST_MIN 256
#define XDL_HEUR_MIN_COST 256
#define XDL_LINE_MAX (long)((1UL << (CHAR_BIT * sizeof(long) - 1)) - 1)
#define XDL_SNAKE_CNT 20
#define XDL_K_HEUR 4

struct xdpsplit {
	isize i1, i2;
	bool min_lo, min_hi;
};


extern struct xdchange *xdl_add_change(struct xdchange *xscr, isize i1, isize i2, isize chg1, isize chg2);


/*
 * If a line is indented more than this, get_indent() just returns this value.
 * This avoids having to do absurd amounts of work for data that are not
 * human-readable text, and also ensures that the output of get_indent fits
 * within an int.
 */
#define MAX_INDENT 200

extern isize get_indent(struct xrecord *rec);

/*
 * If more than this number of consecutive blank rows are found, just return
 * this value. This avoids requiring O(N^2) work for pathological cases, and
 * also ensures that the output of score_split fits in an int.
 */
#define MAX_BLANKS 20

/* Characteristics measured about a hypothetical split position. */
struct split_measurement {
	/*
	 * Is the split at the end of the file (aside from any blank lines)?
	 */
	bool end_of_file;

	/*
	 * How much is the line immediately following the split indented (or -1
	 * if the line is blank):
	 */
	isize indent;

	/*
	 * How many consecutive lines above the split are blank?
	 */
	isize pre_blank;

	/*
	 * How much is the nearest non-blank line above the split indented (or
	 * -1 if there is no such line)?
	 */
	isize pre_indent;

	/*
	 * How many lines after the line following the split are blank?
	 */
	isize post_blank;

	/*
	 * How much is the nearest non-blank line after the line following the
	 * split indented (or -1 if there is no such line)?
	 */
	isize post_indent;
};

struct split_score {
	/* The effective indent of this split (smaller is preferred). */
	isize effective_indent;

	/* Penalty for this split (smaller is preferred). */
	isize penalty;
};

static void measure_split(const struct xd_file_context *ctx, isize split, struct split_measurement *m);

/*
 * The empirically-determined weight factors used by score_split() below.
 * Larger values means that the position is a less favorable place to split.
 *
 * Note that scores are only ever compared against each other, so multiplying
 * all of these weight/penalty values by the same factor wouldn't change the
 * heuristic's behavior. Still, we need to set that arbitrary scale *somehow*.
 * In practice, these numbers are chosen to be large enough that they can be
 * adjusted relative to each other with sufficient precision despite using
 * integer math.
 */

/* Penalty if there are no non-blank lines before the split */
#define START_OF_FILE_PENALTY 1

/* Penalty if there are no non-blank lines after the split */
#define END_OF_FILE_PENALTY 21

/* Multiplier for the number of blank lines around the split */
#define TOTAL_BLANK_WEIGHT (-30)

/* Multiplier for the number of blank lines after the split */
#define POST_BLANK_WEIGHT 6

/*
 * Penalties applied if the line is indented more than its predecessor
 */
#define RELATIVE_INDENT_PENALTY (-4)
#define RELATIVE_INDENT_WITH_BLANK_PENALTY 10

/*
 * Penalties applied if the line is indented less than both its predecessor and
 * its successor
 */
#define RELATIVE_OUTDENT_PENALTY 24
#define RELATIVE_OUTDENT_WITH_BLANK_PENALTY 17

/*
 * Penalties applied if the line is indented less than its predecessor but not
 * less than its successor
 */
#define RELATIVE_DEDENT_PENALTY 23
#define RELATIVE_DEDENT_WITH_BLANK_PENALTY 17

/*
 * We only consider whether the sum of the effective indents for splits are
 * less than (-1), equal to (0), or greater than (+1) each other. The resulting
 * value is multiplied by the following weight and combined with the penalty to
 * determine the better of two scores.
 */
#define INDENT_WEIGHT 60

/*
 * How far do we slide a hunk at most?
 */
#define INDENT_HEURISTIC_MAX_SLIDING 100

extern void score_add_split(const struct split_measurement *m, struct split_score *s);

extern isize score_cmp(struct split_score *s1, struct split_score *s2);

/*
 * Represent a group of changed lines in an xdfile_t (i.e., a contiguous group
 * of lines that was inserted or deleted from the corresponding version of the
 * file). We consider there to be such a group at the beginning of the file, at
 * the end of the file, and between any two unchanged lines, though most such
 * groups will usually be empty.
 *
 * If the first line in a group is equal to the line following the group, then
 * the group can be slid down. Similarly, if the last line in a group is equal
 * to the line preceding the group, then the group can be slid up. See
 * group_slide_down() and group_slide_up().
 *
 * Note that loops that are testing for changed lines in xdf->rchg do not need
 * index bounding since the array is prepared with a zero at position -1 and N.
 */
struct xdlgroup {
	/*
	 * The index of the first changed line in the group, or the index of
	 * the unchanged line above which the (empty) group is located.
	 */
	isize start;

	/*
	 * The index of the first unchanged line after the group. For an empty
	 * group, end is equal to start.
	 */
	isize end;
};

extern void group_init(struct xd_file_context *ctx, struct xdlgroup *g);
extern int group_next(struct xd_file_context *ctx, struct xdlgroup *g);
extern int group_previous(struct xd_file_context *ctx, struct xdlgroup *g);
extern int group_slide_down(struct xd_file_context *ctx, struct xdlgroup *g);
extern int group_slide_up(struct xd_file_context *ctx, struct xdlgroup *g);


static int xdl_call_hunk_func(struct xdpair *pair UNUSED, struct xdchange *xscr, xdemitcb_t *ecb,
			      xdemitconf_t const *xecfg)
{
	struct xdchange *xch, *xche;

	for (xch = xscr; xch; xch = xche->next) {
		xche = xdl_get_hunk(&xch, xecfg);
		if (!xch)
			break;
		if (xecfg->hunk_func(xch->i1, xche->i1 + xche->chg1 - xch->i1,
				     xch->i2, xche->i2 + xche->chg2 - xch->i2,
				     ecb->priv) < 0)
			return -1;
	}
	return 0;
}

static void xdl_mark_ignorable_lines(struct xdchange *xscr, struct xdpair *pair, long flags)
{
	struct xdchange *xch;

	for (xch = xscr; xch; xch = xch->next) {
		bool ignore = true;
		struct xrecord *rec;
		long i;

		rec = &pair->lhs.record->ptr[xch->i1];
		for (i = 0; i < xch->chg1 && ignore; i++)
			ignore = xdl_blankline((const char*) rec[i].ptr, rec[i].size, flags);

		rec = &pair->rhs.record->ptr[xch->i2];
		for (i = 0; i < xch->chg2 && ignore; i++)
			ignore = xdl_blankline((const char*)rec[i].ptr, rec[i].size, flags);

		xch->ignore = ignore;
	}
}

static int record_matches_regex(struct xrecord *rec, xpparam_t const *xpp) {
	regmatch_t regmatch;
	size_t i;

	for (i = 0; i < xpp->ignore_regex_nr; i++)
		if (!regexec_buf(xpp->ignore_regex[i], (const char*) rec->ptr, rec->size, 1,
				 &regmatch, 0))
			return 1;

	return 0;
}

static void xdl_mark_ignorable_regex(struct xdchange *xscr, const struct xdpair *pair,
				     xpparam_t const *xpp)
{
	struct xdchange *xch;

	for (xch = xscr; xch; xch = xch->next) {
		struct xrecord *rec;
		int ignore = 1;
		long i;

		/*
		 * Do not override --ignore-blank-lines.
		 */
		if (xch->ignore)
			continue;

		rec = &pair->lhs.record->ptr[xch->i1];
		for (i = 0; i < xch->chg1 && ignore; i++)
			ignore = record_matches_regex(&rec[i], xpp);

		rec = &pair->rhs.record->ptr[xch->i2];
		for (i = 0; i < xch->chg2 && ignore; i++)
			ignore = record_matches_regex(&rec[i], xpp);

		xch->ignore = ignore;
	}
}

int xdl_diff(mmfile_t *mf1, mmfile_t *mf2, xpparam_t const *xpp,
	     xdemitconf_t const *xecfg, xdemitcb_t *ecb) {
	struct xdchange *xscr;
	struct xd2way two_way;
	emit_func_t ef = xecfg->hunk_func ? xdl_call_hunk_func : xdl_emit_diff;

	xdl_2way_prepare(mf1, mf2, xpp->flags, &two_way);

	if (xdl_do_diff(xpp, &two_way.pair) < 0) {

		return -1;
	}
	if (xdl_change_compact(&two_way.pair.lhs, &two_way.pair.rhs, xpp->flags) < 0 ||
	    xdl_change_compact(&two_way.pair.rhs, &two_way.pair.lhs, xpp->flags) < 0 ||
	    xdl_build_script(&two_way.pair, &xscr) < 0) {

		xdl_2way_free(&two_way);
		return -1;
	}
	if (xscr) {
		if (xpp->flags & XDF_IGNORE_BLANK_LINES)
			xdl_mark_ignorable_lines(xscr, &two_way.pair, xpp->flags);

		if (xpp->ignore_regex)
			xdl_mark_ignorable_regex(xscr, &two_way.pair, xpp);

		if (ef(&two_way.pair, xscr, ecb, xecfg) < 0) {

			xdl_free_script(xscr);
			xdl_2way_free(&two_way);
			return -1;
		}
		xdl_free_script(xscr);
	}
	xdl_2way_free(&two_way);

	return 0;
}
