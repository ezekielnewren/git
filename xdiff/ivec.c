#include <stdlib.h>
#include <string.h>
#include "ivec.h"

#include "xmacros.h"

static void rust_ivec_set_capacity(void* self, usize new_capacity) {
	rawivec_t *this = self;
	this->ptr = xrealloc(this->ptr, new_capacity * this->element_size);
	this->capacity = new_capacity;
}

void rust_ivec_init(void* self, usize element_size) {
	rawivec_t *this = self;
	this->ptr = NULL;
	this->length = 0;
	this->capacity = 0;
	this->element_size = element_size;
}

void rust_ivec_reserve_exact(void* self, usize additional) {
	rawivec_t *this = self;
	usize new_capacity = this->capacity + additional;
	rust_ivec_set_capacity(self, new_capacity);
}

void rust_ivec_reserve(void* self, usize additional) {
	rawivec_t *this = self;
	usize growby = XDL_MIN(128, this->capacity);
	rust_ivec_reserve_exact(self, XDL_MAX(additional, growby));
}

void rust_ivec_shrink_to_fit(void* self) {
	rawivec_t *this = self;
	rust_ivec_set_capacity(self, this->length);
}

void rust_ivec_resize(void* self, usize new_length, void* default_value) {
	rawivec_t *this = self;
	isize additional = (isize) (new_length - this->capacity);
	if (additional > 0) {
		rust_ivec_reserve(self, additional);
	}

	for (usize i = this->length; i < new_length; i++) {
		void* dst = (u8*) this->ptr + (this->length + i) * this->element_size;
		memcpy(dst, default_value, this->element_size);
	}
	this->length = new_length;
}

void rust_ivec_push(void* self, void* value) {
	rawivec_t *this = self;
	u8* dst;

	if (this->length == this->capacity) {
		rust_ivec_reserve(self, 1);
	}
	dst = (u8*) this->ptr + this->length * this->element_size;
	memcpy(dst, value, this->element_size);
	this->length++;
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

/*
 * dest MUST already be initialized, this function will destroy anything
 * that already exists in dest
 */
void rust_ivec_clone(void* self, void* dest) {
	rawivec_t *this = self;
	rawivec_t *other = dest;
	if (this->element_size != other->element_size) {
		BUG("both ivec instances must have the same element_size");
	}

	rust_ivec_free(other);
	other->capacity = other->length = this->length;
	rust_ivec_reserve_exact(other, other->capacity);
	memcpy(other->ptr, this->ptr, this->length * this->element_size);
}

/*
 * dest MUST already be initialized, this function will destroy anything
 * that already exists in dest
 */
void rust_ivec_move(void* self, void* dest) {
	rawivec_t *this = self;
	rawivec_t *other = dest;
	if (this->element_size != other->element_size) {
		BUG("both ivec instances must have the same element_size");
	}

	rust_ivec_free(other);
	other->ptr = this->ptr;
	this->ptr = NULL;

	other->length = this->length;
	this->length = 0;

	other->capacity = this->capacity;
	this->capacity = 0;
}


void rust_ivec_free(void* self) {
	rawivec_t *this = self;
	free(this->ptr);
	this->ptr = NULL;
	this->capacity = 0;
	this->length = 0;
	// don't modify self->element_size
}
