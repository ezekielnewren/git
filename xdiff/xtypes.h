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

#include "../rust/header/types.h"
#include "ivec.h"

typedef struct s_chanode {
	struct s_chanode *next;
	long icurr;
} chanode_t;

typedef struct s_chastore {
	chanode_t *head, *tail;
	long isize, nsize;
	chanode_t *ancur;
	chanode_t *sncur;
	long scurr;
} chastore_t;

typedef struct {
	u8 const* ptr;
	usize size;
	u64 hash;
	u64 flags;
} xrecord_t;

DEFINE_IVEC_TYPE(xrecord_t, xrecord_t);
DEFINE_IVEC_TYPE(xrecord_t*, xrecord_ptr_t);

typedef struct s_xdfile {
	ivec_xrecord_t record;
	ivec_xrecord_ptr_t useless;
	xrecord_t **recs;
	long dstart, dend;
	ivec_u8 rchg_vec;
	char *rchg;
	ivec_isize rindex;
	ivec_u64 hash;
} xdfile_t;

typedef struct s_xdfenv {
	xdfile_t xdf1, xdf2;
} xdfenv_t;



#endif /* #if !defined(XTYPES_H) */
