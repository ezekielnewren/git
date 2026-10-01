#ifndef MD5_BLOCK_MD5_H
#define MD5_BLOCK_MD5_H

#define blk_MD5_BLKSIZE 64

struct blk_MD5_CTX {
	uint32_t state[4];
	uint64_t size;
	uint32_t offset;
	uint8_t buf[blk_MD5_BLKSIZE];
};

typedef struct blk_MD5_CTX blk_MD5_CTX;

void blk_MD5_Init(blk_MD5_CTX *ctx);
void blk_MD5_Update(blk_MD5_CTX *ctx, const void *data, size_t len);
void blk_MD5_Final(unsigned char *digest, blk_MD5_CTX *ctx);

#define platform_MD5_CTX blk_MD5_CTX
#define platform_MD5_Init blk_MD5_Init
#define platform_MD5_Update blk_MD5_Update
#define platform_MD5_Final blk_MD5_Final

#endif
