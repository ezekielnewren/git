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


long xdl_bogosqrt(long n) {
	long i;

	/*
	 * Classical integer square root approximation using shifts.
	 */
	for (i = 1; n > 0; n >>= 2)
		i <<= 1;

	return i;
}


int xdl_emit_diffrec(char const *rec, long size, char const *pre, long psize,
		     xdemitcb_t *ecb) {
	int i = 2;
	mmbuffer_t mb[3];

	mb[0].ptr = (char *) pre;
	mb[0].size = psize;
	mb[1].ptr = (char *) rec;
	mb[1].size = size;
	if (size > 0 && rec[size - 1] != '\n') {
		mb[2].ptr = (char *) "\n\\ No newline at end of file\n";
		mb[2].size = strlen(mb[2].ptr);
		i++;
	}
	if (ecb->out_line(ecb->priv, mb, i) < 0) {

		return -1;
	}

	return 0;
}

void *xdl_mmfile_first(mmfile_t *mmf, long *size)
{
	*size = mmf->size;
	return mmf->ptr;
}


long xdl_mmfile_size(mmfile_t *mmf)
{
	return mmf->size;
}


int xdl_cha_init(chastore_t *cha, long isize, long icount) {

	cha->head = cha->tail = NULL;
	cha->isize = isize;
	cha->nsize = icount * isize;
	cha->ancur = cha->sncur = NULL;
	cha->scurr = 0;

	return 0;
}


void xdl_cha_free(chastore_t *cha) {
	chanode_t *cur, *tmp;

	for (cur = cha->head; (tmp = cur) != NULL;) {
		cur = cur->next;
		xdl_free(tmp);
	}
}


void *xdl_cha_alloc(chastore_t *cha) {
	chanode_t *ancur;
	void *data;

	if (!(ancur = cha->ancur) || ancur->icurr == cha->nsize) {
		if (!(ancur = (chanode_t *) xdl_malloc(sizeof(chanode_t) + cha->nsize))) {

			return NULL;
		}
		ancur->icurr = 0;
		ancur->next = NULL;
		if (cha->tail)
			cha->tail->next = ancur;
		if (!cha->head)
			cha->head = ancur;
		cha->tail = ancur;
		cha->ancur = ancur;
	}

	data = (char *) ancur + sizeof(chanode_t) + ancur->icurr;
	ancur->icurr += cha->isize;

	return data;
}

int xdl_blankline(const char *line, long size, long flags)
{
	long i;

	if (!(flags & XDF_WHITESPACE_FLAGS))
		return (size <= 1);

	for (i = 0; i < size && XDL_ISSPACE(line[i]); i++)
		;

	return (i == size);
}



unsigned int xdl_hashbits(unsigned int size) {
	unsigned int val = 1, bits = 0;

	for (; val < size && bits < CHAR_BIT * sizeof(unsigned int); val <<= 1, bits++);
	return bits ? bits: 1;
}


int xdl_num_out(char *out, long val) {
	char *ptr, *str = out;
	char buf[32];

	ptr = buf + sizeof(buf) - 1;
	*ptr = '\0';
	if (val < 0) {
		*--ptr = '-';
		val = -val;
	}
	for (; val && ptr > buf; val /= 10)
		*--ptr = "0123456789"[val % 10];
	if (*ptr)
		for (; *ptr; ptr++, str++)
			*str = *ptr;
	else
		*str++ = '0';
	*str = '\0';

	return str - out;
}

static int xdl_format_hunk_hdr(long s1, long c1, long s2, long c2,
			       const char *func, long funclen,
			       xdemitcb_t *ecb) {
	int nb = 0;
	mmbuffer_t mb;
	char buf[128];

	memcpy(buf, "@@ -", 4);
	nb += 4;

	nb += xdl_num_out(buf + nb, c1 ? s1: s1 - 1);

	if (c1 != 1) {
		memcpy(buf + nb, ",", 1);
		nb += 1;

		nb += xdl_num_out(buf + nb, c1);
	}

	memcpy(buf + nb, " +", 2);
	nb += 2;

	nb += xdl_num_out(buf + nb, c2 ? s2: s2 - 1);

	if (c2 != 1) {
		memcpy(buf + nb, ",", 1);
		nb += 1;

		nb += xdl_num_out(buf + nb, c2);
	}

	memcpy(buf + nb, " @@", 3);
	nb += 3;
	if (func && funclen) {
		buf[nb++] = ' ';
		if (funclen > sizeof(buf) - nb - 1)
			funclen = sizeof(buf) - nb - 1;
		memcpy(buf + nb, func, funclen);
		nb += funclen;
	}
	buf[nb++] = '\n';

	mb.ptr = buf;
	mb.size = nb;
	if (ecb->out_line(ecb->priv, &mb, 1) < 0)
		return -1;
	return 0;
}

