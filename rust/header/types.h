#ifndef RUST_INTERFACE_H
#define RUST_INTERFACE_H

#ifdef __cplusplus
extern "C" {
#endif

#include "../../git-compat-util.h"

typedef uint8_t   u8;
typedef uint16_t  u16;
typedef uint32_t  u32;
typedef uint64_t  u64;

typedef int8_t    i8;
typedef int16_t   i16;
typedef int32_t   i32;
typedef int64_t   i64;

typedef float     f32;
typedef double    f64;

typedef size_t    usize;
typedef ptrdiff_t isize;

#ifdef __cplusplus
}
#endif

#endif //RUST_INTERFACE_H
