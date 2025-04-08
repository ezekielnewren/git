/*
 *  LibXDiff by Davide Libenzi ( File Differential Library )
 *  Copyright (C) 2003-2016 Davide Libenzi, Johannes E. Schindelin
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

/*
 * The basic idea of patience diff is to find lines that are unique in
 * both files.  These are intuitively the ones that we want to see as
 * common lines.
 *
 * The maximal ordered sequence of such line pairs (where ordered means
 * that the order in the sequence agrees with the order of the lines in
 * both files) naturally defines an initial set of common lines.
 *
 * Now, the algorithm tries to extend the set of common lines by growing
 * the line ranges where the files have identical lines.
 *
 * Between those common lines, the patience diff algorithm is applied
 * recursively, until no unique line pairs can be found; these line ranges
 * are handled by the well-known Myers algorithm.
 */

#define NON_UNIQUE ULONG_MAX

struct entry {
	u64 minimal_perfect_hash;
	/*
	 * 0 = unused entry, 1 = first line, 2 = second, etc.
	 * line2 is NON_UNIQUE if the line is not unique
	 * in either the first or the second file.
	 */
	usize line1, line2;
	/*
	 * "next" & "previous" are used for the longest common
	 * sequence;
	 * initially, "next" reflects only the order in file1.
	 */
	struct entry *next, *previous;

	/*
	 * If 1, this entry can serve as an anchor. See
	 * Documentation/diff-options.adoc for more information.
	 */
	bool anchor : true;
};

DEFINE_IVEC_TYPE(struct entry, entry);

/*
 * This is a hash mapping from line hash to line numbers in the first and
 * second file.
 */
struct hashmap {
	usize nr;
	struct ivec_entry entries;
	struct entry *first, *last;
	/* were common records found? */
	bool has_matches;
};

extern bool is_anchor(xpparam_t const *xpp, u8 const *line);

/* The argument "pass" is 1 for the first file, 2 for the second. */
extern void insert_record(xpparam_t const *xpp, struct xdpair *pair,
	usize line, struct hashmap *map, i32 pass
);


extern i32 fill_hashmap(xpparam_t const *xpp, struct xdpair *pair,
		struct hashmap *result,
		usize line1, usize count1, usize line2, usize count2);


DEFINE_IVEC_TYPE(struct entry*, entry_ptr);


extern isize binary_search(struct ivec_entry_ptr *sequence, isize longest,
		struct entry *entry);


extern i32 find_longest_common_sequence(struct hashmap *map, struct entry **res);


static bool record_equal(struct xdpair *pair, usize line1, usize line2) {
	u64 mph1 = pair->lhs.minimal_perfect_hash->ptr[line1 - LINE_SHIFT];
	u64 mph2 = pair->rhs.minimal_perfect_hash->ptr[line2 - LINE_SHIFT];
	return mph1 == mph2;
}

static i32 fall_back_to_classic_diff(u64 flags, struct xdpair *pair,
		usize line1, usize count1, usize line2, usize count2)
{
	xpparam_t xpp;

	memset(&xpp, 0, sizeof(xpp));
	xpp.flags = flags & ~XDF_DIFF_ALGORITHM_MASK;

	return xdl_fall_back_diff(pair, &xpp,
				  line1, count1, line2, count2);
}

static i32 patience_diff(xpparam_t const *xpp, struct xdpair *pair,
		usize line1, usize count1, usize line2, usize count2);

extern i32 walk_common_sequence(xpparam_t const *xpp, struct xdpair *pair,
	struct entry *first,
	usize line1, usize count1, usize line2, usize count2
);

extern i32 patience_diff(xpparam_t const *xpp, struct xdpair *pair,
		usize line1, usize count1, usize line2, usize count2
);

