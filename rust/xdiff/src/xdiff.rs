

pub const INVALID_INDEX: usize = usize::MAX;
pub const LINE_SHIFT: usize = 1;
pub const SENTINEL: usize = 1;

pub const NO: u8 = 0;
pub const YES: u8 = 1;
pub const TOO_MANY: u8 = 2;

/* xpparm_t.flags */
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

pub const XDF_INDENT_HEURISTIC: u64 = 1 << 23;

/* xdemitconf_t.flags */
pub const XDL_EMIT_FUNCNAMES: u64 = 1 << 0;
pub const XDL_EMIT_NO_HUNK_HDR: u64 = 1 << 1;
pub const XDL_EMIT_FUNCCONTEXT: u64 = 1 << 2;

/* merge simplification levels */
pub const XDL_MERGE_MINIMAL: usize = 0;
pub const XDL_MERGE_EAGER: usize = 1;
pub const XDL_MERGE_ZEALOUS: usize = 2;
pub const XDL_MERGE_ZEALOUS_ALNUM: usize = 3;

/* merge favor modes */
pub const XDL_MERGE_FAVOR_OURS: usize = 1;
pub const XDL_MERGE_FAVOR_THEIRS: usize = 2;
pub const XDL_MERGE_FAVOR_UNION: usize = 3;

/* merge output styles */
pub const XDL_MERGE_DIFF3: u64 = 1;
pub const XDL_MERGE_ZEALOUS_DIFF3: u64 = 2;

pub const DEFAULT_CONFLICT_MARKER_SIZE: usize = 7;


pub(crate) type find_func_t = unsafe extern "C" fn(line: *const u8, line_len: isize, buffer: *mut u8, buffer_size: isize, _priv: *mut libc::c_void) -> isize;
pub(crate) type xdl_emit_hunk_consume_func_t = unsafe extern "C" fn(start_a: isize, count_a: isize, start_b: isize, count_b: isize, cb_data: *mut libc::c_void) -> i32;


#[repr(C)]
pub(crate) struct xdemitconf {
    pub(crate) ctxlen: isize,
    pub(crate) interhunkctxlen: isize,
    pub(crate) flags: u64,
    pub(crate) find_func: *const find_func_t,
    pub(crate) find_func_priv: *mut libc::c_void,
    pub(crate) hunk_func: *const std::ffi::c_void, // xdl_emit_hunk_consume_func_t,
}


#[repr(C)]
pub struct xpparam_t {
	pub flags: u64,

	/* -I<regex> */
    pub ignore_regex: *mut *mut libc::regex_t,
    pub ignore_regex_nr: usize,

	/* See Documentation/diff-options.adoc. */
    pub anchors: *mut *mut libc::c_char,
    pub anchors_nr: usize,
}

impl Default for xpparam_t {
    fn default() -> Self {
        Self {
            flags: 0,
            ignore_regex: std::ptr::null_mut(),
            ignore_regex_nr: 0,
            anchors: std::ptr::null_mut(),
            anchors_nr: 0,
        }
    }
}

#[repr(C)]
pub struct mmfile {
    pub ptr: *const libc::c_char,
    pub size: libc::c_long,
}

impl mmfile {
    pub(crate) fn from_slice(p0: &[u8]) -> Self {
        Self {
            ptr: p0.as_ptr() as *const libc::c_char,
            size: p0.len() as libc::c_long,
        }
    }
}

impl mmfile {

    pub unsafe fn from_raw<'a>(mf: *const mmfile) -> &'a [u8] {
        if (*mf).ptr.is_null() {
            &[]
        } else {
            std::slice::from_raw_parts((*mf).ptr as *const u8, (*mf).size as usize)
        }
    }

}

pub struct mmbuffer {
    pub ptr: *const libc::c_char,
    pub size: libc::c_long,
}


impl mmbuffer {
    pub(crate) fn from_slice(p0: &[u8]) -> Self {
        Self {
            ptr: p0.as_ptr() as *const libc::c_char,
            size: p0.len() as libc::c_long,
        }
    }
}

impl mmbuffer {

    pub unsafe fn from_raw<'a>(mf: *const mmbuffer) -> &'a [u8] {
        if (*mf).ptr.is_null() {
            &[]
        } else {
            std::slice::from_raw_parts((*mf).ptr as *const u8, (*mf).size as usize)
        }
    }

    pub unsafe fn from_raw_mut<'a>(mf: *mut mmbuffer) -> &'a mut [u8] {
        if (*mf).ptr.is_null() {
            &mut []
        } else {
            std::slice::from_raw_parts_mut((*mf).ptr as *mut u8, (*mf).size as usize)
        }
    }

}


type xdemitcb_out_hunk_func = unsafe extern "C" fn(p0: *mut libc::c_void,
old_begin: isize, old_nr: isize,
new_begin: isize, new_nr: isize,
func: *const u8, funclen: isize) -> i32;

type xdemitcb_out_line_func = unsafe extern "C" fn(p0: *mut libc::c_void, p1: *mut mmbuffer, p2: i32) -> i32;


#[repr(C)]
pub(crate) struct xdemitcb {
	pub(crate) private: *mut libc::c_void,
    out_hunk: *mut xdemitcb_out_hunk_func,
    out_line: *mut xdemitcb_out_line_func,
}



#[repr(C)]
pub(crate) struct xmparam {
    pub(crate) xpp: xpparam_t,
	pub(crate) marker_size: i32,
	pub(crate) level: i32,
	pub(crate) favor: i32,
	pub(crate) style: i32,
	pub(crate) ancestor: *const u8, /* label for orig */
	pub(crate) file1: *const u8,    /* label for mf1 */
	pub(crate) file2: *const u8,    /* label for mf2 */
}
