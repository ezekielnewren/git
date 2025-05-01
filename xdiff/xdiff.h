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

#if !defined(XDIFF_H)
#define XDIFF_H

#ifdef __cplusplus
extern "C" {
#endif /* #ifdef __cplusplus */

#include "../rust/header/types.h"

#define INVALID_INDEX SIZE_MAX
#define LINE_SHIFT 1
#define SENTINEL 1

#define NO 0
#define YES 1
#define TOO_MANY 2

#define XDL_MIN(a, b) ((a) < (b) ? (a): (b))
#define XDL_MAX(a, b) ((a) > (b) ? (a): (b))
#define XDL_ISSPACE(c) (isspace((unsigned char)(c)))

/* xpparm_t.flags */
#define XDF_NEED_MINIMAL (1 << 0)

#define XDF_IGNORE_WHITESPACE (1 << 1)
#define XDF_IGNORE_WHITESPACE_CHANGE (1 << 2)
#define XDF_IGNORE_WHITESPACE_AT_EOL (1 << 3)
#define XDF_IGNORE_CR_AT_EOL (1 << 4)
#define XDF_WHITESPACE_FLAGS (XDF_IGNORE_WHITESPACE | \
			      XDF_IGNORE_WHITESPACE_CHANGE | \
			      XDF_IGNORE_WHITESPACE_AT_EOL | \
			      XDF_IGNORE_CR_AT_EOL)

#define XDF_IGNORE_BLANK_LINES (1 << 7)

#define XDF_PATIENCE_DIFF (1 << 14)
#define XDF_HISTOGRAM_DIFF (1 << 15)
#define XDF_DIFF_ALGORITHM_MASK (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)
#define XDF_DIFF_ALG(x) ((x) & XDF_DIFF_ALGORITHM_MASK)

#define XDF_INDENT_HEURISTIC (1 << 23)

/* xdemitconf_t.flags */
#define XDL_EMIT_FUNCNAMES (1 << 0)
#define XDL_EMIT_NO_HUNK_HDR (1 << 1)
#define XDL_EMIT_FUNCCONTEXT (1 << 2)

/* merge simplification levels */
#define XDL_MERGE_MINIMAL 0
#define XDL_MERGE_EAGER 1
#define XDL_MERGE_ZEALOUS 2
#define XDL_MERGE_ZEALOUS_ALNUM 3

/* merge favor modes */
#define XDL_MERGE_FAVOR_OURS 1
#define XDL_MERGE_FAVOR_THEIRS 2
#define XDL_MERGE_FAVOR_UNION 3

/* merge output styles */
#define XDL_MERGE_DIFF3 1
#define XDL_MERGE_ZEALOUS_DIFF3 2

typedef struct s_mmfile {
	char *ptr;
	long size;
} mmfile_t;

typedef struct s_mmbuffer {
	char *ptr;
	long size;
} mmbuffer_t;

typedef struct s_xpparam {
	u64 flags;

	/* -I<regex> */
	regex_t **ignore_regex;
	size_t ignore_regex_nr;

	/* See Documentation/diff-options.adoc. */
	char **anchors;
	size_t anchors_nr;
} xpparam_t;

struct xdemitcb {
	void *priv;
	i32 (*out_hunk)(void *,
			isize old_begin, isize old_nr,
			isize new_begin, isize new_nr,
			u8 const* func, isize funclen);
	i32 (*out_line)(void *, mmbuffer_t *, i32);
};

typedef isize (*find_func_t)(u8 const* line, isize line_len, u8* buffer, isize buffer_size, void *priv);

typedef i32 (*xdl_emit_hunk_consume_func_t)(isize start_a, isize count_a,
					    isize start_b, isize count_b,
					    void *cb_data);

struct xdemitconf {
	isize ctxlen;
	isize interhunkctxlen;
	u64 flags;
	find_func_t find_func;
	void *find_func_priv;
	xdl_emit_hunk_consume_func_t hunk_func;
};

typedef struct s_bdiffparam {
	long bsize;
} bdiffparam_t;


#define xdl_malloc(x) xmalloc(x)

void *xdl_mmfile_first(mmfile_t *mmf, long *size);
long xdl_mmfile_size(mmfile_t *mmf);

int xdl_diff(mmfile_t *mf1, mmfile_t *mf2, xpparam_t const *xpp,
	     struct xdemitconf const *xecfg, struct xdemitcb *ecb);

struct xmparam {
	xpparam_t xpp;
	i32 marker_size;
	i32 level;
	i32 favor;
	i32 style;
	u8 const* ancestor;	/* label for orig */
	u8 const* file1;	/* label for mf1 */
	u8 const* file2;	/* label for mf2 */
};

#define DEFAULT_CONFLICT_MARKER_SIZE 7

extern i32 xdl_merge(mmfile_t *orig, mmfile_t *mf1, mmfile_t *mf2,
		struct xmparam const *xmp, mmbuffer_t *result);

#ifdef __cplusplus
}
#endif /* #ifdef __cplusplus */

#endif /* #if !defined(XDIFF_H) */
