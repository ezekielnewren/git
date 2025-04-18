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


static struct xdchange *xdl_add_change(struct xdchange *xscr, isize i1, isize i2, isize chg1, isize chg2) {
	struct xdchange *xch;

	if (!(xch = (struct xdchange *) xdl_malloc(sizeof(struct xdchange))))
		return NULL;

	xch->next = xscr;
	xch->i1 = i1;
	xch->i2 = i2;
	xch->chg1 = chg1;
	xch->chg2 = chg2;
	xch->ignore = false;

	return xch;
}


/*
 * If a line is indented more than this, get_indent() just returns this value.
 * This avoids having to do absurd amounts of work for data that are not
 * human-readable text, and also ensures that the output of get_indent fits
 * within an int.
 */
#define MAX_INDENT 200

/*
 * Return the amount of indentation of the specified line, treating TAB as 8
 * columns. Return -1 if line is empty or contains only whitespace. Clamp the
 * output value at MAX_INDENT.
 */
static int get_indent(struct xrecord *rec)
{
	long i;
	int ret = 0;

	for (i = 0; i < (isize) rec->size; i++) {
		char c = rec->ptr[i];

		if (!XDL_ISSPACE(c))
			return ret;
		else if (c == ' ')
			ret += 1;
		else if (c == '\t')
			ret += 8 - ret % 8;
		/* ignore other whitespace characters */

		if (ret >= MAX_INDENT)
			return MAX_INDENT;
	}

	/* The line contains only whitespace. */
	return -1;
}

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
	int end_of_file;

	/*
	 * How much is the line immediately following the split indented (or -1
	 * if the line is blank):
	 */
	int indent;

	/*
	 * How many consecutive lines above the split are blank?
	 */
	int pre_blank;

	/*
	 * How much is the nearest non-blank line above the split indented (or
	 * -1 if there is no such line)?
	 */
	int pre_indent;

	/*
	 * How many lines after the line following the split are blank?
	 */
	int post_blank;

	/*
	 * How much is the nearest non-blank line after the line following the
	 * split indented (or -1 if there is no such line)?
	 */
	int post_indent;
};

struct split_score {
	/* The effective indent of this split (smaller is preferred). */
	int effective_indent;

	/* Penalty for this split (smaller is preferred). */
	int penalty;
};

/*
 * Fill m with information about a hypothetical split of xdf above line split.
 */
static void measure_split(const struct xd_file_context *ctx, isize split,
			  struct split_measurement *m)
{
	isize i;

	if (split >= (isize) ctx->record->length) {
		m->end_of_file = 1;
		m->indent = -1;
	} else {
		m->end_of_file = 0;
		m->indent = get_indent(&ctx->record->ptr[split]);
	}

	m->pre_blank = 0;
	m->pre_indent = -1;
	for (i = split - 1; i >= 0; i--) {
		m->pre_indent = get_indent(&ctx->record->ptr[i]);
		if (m->pre_indent != -1)
			break;
		m->pre_blank += 1;
		if (m->pre_blank == MAX_BLANKS) {
			m->pre_indent = 0;
			break;
		}
	}

	m->post_blank = 0;
	m->post_indent = -1;
	for (i = split + 1; i < (isize) ctx->record->length; i++) {
		m->post_indent = get_indent(&ctx->record->ptr[i]);
		if (m->post_indent != -1)
			break;
		m->post_blank += 1;
		if (m->post_blank == MAX_BLANKS) {
			m->post_indent = 0;
			break;
		}
	}
}

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

/*
 * Compute a badness score for the hypothetical split whose measurements are
 * stored in m. The weight factors were determined empirically using the tools
 * and corpus described in
 *
 *     https://github.com/mhagger/diff-slider-tools
 *
 * Also see that project if you want to improve the weights based on, for
 * example, a larger or more diverse corpus.
 */