int xdl_emit_hunk_hdr(long s1, long c1, long s2, long c2,
		      const char *func, long funclen,
		      xdemitcb_t *ecb) {
	if (!ecb->out_hunk)
		return xdl_format_hunk_hdr(s1, c1, s2, c2, func, funclen, ecb);
	if (ecb->out_hunk(ecb->priv,
			  c1 ? s1 : s1 - 1, c1,
			  c2 ? s2 : s2 - 1, c2,
			  func, funclen) < 0)
		return -1;
	return 0;
}

int xdl_fall_back_diff(xdfenv_t *diff_env, xpparam_t const *xpp,
		int line1, int count1, int line2, int count2)
{
	/*
	 * This probably does not work outside Git, since
	 * we have a very simple mmfile structure.
	 *
	 * Note: ideally, we would reuse the prepared environment, but
	 * the libxdiff interface does not (yet) allow for diffing only
	 * ranges of lines instead of the whole files.
	 */
	mmfile_t subfile1, subfile2;
	xdfenv_t env;

	subfile1.ptr = (char *) diff_env->xdf1.record.ptr[line1 - 1].ptr;
	subfile1.size = (char *) diff_env->xdf1.record.ptr[line1 + count1 - 2].ptr +
		diff_env->xdf1.record.ptr[line1 + count1 - 2].size_with_eol - subfile1.ptr;
	subfile2.ptr = (char *) diff_env->xdf2.record.ptr[line2 - 1].ptr;
	subfile2.size = (char *) diff_env->xdf2.record.ptr[line2 + count2 - 2].ptr +
		diff_env->xdf2.record.ptr[line2 + count2 - 2].size_with_eol - subfile2.ptr;
	if (xdl_do_diff(&subfile1, &subfile2, xpp, &env) < 0)
		return -1;

	memcpy(diff_env->xdf1.rchg + line1 - 1, env.xdf1.rchg, count1);
	memcpy(diff_env->xdf2.rchg + line2 - 1, env.xdf2.rchg, count2);

	xdl_free_env(&env);

	return 0;
}

void* xdl_alloc_grow_helper(void *p, long nr, long *alloc, size_t size)
{
	void *tmp = NULL;
	size_t n = ((LONG_MAX - 16) / 2 >= *alloc) ? 2 * *alloc + 16 : LONG_MAX;
	if (nr > n)
		n = nr;
	if (SIZE_MAX / size >= n)
		tmp = xdl_realloc(p, n * size);
	if (tmp) {
		*alloc = n;
	} else {
		xdl_free(p);
		*alloc = 0;
	}
	return tmp;
}

void xdl_line_length(u8 const* start, u8 const* end, bool ignore_cr_at_eol, usize *no_eol, usize *with_eol) {
	usize size = end - start;
	*no_eol = size;
	*with_eol = size;
	for (usize i = 0; i < size; i++) {
		if (start[i] == '\n') {
			*no_eol = i;
			*with_eol = i+1;
			break;
		}
	}

	if (ignore_cr_at_eol && *no_eol > 0 && start[*no_eol - 1] == '\r') {
		*no_eol -= 1;
	}
}

#ifdef DEBUG
static void validate_line_arguments(
	u8 const* ptr, usize line_size_without_eol, u64 flags
) {
	if (ptr == NULL) {
		BUG("xdl_line_iter_init() ptr is null");
	}
	if (line_size_without_eol > 0) {
		for (usize i = 0; i < line_size_without_eol - 1; i++) {
			if (ptr[i] == '\n') {
				BUG("line_size_without_eol incorrect: found lf in the middle of the line");
			}
		}
		if (ptr[line_size_without_eol - 1] == '\n') {
			BUG("line_size_without_eol incorrect: found trailing lf");
		}
		// if ((flags & XDF_IGNORE_CR_AT_EOL) != 0 && ptr[line_size_without_eol - 1] == '\r') {
		// 	BUG("line_size_without_eol incorrect: found trailing cr with flag XDF_IGNORE_CR_AT_EOL set");
		// }
	}
}
#endif

/*
 * line_size_without_eol means that for the line "ab\r\n" the size is 3
 * if XDF_IGNORE_CR_AT_EOL is set then the size is 2
 */
void xdl_line_iter_init(struct xlineiter_t* it,
	u8 const* ptr, usize line_size_without_eol, u64 flags
) {
#ifdef DEBUG
	if (it == NULL) {
		BUG("xlineiter_t is null");
	}
	validate_line_arguments(ptr, line_size_without_eol, flags);
#endif
	it->ptr = ptr;
	it->size = line_size_without_eol;
	it->index = 0;
	it->flags = flags;
}

