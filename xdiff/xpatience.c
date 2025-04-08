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
	usize nr, alloc;
	struct ivec_entry entries;
	struct entry *first, *last;
	/* were common records found? */
	bool has_matches;
	struct xdpair *pair;
	xpparam_t const *xpp;
};

static bool is_anchor(xpparam_t const *xpp, const char *line)
{
	size_t i;
	for (i = 0; i < xpp->anchors_nr; i++) {
		if (!strncmp(line, xpp->anchors[i], strlen(xpp->anchors[i])))
			return true;
	}
	return false;
}

/* The argument "pass" is 1 for the first file, 2 for the second. */
static void insert_record(xpparam_t const *xpp, usize line, struct hashmap *map,
			  i32 pass)
{
	u64* mph_vec = pass == 1 ?
		map->pair->lhs.minimal_perfect_hash->ptr : map->pair->rhs.minimal_perfect_hash->ptr;
	u64 mph = mph_vec[line - 1];
	/*
	 * After xdl_prepare_env() (or more precisely, due to
	 * xdl_classify_record()), the "ha" member of the records (AKA lines)
	 * is _not_ the hash anymore, but a linearized version of it.  In
	 * other words, the "ha" member is guaranteed to start with 0 and
	 * the second record's ha can only be 0 or 1, etc.
	 *
	 * So we multiply ha by 2 in the hope that the hashing was
	 * "unique enough".
	 */
	usize index = ((mph << 1) % map->alloc);

	while (map->entries.ptr[index].line1) {
		if (map->entries.ptr[index].minimal_perfect_hash != mph) {
			if (++index >= map->alloc)
				index = 0;
			continue;
		}
		if (pass == 2)
			map->has_matches = true;
		if (pass == 1 || map->entries.ptr[index].line2)
			map->entries.ptr[index].line2 = NON_UNIQUE;
		else
			map->entries.ptr[index].line2 = line;
		return;
	}
	if (pass == 2)
		return;
	map->entries.ptr[index].line1 = line;
	map->entries.ptr[index].minimal_perfect_hash = mph;
	map->entries.ptr[index].anchor = is_anchor(xpp, (const char*) map->pair->lhs.record->ptr[line - 1].ptr);
	if (!map->first)
		map->first = &map->entries.ptr[index];
	if (map->last) {
		map->last->next = &map->entries.ptr[index];
		map->entries.ptr[index].previous = map->last;
	}
	map->last = &map->entries.ptr[index];
	map->nr++;
}

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
	result->xpp = xpp;
	result->pair = pair;

	/* We know exactly how large we want the hash map */
	result->alloc = count1 * 2;
	IVEC_INIT(result->entries);
	ivec_zero(&result->entries, result->alloc);

	/* First, fill with entries from the first file */
	while (count1--)
		insert_record(xpp, line1++, result, 1);

	/* Then search for matches in the second file */
	while (count2--)
		insert_record(xpp, line2++, result, 2);

	return 0;
}

/*
 * Find the longest sequence with a smaller last element (meaning a smaller
 * line2, as we construct the sequence with entries ordered by line1).
 */
static i32 binary_search(struct entry **sequence, int longest,
		struct entry *entry)
{
	int left = -1, right = longest;

