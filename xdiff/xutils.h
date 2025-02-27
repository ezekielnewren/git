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

#if !defined(XUTILS_H)
#define XUTILS_H

struct xlineiter_t {
	u8 const* ptr;
	usize size;
	usize index;
	u64 flags;
};


long xdl_bogosqrt(long n);
int xdl_emit_diffrec(char const *rec, long size, char const *pre, long psize,
		     xdemitcb_t *ecb);
int xdl_cha_init(chastore_t *cha, long isize, long icount);
void xdl_cha_free(chastore_t *cha);
void *xdl_cha_alloc(chastore_t *cha);
int xdl_blankline(const char *line, long size, long flags);
unsigned int xdl_hashbits(unsigned int size);
int xdl_num_out(char *out, long val);
int xdl_emit_hunk_hdr(long s1, long c1, long s2, long c2,
		      const char *func, long funclen, xdemitcb_t *ecb);
int xdl_fall_back_diff(xdfenv_t *diff_env, xpparam_t const *xpp,
		       int line1, int count1, int line2, int count2);

/* Do not call this function, use XDL_ALLOC_GROW instead */
void* xdl_alloc_grow_helper(void* p, long nr, long* alloc, size_t size);
void xdl_line_length(u8 const* start, u8 const* end, bool ignore_cr_at_eol, usize *no_eol, usize *with_eol);
void xdl_line_iter_init(struct xlineiter_t* it, u8 const* ptr, usize line_size_without_eol, u64 flags);
bool xdl_line_iter_next(struct xlineiter_t* it, u8 const** ptr, usize *run_size);
void xdl_line_iter_done(struct xlineiter_t* it);
u64  xdl_line_hash(u8 const* ptr, usize line_size_without_eol, u64 flags);
bool xdl_line_equal(u8 const* line1, usize size1, u8 const* line2, usize size2, u64 flags);

#endif /* #if !defined(XUTILS_H) */
