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

#if !defined(XTYPES_H)
#define XTYPES_H

#include "ivec.h"

struct xrange {
	usize start, end;
};

struct xoccurrence {
	usize file1;
	usize file2;
};

DEFINE_IVEC_TYPE(struct xoccurrence, xoccurrence);

struct xrecord {
	u8 const* ptr;
	usize size;
};

DEFINE_IVEC_TYPE(struct xrecord, xrecord);

struct xdfile {
	struct ivec_u64 minimal_perfect_hash;
	struct ivec_xrecord record;
};

struct xd_file_context {
	struct ivec_u64 *minimal_perfect_hash;
	struct ivec_xrecord *record;
	struct ivec_u8 consider;
	struct ivec_usize rindex;
};

struct xdpair {
	struct xd_file_context lhs, rhs;
	usize delta_start, delta_end;
	usize minimal_perfect_hash_size;
};

struct xd2way {
	struct xdfile lhs;
	struct xdfile rhs;
	struct xdpair pair;
	usize minimal_perfect_hash_size;
};

struct xd3way {
	struct xdfile base;
	struct xdfile side1;
	struct xdfile side2;
	struct xdpair pair1;
	struct xdpair pair2;
	usize minimal_perfect_hash_size;
};

extern void xdl_2way_prepare(mmfile_t const* mf1, mmfile_t const* mf2,
	u64 flags, struct xd2way *two_way);
extern void xdl_2way_free(struct xd2way *two_way);

extern void xdl_3way_prepare(mmfile_t const* base, mmfile_t const* side1, mmfile_t const* side2,
	u64 flags, struct xd3way *three_way);
extern void xdl_3way_free(struct xd3way *three_way);

extern void xdl_2way_slice(
	struct xd_file_context *lhs, struct xrange lhs_range,
	struct xd_file_context *rhs, struct xrange rhs_range,
	usize mph_size, struct xd2way *two_way
);

#endif /* #if !defined(XTYPES_H) */
