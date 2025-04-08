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


/*
 * This function has to be called for each recursion into the inter-hunk
 * parts, as previously non-unique lines can become unique when being
 * restricted to a smaller part of the files.
 *
 * It is assumed that env has been prepared using xdl_prepare().
 */
static i32 fill_hashmap(xpparam_t const *xpp, struct xdpair *pair,
		struct hashmap *result,
		usize line1, usize count1, usize line2, usize count2)
{
	/* We know exactly how large we want the hash map */
	IVEC_INIT(result->entries);
	ivec_zero(&result->entries, count1 * 2);

	/* First, fill with entries from the first file */
	while (count1--)
		insert_record(xpp, pair, line1++, result, 1);

	/* Then search for matches in the second file */
	while (count2--)
		insert_record(xpp, pair, line2++, result, 2);

	return 0;
}

/*
 * Find the longest sequence with a smaller last element (meaning a smaller
 * line2, as we construct the sequence with entries ordered by line1).
 */
static isize binary_search(struct entry **sequence, usize longest,
		struct entry *entry)
{
	isize left = -1;
	usize right = longest;

	while (left + 1 < right) {
		usize middle = left + (right - left) / 2;
		/* by construction, no two entries can be equal */
		if (sequence[middle]->line2 > entry->line2)
			right = middle;
		else
			left = middle;
	}
	/* return the index in "sequence", _not_ the sequence length */
	return left;
}

/*
 * The idea is to start with the list of common unique lines sorted by
 * the order in file1.  For each of these pairs, the longest (partial)
 * sequence whose last element's line2 is smaller is determined.
 *
 * For efficiency, the sequences are kept in a list containing exactly one
 * item per sequence length: the sequence with the smallest last
 * element (in terms of line2).
 */
static i32 find_longest_common_sequence(struct hashmap *map, struct entry **res) {
	DEFINE_IVEC_TYPE(struct entry*, entry_ptr);
	struct ivec_entry_ptr sequence;
	IVEC_INIT(sequence);

	isize longest = 0, i;
	struct entry *entry;

	/*
	 * If not -1, this entry in sequence must never be overridden.
	 * Therefore, overriding entries before this has no effect, so
	 * do not do that either.
	 */
	isize anchor_i = -1;

	ivec_reserve_exact(&sequence, map->entries.length);

	for (entry = map->first; entry; entry = entry->next) {
		if (!entry->line2 || entry->line2 == NON_UNIQUE)
			continue;
		i = binary_search(sequence.ptr, longest, entry);
		entry->previous = i < 0 ? NULL : sequence.ptr[i];
		++i;
		if (i <= anchor_i)
			continue;
		sequence.ptr[i] = entry;
		if (entry->anchor) {
			anchor_i = i;
			longest = anchor_i + 1;
		} else if (i == longest) {
			longest++;
		}
	}

	/* No common unique lines were found */
	if (!longest) {
		*res = NULL;
		ivec_free(&sequence);
		return 0;
	}

	/* Iterate starting at the last element, adjusting the "next" members */
	entry = sequence.ptr[longest - 1];
	entry->next = NULL;
	while (entry->previous) {
		entry->previous->next = entry;
		entry = entry->previous;
	}
	*res = entry;
	ivec_free(&sequence);
	return 0;
}

static bool match(struct xdpair *pair, usize line1, usize line2) {
	u64 mph1 = pair->lhs.minimal_perfect_hash->ptr[line1 - LINE_SHIFT];
	u64 mph2 = pair->rhs.minimal_perfect_hash->ptr[line2 - LINE_SHIFT];
	return mph1 == mph2;
}

static i32 patience_diff(xpparam_t const *xpp, struct xdpair *pair,
		usize line1, usize count1, usize line2, usize count2);

static i32 walk_common_sequence(xpparam_t const *xpp, struct xdpair *pair,
	struct hashmap *map, struct entry *first,
	usize line1, usize count1, usize line2, usize count2
) {
	usize end1 = line1 + count1, end2 = line2 + count2;
	usize next1, next2;

	for (;;) {
		/* Try to grow the line ranges of common lines */
		if (first) {
			next1 = first->line1;
			next2 = first->line2;
			while (next1 > line1 && next2 > line2 &&
					match(pair, next1 - 1, next2 - 1)) {
				next1--;
				next2--;
			}
		} else {
			next1 = end1;
			next2 = end2;
		}
		while (line1 < next1 && line2 < next2 &&
				match(pair, line1, line2)) {
			line1++;
			line2++;
		}

		/* Recurse */
		if (next1 > line1 || next2 > line2) {
			if (patience_diff(xpp, pair,
					line1, next1 - line1,
					line2, next2 - line2))
				return -1;
		}

		if (!first)
			return 0;

		while (first->next &&
				first->next->line1 == first->line1 + 1 &&
				first->next->line2 == first->line2 + 1)
			first = first->next;

		line1 = first->line1 + 1;
		line2 = first->line2 + 1;

		first = first->next;
	}
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

/*
 * Recursively find the longest common sequence of unique lines,
 * and if none was found, ask xdl_do_diff() to do the job.
 *
 * This function assumes that env was prepared with xdl_prepare_env().
 */
static i32 patience_diff(xpparam_t const *xpp, struct xdpair *pair,
		usize line1, usize count1, usize line2, usize count2)
{
	struct hashmap map;
	struct entry *first;
	i32 result = 0;

	/* trivial case: one side is empty */
	if (!count1) {
		while(count2--)
			pair->rhs.consider.ptr[SENTINEL + line2++ - LINE_SHIFT] = YES;
		return 0;
	} else if (!count2) {
		while(count1--)
			pair->lhs.consider.ptr[SENTINEL + line1++ - LINE_SHIFT] = YES;
		return 0;
	}

	memset(&map, 0, sizeof(map));
	if (fill_hashmap(xpp, pair, &map,
			line1, count1, line2, count2))
		return -1;

	/* are there any matching lines at all? */
	if (!map.has_matches) {
		while(count1--)
			pair->lhs.consider.ptr[SENTINEL + line1++ - LINE_SHIFT] = YES;
		while(count2--)
			pair->rhs.consider.ptr[SENTINEL + line2++ - LINE_SHIFT] = YES;
		ivec_free(&map.entries);
		return 0;
	}

	result = find_longest_common_sequence(&map, &first);
	if (result)
		goto out;
	if (first)
		result = walk_common_sequence(xpp, pair, &map, first,
			line1, count1, line2, count2);
	else
		result = fall_back_to_classic_diff(xpp->flags, pair,
			line1, count1, line2, count2);
 out:
	ivec_free(&map.entries);
	return result;
}

i32 xdl_do_patience_diff(xpparam_t const *xpp, struct xdpair *pair) {
	return patience_diff(xpp, pair, LINE_SHIFT, pair->lhs.record->length, LINE_SHIFT, pair->rhs.record->length);
}
