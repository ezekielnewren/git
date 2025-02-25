#include <stdlib.h>
#include <string.h>
#include "ivec.h"

#include "xmacros.h"

static void _rust_ivec_resize(void* self, usize new_length, void* default_value, bool exact) {
	rawivec_t *this = self;
	isize additional = (isize) (new_length - this->capacity);
	if (additional > 0) {
		if (exact) {
			rust_ivec_reserve_exact(self, additional);
		} else {
			rust_ivec_reserve(self, additional);
		}
	}

	for (usize i = this->length; i < new_length; i++) {
		void* dst = (u8*) this->ptr + (this->length + i) * this->element_size;
		memcpy(dst, default_value, this->element_size);
	}
	this->length = new_length;
}

void rust_ivec_init(void* self, usize element_size) {
	rawivec_t *this = self;
	this->ptr = NULL;
	this->length = 0;
	this->capacity = 0;
	this->element_size = element_size;
}

void rust_ivec_reserve(void* self, usize additional) {
	rawivec_t *this = self;
	rust_ivec_reserve_exact(self, XDL_MAX(additional, this->capacity));
}

void rust_ivec_reserve_exact(void* self, usize additional) {
	rawivec_t *this = self;
	void* t;
	usize new_capacity = this->capacity + additional;

	t = xrealloc(this->ptr, new_capacity * this->element_size);
	if (t == NULL) {
		die("out of memory");
	}
	this->ptr = t;
	this->capacity = new_capacity;
}

void rust_ivec_shrink_to_fit(void* self) {
	rawivec_t *this = self;
	usize alloc = this->length * this->element_size;

	this->ptr = xrealloc(this->ptr, alloc);
	this->capacity = this->length;
}

void rust_ivec_resize(void* self, usize new_length, void* default_value) {
	_rust_ivec_resize(self, new_length, default_value, false);
}

void rust_ivec_resize_exact(void* self, usize new_length, void* default_value) {
	_rust_ivec_resize(self, new_length, default_value, true);
}

void rust_ivec_push(void* self, void* value) {
	rawivec_t *this = self;
	u8* dst;

	if (this->length + 1 > this->capacity) {
		rust_ivec_reserve(self, 1);
	}
	dst = (u8*) this->ptr + this->length * this->element_size;
	memcpy(dst, value, this->element_size);
	this->length += 1;
}

void* rust_ivec_steal_memory(void* self) {
	rawivec_t *this = self;
	void* t = this->ptr;
	this->ptr = NULL;
	this->capacity = 0;
	this->length = 0;
	this->element_size = 0;
	return t;
}


bool rust_ivec_equal(void* self, void* other) {
	rawivec_t *lhs = self;
	rawivec_t *rhs = other;

	if (lhs->element_size != rhs->element_size) {
		return false;
	}

	if (lhs->length != rhs->length) {
		return false;
	}


	for (usize i = 0; i < lhs->length; i++) {
		void* left = (u8 *) lhs->ptr + i * lhs->element_size;
		void* right = (u8 *) rhs->ptr + i * rhs->element_size;
		if (memcmp(left, right, lhs->element_size) != 0) {
			return false;
		}
	}

	return true;
}


void rust_ivec_free(void* self) {
	rawivec_t *this = self;
	free(this->ptr);
	this->capacity = 0;
	this->length = 0;
	// don't modify self->element_size
}