bool xdl_line_iter_next(struct xlineiter_t* it, u8 const** ptr, usize *run_size) {
	if (it->index >= it->size) {
		*ptr = NULL;
		*run_size = 0;
		return false;
	}

	if ((it->flags & XDF_IGNORE_WHITESPACE_WITHIN) == 0) {
		it->index = it->size;
		*ptr = it->ptr;
		*run_size = it->size;
		return true;
	}

	while (true) {
		usize start = it->index;
		if (it->index == it->size) {
			*ptr = NULL;
			*run_size = 0;
			return false;
		}

		/* return contiguous run of not space bytes */
		while (it->index < it->size) {
			if XDL_ISSPACE(it->ptr[it->index]) {
				break;
			}
			it->index += 1;
		}
		if (it->index > start) {
			*ptr = it->ptr + start;
			*run_size = it->index - start;
			return true;
		}
		/* the current byte had better be a space */
#ifdef DEBUG
		if (!XDL_ISSPACE(it->ptr[it->index])) {
			BUG("xdl_line_iter_next XDL_ISSPACE() is false")
		}
#endif

		for (; it->index < it->size; it->index++) {
			if (!XDL_ISSPACE(it->ptr[it->index])) {
				break;
			}
		}

#ifdef DEBUG
		if (it->index <= start) {
			BUG("XDL_ISSPACE() cannot simultaneously be true and false");
		}
#endif
		if ((it->flags & XDF_IGNORE_WHITESPACE_AT_EOL) != 0
		    && it->index == it->size)
		{
			*ptr = NULL;
			*run_size = 0;
			return false;
		}
		if ((it->flags & XDF_IGNORE_WHITESPACE) != 0) {
			continue;
		}
		if ((it->flags & XDF_IGNORE_WHITESPACE_CHANGE) != 0) {
			const u8 *SINGLE_SPACE = (const u8 *) " ";
			if (it->index == it->size) {
				continue;
			}
			*ptr = SINGLE_SPACE;
			*run_size = 1;
			return true;
		}
		*ptr = it->ptr + start;
		*run_size = it->index - start;
		return true;
	}
}

void xdl_line_iter_done(struct xlineiter_t* it) {
#ifdef DEBUG
	if (it->index < it->size) {
		BUG("xlineiter_t: didn't consume the whole iterator");
	}
	if (it->index > it->size) {
		BUG("xlineiter_t: index was incremented too much");
	}
#endif
	it->ptr = NULL;
	it->size = 0;
	it->index = 0;
	it->flags = 0;
}

u64 xdl_line_hash(u8 const* ptr, usize line_size_without_eol, u64 flags) {
	struct xlineiter_t it;
	u8 const* run_start;
	usize run_size;

#ifdef DEBUG
	validate_line_arguments(ptr, line_size_without_eol, flags);
#endif

	u64 hash = 5381;

	xdl_line_iter_init(&it, ptr, line_size_without_eol, flags);
	while (xdl_line_iter_next(&it, &run_start, &run_size)) {
		for (usize i = 0; i < run_size; i++) {
			hash = hash * 33 ^ (u64) run_start[i];
		}
	}
	xdl_line_iter_done(&it);

	return hash;
}

bool xdl_line_equal(u8 const* line1, usize size1, u8 const* line2, usize size2, u64 flags) {
	struct xlineiter_t it1, it2;
	u8 const *run_start1, *run_start2;
	usize run_size1, run_size2;
	usize i1, i2;
	bool has_next1, has_next2;

#ifdef DEBUG
	validate_line_arguments(line1, size1, flags);
	validate_line_arguments(line2, size2, flags);
#endif

	if ((flags & XDF_IGNORE_WHITESPACE_WITHIN) == 0) {
		return memcmp(line1, line2, size1) == 0;
	}

	xdl_line_iter_init(&it1, line1, size1, flags);
	xdl_line_iter_init(&it2, line2, size2, flags);

	has_next1 = xdl_line_iter_next(&it1, &run_start1, &run_size1);
	has_next2 = xdl_line_iter_next(&it2, &run_start2, &run_size2);

	i1 = 0, i2 = 0;
	while (has_next1 && has_next2) {
		while (i1 < run_size1 && i2 < run_size2) {
			if (run_start1[i1] != run_start2[i2])
				return false;
			i1++, i2++;
		}

		if (i1 == run_size1) {
			i1 = 0;
			has_next1 = xdl_line_iter_next(&it1, &run_start1, &run_size1);
		}

		if (i2 == run_size2) {
			i2 = 0;
			has_next2 = xdl_line_iter_next(&it2, &run_start2, &run_size2);
		}
	}

	/*
	 * check for emtpy runs
	 */
	while (has_next1 && run_size1 == 0) {
		has_next1 = xdl_line_iter_next(&it1, &run_start1, &run_size1);
	}

	while (has_next2 && run_size2 == 0) {
		has_next2 = xdl_line_iter_next(&it2, &run_start2, &run_size2);
	}

	return !has_next1 && !has_next2;
}