static void score_add_split(const struct split_measurement *m, struct split_score *s)
{
	/*
	 * A place to accumulate penalty factors (positive makes this index more
	 * favored):
	 */
	int post_blank, total_blank, indent, any_blanks;

	if (m->pre_indent == -1 && m->pre_blank == 0)
		s->penalty += START_OF_FILE_PENALTY;

	if (m->end_of_file)
		s->penalty += END_OF_FILE_PENALTY;

	/*
	 * Set post_blank to the number of blank lines following the split,
	 * including the line immediately after the split:
	 */
	post_blank = (m->indent == -1) ? 1 + m->post_blank : 0;
	total_blank = m->pre_blank + post_blank;

	/* Penalties based on nearby blank lines: */
	s->penalty += TOTAL_BLANK_WEIGHT * total_blank;
	s->penalty += POST_BLANK_WEIGHT * post_blank;

	if (m->indent != -1)
		indent = m->indent;
	else
		indent = m->post_indent;

	any_blanks = (total_blank != 0);

	/* Note that the effective indent is -1 at the end of the file: */
	s->effective_indent += indent;

	if (indent == -1) {
		/* No additional adjustments needed. */
	} else if (m->pre_indent == -1) {
		/* No additional adjustments needed. */
	} else if (indent > m->pre_indent) {
		/*
		 * The line is indented more than its predecessor.
		 */
		s->penalty += any_blanks ?
			RELATIVE_INDENT_WITH_BLANK_PENALTY :
			RELATIVE_INDENT_PENALTY;
	} else if (indent == m->pre_indent) {
		/*
		 * The line has the same indentation level as its predecessor.
		 * No additional adjustments needed.
		 */
	} else {
		/*
		 * The line is indented less than its predecessor. It could be
		 * the block terminator of the previous block, but it could
		 * also be the start of a new block (e.g., an "else" block, or
		 * maybe the previous block didn't have a block terminator).
		 * Try to distinguish those cases based on what comes next:
		 */
		if (m->post_indent != -1 && m->post_indent > indent) {
			/*
			 * The following line is indented more. So it is likely
			 * that this line is the start of a block.
			 */
			s->penalty += any_blanks ?
				RELATIVE_OUTDENT_WITH_BLANK_PENALTY :
				RELATIVE_OUTDENT_PENALTY;
		} else {
			/*
			 * That was probably the end of a block.
			 */
			s->penalty += any_blanks ?
				RELATIVE_DEDENT_WITH_BLANK_PENALTY :
				RELATIVE_DEDENT_PENALTY;
		}
	}
}

static int score_cmp(struct split_score *s1, struct split_score *s2)
{
	/* -1 if s1.effective_indent < s2->effective_indent, etc. */
	int cmp_indents = ((s1->effective_indent > s2->effective_indent) -
			   (s1->effective_indent < s2->effective_indent));

	return INDENT_WEIGHT * cmp_indents + (s1->penalty - s2->penalty);
}

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

/*
 * Move back and forward change groups for a consistent and pretty diff output.
 * This also helps in finding joinable change groups and reducing the diff
 * size.
 */