	while (left + 1 < right) {
		int middle = left + (right - left) / 2;
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
static int find_longest_common_sequence(struct hashmap *map, struct entry **res)
{
	struct entry **sequence;
	int longest = 0, i;
	struct entry *entry;

	/*
	 * If not -1, this entry in sequence must never be overridden.
	 * Therefore, overriding entries before this has no effect, so
	 * do not do that either.
	 */
	int anchor_i = -1;

	if (!XDL_ALLOC_ARRAY(sequence, map->nr))
		return -1;

	for (entry = map->first; entry; entry = entry->next) {
		if (!entry->line2 || entry->line2 == NON_UNIQUE)
			continue;
		i = binary_search(sequence, longest, entry);
		entry->previous = i < 0 ? NULL : sequence[i];
		++i;
		if (i <= anchor_i)
			continue;
		sequence[i] = entry;
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
		xdl_free(sequence);
		return 0;
	}

	/* Iterate starting at the last element, adjusting the "next" members */
	entry = sequence[longest - 1];
	entry->next = NULL;
	while (entry->previous) {
		entry->previous->next = entry;
		entry = entry->previous;
	}
	*res = entry;
	xdl_free(sequence);
	return 0;
}

static int match(struct hashmap *map, int line1, int line2) {
	u64 mph1 = map->pair->lhs.minimal_perfect_hash->ptr[line1 - 1];
	u64 mph2 = map->pair->rhs.minimal_perfect_hash->ptr[line2 - 1];
	return mph1 == mph2;
}

static int patience_diff(xpparam_t const *xpp, struct xdpair *pair,
		int line1, int count1, int line2, int count2);

static int walk_common_sequence(struct hashmap *map, struct entry *first,
		int line1, int count1, int line2, int count2)
{
	int end1 = line1 + count1, end2 = line2 + count2;
	int next1, next2;

	for (;;) {
		/* Try to grow the line ranges of common lines */
		if (first) {
			next1 = first->line1;
			next2 = first->line2;
			while (next1 > line1 && next2 > line2 &&
					match(map, next1 - 1, next2 - 1)) {
				next1--;
				next2--;
			}
		} else {
			next1 = end1;
			next2 = end2;
		}
		while (line1 < next1 && line2 < next2 &&
				match(map, line1, line2)) {
			line1++;
			line2++;
		}

		/* Recurse */
		if (next1 > line1 || next2 > line2) {
			if (patience_diff(map->xpp, map->pair,
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

static int fall_back_to_classic_diff(struct hashmap *map,
		int line1, int count1, int line2, int count2)
{
	xpparam_t xpp;

	memset(&xpp, 0, sizeof(xpp));
	xpp.flags = map->xpp->flags & ~XDF_DIFF_ALGORITHM_MASK;

	return xdl_fall_back_diff(map->pair, &xpp,
				  line1, count1, line2, count2);
}

/*
 * Recursively find the longest common sequence of unique lines,
 * and if none was found, ask xdl_do_diff() to do the job.
 *
 * This function assumes that env was prepared with xdl_prepare_env().
 */
static int patience_diff(xpparam_t const *xpp, struct xdpair *pair,
		int line1, int count1, int line2, int count2)
{
	struct hashmap map;
	struct entry *first;
	int result = 0;

	/* trivial case: one side is empty */
	if (!count1) {
		while(count2--)
			pair->rhs.consider.ptr[SENTINEL + line2++ - 1] = YES;
		return 0;
	} else if (!count2) {
		while(count1--)
			pair->lhs.consider.ptr[SENTINEL + line1++ - 1] = YES;
		return 0;
	}

	memset(&map, 0, sizeof(map));
	if (fill_hashmap(xpp, pair, &map,
			line1, count1, line2, count2))
		return -1;

	/* are there any matching lines at all? */
	if (!map.has_matches) {
		while(count1--)
			pair->lhs.consider.ptr[SENTINEL + line1++ - 1] = YES;
		while(count2--)
			pair->rhs.consider.ptr[SENTINEL + line2++ - 1] = YES;
		ivec_free(&map.entries);
		return 0;
	}

	result = find_longest_common_sequence(&map, &first);
	if (result)
		goto out;
	if (first)
		result = walk_common_sequence(&map, first,
			line1, count1, line2, count2);
	else
		result = fall_back_to_classic_diff(&map,
			line1, count1, line2, count2);
 out:
	ivec_free(&map.entries);
	return result;
}

int xdl_do_patience_diff(xpparam_t const *xpp, struct xdpair *pair)
{
	return patience_diff(xpp, pair, 1, pair->lhs.record->length, 1, pair->rhs.record->length);
}
