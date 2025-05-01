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



long xdl_bogosqrt(long n);
int xdl_emit_diffrec(char const *rec, long size, char const *pre, long psize,
		     struct xdemitcb *ecb);
int xdl_blankline(const char *line, long size, long flags);
extern bool xdl_line_equal(u8 const* line1, usize size1, u8 const* line2, usize size2, u64 flags);
u64 xdl_line_hash(u8 const* line, usize size, u64 flags);
unsigned int xdl_hashbits(unsigned int size);
int xdl_num_out(char *out, long val);
int xdl_emit_hunk_hdr(long s1, long c1, long s2, long c2,
		      const char *func, long funclen, struct xdemitcb *ecb);

#endif /* #if !defined(XUTILS_H) */
