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

#if !defined(XPREPARE_H)
#define XPREPARE_H


void xdl_file_prepare(mmfile_t *mf, u64 flags, xdfile_t *xdf);
void xdl_file_free(xdfile_t *xdf);

int  xdl_2way_prepare(mmfile_t *mf1, mmfile_t *mf2, u64 flags, struct xd2way *two_way);
void xdl_2way_free(struct xd2way *two_way);

int  xdl_3way_prepare(mmfile_t *mf_base, mmfile_t *mf_side1, mmfile_t *mf_side2, u64 flags, struct xd3way *three_way);
void xdl_3way_free(struct xd3way *three_way);

void xdl_env_init(xdfenv_t *xe);
int  xdl_env_prepare(xdfile_t *xdf1, xdfile_t *xdf2, u64 flags, usize mph_size, xdfenv_t *xe);



#endif /* #if !defined(XPREPARE_H) */
