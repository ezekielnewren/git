#include "git-compat-util.h"
#include "./md5.h"

#undef RND
#undef BLKSIZE

#define BLKSIZE blk_MD5_BLKSIZE

static const uint32_t S[64] = {
//  0               4               8               12
    7, 12, 17, 22,  7, 12, 17, 22,  7, 12, 17, 22,  7, 12, 17, 22,
//  16              20              24              28
    5,  9, 14, 20,  5,  9, 14, 20,  5,  9, 14, 20,  5,  9, 14, 20,
//  32              36              40              44
    4, 11, 16, 23,  4, 11, 16, 23,  4, 11, 16, 23,  4, 11, 16, 23,
//  48              52              56              60
    6, 10, 15, 21,  6, 10, 15, 21,  6, 10, 15, 21,  6, 10, 15, 21,
};


static const uint32_t K[64] = {
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee,  // 0
    0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,  // 4
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,  // 8
    0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,  // 12
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa,  // 16
    0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,  // 20
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,  // 24
    0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,  // 28
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,  // 32
    0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,  // 36
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05,  // 40
    0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,  // 44
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039,  // 48
    0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,  // 52
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,  // 56
    0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,  // 60
};

void blk_MD5_Init(blk_MD5_CTX *ctx)
{
	ctx->offset = 0;
	ctx->size = 0;
	ctx->state[0] = 0x67452301ul;
	ctx->state[1] = 0xefcdab89ul;
	ctx->state[2] = 0x98badcfeul;
	ctx->state[3] = 0x10325476ul;
}

static size_t step2round(size_t step)
{
	return step >> 4;
}
static inline uint32_t rotl32(uint32_t x, unsigned n)
{
	return (x << n) | (x >> (32 - n));
}
static uint32_t ff(uint32_t b, uint32_t c, uint32_t d)
{
	return d ^ (b & (c ^ d));
}
static uint32_t gg(uint32_t b, uint32_t c, uint32_t d)
{
	return c ^ (d & (b ^ c));
}
static uint32_t hh(uint32_t b, uint32_t c, uint32_t d)
{
	return b ^ c ^ d;
}
static uint32_t ii(uint32_t b, uint32_t c, uint32_t d)
{
	return c ^ (b | ~d);
}

static uint32_t combination_function(size_t round, uint32_t b, uint32_t c, uint32_t d)
{
	switch (round) {
		case 0: return ff(b, c, d);
		case 1: return gg(b, c, d);
		case 2: return hh(b, c, d);
		case 3: return ii(b, c, d);
		default: die("round can only be 0, 1, 2, 3");
	}
}

static size_t m_shuffle(size_t round, size_t step)
{
	switch (round) {
		case 0: return step;
		case 1: return (5 * step + 1) & 0xF;
		case 2: return (3 * step + 5) & 0xF;
		case 3: return (7 * step) & 0xF;
		default: die("round can only be 0, 1, 2, 3");
	}
}


static uint32_t md5_step_forward(size_t step, uint32_t a0, uint32_t b0, uint32_t c0, uint32_t d0, const uint32_t *block)
{
	size_t round = step2round(step);
	uint32_t f = combination_function(round, b0, c0, d0);
	uint32_t m = block[m_shuffle(round, step)];

	return rotl32(a0 + K[step] + m + f, S[step]) + b0;
}


static void blk_MD5_Transform(blk_MD5_CTX *ctx, const unsigned char *buf)
{
	const size_t WORD_COUNT = BLKSIZE/sizeof(uint32_t);
	uint32_t s[4], block[BLKSIZE/sizeof(uint32_t)];
	memcpy(s, ctx->state, sizeof(s));

	for (size_t i = 0; i < WORD_COUNT; i++, buf += sizeof(uint32_t)) {
		block[i] = get_le32(buf);
	}

	for (size_t step = 0; step < 64; step++) {
		s[0] = md5_step_forward(step, s[0], s[1], s[2], s[3], block);
		// (a, b, c, d) = (d, a, b, c);
		// (s[0], s[1], s[2], s[3]) = (s[3], s[0], s[1], s[2]);
		uint32_t tmp = s[3];
		s[3] = s[2];
		s[2] = s[1];
		s[1] = s[0];
		s[0] = tmp;
	}

	for (size_t i = 0; i < 4; i++) {
		ctx->state[i] += s[i];
	}
}

void blk_MD5_Update(blk_MD5_CTX *ctx, const void *data, size_t len)
{
	unsigned int len_buf = ctx->size & 63;

	ctx->size += len;

	/* Read the data into buf and process blocks as they get full */
	if (len_buf) {
		unsigned int left = 64 - len_buf;
		if (len < left)
			left = len;
		memcpy(len_buf + ctx->buf, data, left);
		len_buf = (len_buf + left) & 63;
		len -= left;
		data = ((const char *)data + left);
		if (len_buf)
			return;
		blk_MD5_Transform(ctx, ctx->buf);
	}
	while (len >= 64) {
		blk_MD5_Transform(ctx, data);
		data = ((const char *)data + 64);
		len -= 64;
	}
	if (len)
		memcpy(ctx->buf, data, len);
}

void blk_MD5_Final(uint8_t *digest, blk_MD5_CTX *ctx)
{
	static const uint8_t pad[64] = { 0x80 };
	uint8_t padlen[8];

	put_le64(padlen, ctx->size << 3);

	blk_MD5_Update(ctx, pad, 1 + (63 & (55 - (ctx->size & 63))));
	blk_MD5_Update(ctx, padlen, sizeof(padlen));

	/* copy output */
	for (size_t i = 0; i < 4; i++, digest += sizeof(uint32_t))
		put_le32(digest, ctx->state[i]);
}