i32 xdl_change_compact(struct xd_file_context *ctx, struct xd_file_context *ctx_out, u64 flags) {
	struct xdlgroup g, go;
	isize earliest_end, end_matching_other;
	isize groupsize;

	group_init(ctx, &g);
	group_init(ctx_out, &go);

	while (1) {
		/*
		 * If the group is empty in the to-be-compacted file, skip it:
		 */
		if (g.end != g.start) {
			/*
			 * Now shift the change up and then down as far as possible in
			 * each direction. If it bumps into any other changes, merge
			 * them.
			 */
			while (1) {
				groupsize = g.end - g.start;

				/*
				 * Keep track of the last "end" index that causes this
				 * group to align with a group of changed lines in the
				 * other file. -1 indicates that we haven't found such
				 * a match yet:
				 */
				end_matching_other = -1;

				/* Shift the group backward as much as possible: */
				while (!group_slide_up(ctx, &g))
					if (group_previous(ctx_out, &go))
						BUG("group sync broken sliding up");

				/*
				 * This is this highest that this group can be shifted.
				 * Record its end index:
				 */
				earliest_end = g.end;

				if (go.end > go.start)
					end_matching_other = g.end;

				/* Now shift the group forward as far as possible: */
				while (1) {
					if (group_slide_down(ctx, &g))
						break;
					if (group_next(ctx_out, &go))
						BUG("group sync broken sliding down");

					if (go.end > go.start)
						end_matching_other = g.end;
				}

				if (groupsize == g.end - g.start) {
					break;
				}
			}

			/*
			 * If the group can be shifted, then we can possibly use this
			 * freedom to produce a more intuitive diff.
			 *
			 * The group is currently shifted as far down as possible, so
			 * the heuristics below only have to handle upwards shifts.
			 */

			if (g.end == earliest_end) {
				/* no shifting was possible */
			} else if (end_matching_other != -1) {
				/*
				 * Move the possibly merged group of changes back to
				 * line up with the last group of changes from the
				 * other file that it can align with.
				 */
				while (go.end == go.start) {
					if (group_slide_up(ctx, &g))
						BUG("match disappeared");
					if (group_previous(ctx_out, &go))
						BUG("group sync broken sliding to match");
				}
			} else if (flags & XDF_INDENT_HEURISTIC) {
				/*
				 * Indent heuristic: a group of pure add/delete lines
				 * implies two splits, one between the end of the
				 * "before" context and the start of the group, and
				 * another between the end of the group and the
				 * beginning of the "after" context. Some splits are
				 * aesthetically better and some are worse. We compute
				 * a badness "score" for each split, and add the scores
				 * for the two splits to define a "score" for each
				 * position that the group can be shifted to. Then we
				 * pick the shift with the lowest score.
				 */
				isize shift, best_shift = -1;
				struct split_score best_score;

				shift = earliest_end;
				if (g.end - groupsize - 1 > shift)
					shift = g.end - groupsize - 1;
				if (g.end - INDENT_HEURISTIC_MAX_SLIDING > shift)
					shift = g.end - INDENT_HEURISTIC_MAX_SLIDING;
				for (; shift <= g.end; shift++) {
					struct split_measurement m;
					struct split_score score = {0, 0};

					measure_split(ctx, shift, &m);
					score_add_split(&m, &score);
					measure_split(ctx, shift - groupsize, &m);
					score_add_split(&m, &score);
					if (best_shift == -1 ||
					    score_cmp(&score, &best_score) <= 0) {
						best_score.effective_indent = score.effective_indent;
						best_score.penalty = score.penalty;
						best_shift = shift;
					    }
				}

				while (g.end > best_shift) {
					if (group_slide_up(ctx, &g))
						BUG("best shift unreached");
					if (group_previous(ctx_out, &go))
						BUG("group sync broken sliding to blank line");
				}
			}
		}

		/* Move past the just-processed group: */
		if (group_next(ctx, &g))
			break;
		if (group_next(ctx_out, &go))
			BUG("group sync broken moving to next group");
	}

	if (!group_next(ctx_out, &go))
		BUG("group sync broken at end of file");

	return 0;
}


int xdl_build_script(struct xdpair *pair, struct xdchange **xscr) {
	struct xdchange *cscr = NULL, *xch;
	long i1, i2, l1, l2;

	/*
	 * Trivial. Collects "groups" of changes and creates an edit script.
	 */
	for (i1 = pair->lhs.record->length, i2 = pair->rhs.record->length; i1 >= 0 || i2 >= 0; i1--, i2--)
		if (pair->lhs.consider.ptr[SENTINEL + i1 - 1] || pair->rhs.consider.ptr[SENTINEL + i2 - 1]) {
			for (l1 = i1; pair->lhs.consider.ptr[SENTINEL + i1 - 1]; i1--);
			for (l2 = i2; pair->rhs.consider.ptr[SENTINEL + i2 - 1]; i2--);

			if (!(xch = xdl_add_change(cscr, i1, i2, l1 - i1, l2 - i2))) {
				xdl_free_script(cscr);
				return -1;
			}
			cscr = xch;
		}

	*xscr = cscr;

	return 0;
}


void xdl_free_script(struct xdchange *xscr) {
	struct xdchange *xch;

	while ((xch = xscr) != NULL) {
		xscr = xscr->next;
		free(xch);
	}
}

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
