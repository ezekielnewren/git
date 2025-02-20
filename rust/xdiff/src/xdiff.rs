#![allow(non_camel_case_types)]
#![allow(non_snake_case)]


pub const INVALID_INDEX: usize = usize::MAX;
pub const XDF_NEED_MINIMAL: u64 = 1 << 0;

pub const XDF_IGNORE_WHITESPACE: u64 = 1 << 1;
pub const XDF_IGNORE_WHITESPACE_CHANGE: u64 = 1 << 2;
pub const XDF_IGNORE_WHITESPACE_AT_EOL: u64 = 1 << 3;
pub const XDF_IGNORE_CR_AT_EOL: u64 = 1 << 4;
pub const XDF_WHITESPACE_FLAGS: u64 =
	XDF_IGNORE_WHITESPACE |
	XDF_IGNORE_WHITESPACE_CHANGE |
	XDF_IGNORE_WHITESPACE_AT_EOL |
	XDF_IGNORE_CR_AT_EOL;

pub const XDF_IGNORE_BLANK_LINES: u64 = 1 << 7;

pub const XDF_PATIENCE_DIFF: u64 = 1 << 14;
pub const XDF_HISTOGRAM_DIFF: u64 = 1 << 15;
pub const XDF_DIFF_ALGORITHM_MASK: u64 = XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF;
pub fn XDF_DIFF_ALG(x: u64) -> u64 {
	x & XDF_DIFF_ALGORITHM_MASK
}
pub const XDF_INDENT_HEURISTIC: u64 = 1 << 23;

pub const XDL_EMIT_FUNCNAMES: u64 = 1 << 0;
pub const XDL_EMIT_NO_HUNK_HDR: u64 = 1 << 1;
pub const XDL_EMIT_FUNCCONTEXT: u64 = 1 << 2;

// /* merge simplification levels */
pub const XDL_MERGE_MINIMAL: u64 = 0;
pub const XDL_MERGE_EAGER: u64 = 1;
pub const XDL_MERGE_ZEALOUS: u64 = 2;
pub const XDL_MERGE_ZEALOUS_ALNUM: u64 = 3;

// /* merge favor modes */
pub const XDL_MERGE_FAVOR_OURS: u64 = 1;
pub const XDL_MERGE_FAVOR_THEIRS: u64 = 2;
pub const XDL_MERGE_FAVOR_UNION: u64 = 3;

// /* merge output styles */
pub const XDL_MERGE_DIFF3: u64 = 1;
pub const XDL_MERGE_ZEALOUS_DIFF3: u64 = 2;


pub(crate) fn malloc_array<T>(size: usize) -> *mut T {
	if size == 0 {
		return std::ptr::null_mut();
	}
	unsafe {
		let t = libc::malloc(size * size_of::<T>());
		if t.is_null() {
			panic!("out of memory");
		}
		t as *mut T
	}
}

pub(crate) fn calloc_array<T>(size: usize) -> *mut T {
	if size == 0 {
		return std::ptr::null_mut();
	}
	unsafe {
		let t = libc::calloc(size, size_of::<T>());
		if t.is_null() {
			panic!("out of memory");
		}
		t as *mut T
	}
}

#[repr(C)]
pub struct mmfile_t {
	pub ptr: *mut libc::c_char,
	pub size: libc::c_long,
}

impl mmfile_t {
	pub unsafe fn as_ref(&self) -> &[u8] {
		if self.ptr.is_null() {
			&[]
		} else {
			std::slice::from_raw_parts(self.ptr as *mut u8, self.size as usize)
		}
	}

	pub unsafe fn from_raw<'a>(mf: *const Self) -> &'a [u8] {
		let ptr = (*mf).ptr as *const u8;
		let size = (*mf).size as usize;

		if ptr.is_null() {
			return &[];
		}
		#[cfg(debug_assertions)]
		{
			if size > isize::MAX as usize {
				panic!("mmfile_t is too big!");
			}
			if (mf as usize) % align_of::<Self>() != 0 {
				panic!("misaligned mmfile_t pointer");
			}
		}
		std::slice::from_raw_parts(ptr , size)
	}

	pub unsafe fn malloc(size: usize) -> Self {
		Self {
			ptr: malloc_array::<u8>(size) as *mut libc::c_char,
			size: size as libc::c_long,
		}
	}

	pub unsafe fn calloc(size: usize) -> Self {
		Self {
			ptr: calloc_array::<u8>(size) as *mut libc::c_char,
			size: size as libc::c_long,
		}
	}

}


pub const DEFAULT_CONFLICT_MARKER_SIZE: u64 = 7;

