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

struct xdl_mphb_node_t {
	u8 const* ptr;
	usize size_no_eol;
	u64 line_hash;
	u64 value;
	struct xdl_mphb_node_t *next;
};

DEFINE_IVEC_TYPE(struct xdl_mphb_node_t, xdl_mphb_node_t);
DEFINE_IVEC_TYPE(struct xdl_mphb_node_t*, xdl_mphb_node_ptr_t);

struct xdl_minimal_perfect_hash_builder_t {
	struct xdl_mphb_node_t **head;
	struct xdl_mphb_node_t *kv;
	usize kv_capacity;
	usize kv_length;
	u32 hbits;
	u64 flags;
};

struct xwhitespaceiter_t {
	u8 const* ptr;
	usize size;
	usize index;
	u64 flags;
};


struct xlinereader_t {
	u8 const* cur;
	usize size;
};

long xdl_bogosqrt(long n);
int xdl_emit_diffrec(char const *rec, long size, char const *pre, long psize,
		     xdemitcb_t *ecb);
int xdl_cha_init(chastore_t *cha, long isize, long icount);
void xdl_cha_free(chastore_t *cha);
void *xdl_cha_alloc(chastore_t *cha);
int xdl_blankline(const char *line, long size, long flags);
int xdl_recmatch(const char *l1, long s1, const char *l2, long s2, long flags);
unsigned long xdl_hash_record(char const **data, char const *top, long flags);
unsigned int xdl_hashbits(unsigned int size);
int xdl_num_out(char *out, long val);
int xdl_emit_hunk_hdr(long s1, long c1, long s2, long c2,
		      const char *func, long funclen, xdemitcb_t *ecb);
int xdl_fall_back_diff(xdfenv_t *diff_env, xpparam_t const *xpp,
		       int line1, int count1, int line2, int count2);

void xdl_mphb_init(struct xdl_minimal_perfect_hash_builder_t *mphb, usize size, u64 flags);
u64 xdl_mphb_hash(struct xdl_minimal_perfect_hash_builder_t *mphb, xrecord_t *key);
usize xdl_mphb_finish(struct xdl_minimal_perfect_hash_builder_t *mphb);
void xdl_linereader_init(struct xlinereader_t *it, u8 const* ptr, usize size);
bool xdl_linereader_next(struct xlinereader_t *it, u8 const **cur, usize *no_eol, usize *with_eol);
void xdl_linereader_assert_done(struct xlinereader_t *it);
void xdl_whitespace_iter_init(struct xwhitespaceiter_t* it, u8 const* ptr, usize line_size_without_eol, u64 flags);
bool xdl_whitespace_iter_next(struct xwhitespaceiter_t* it, u8 const** ptr, usize *run_size);
void xdl_whitespace_iter_assert_done(struct xwhitespaceiter_t* it);
u64  xdl_line_hash(u8 const* ptr, usize line_size_without_eol, u64 flags);
bool xdl_line_equal(u8 const* line1, usize size1, u8 const* line2, usize size2, u64 flags);
bool xdl_record_equal(xrecord_t *lhs, xrecord_t *rhs, u64 flags);


/* Do not call this function, use XDL_ALLOC_GROW instead */
void* xdl_alloc_grow_helper(void* p, long nr, long* alloc, size_t size);

#endif /* #if !defined(XUTILS_H) */
