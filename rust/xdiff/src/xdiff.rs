

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
pub const XDF_IGNORE_WHITESPACE_WITHIN: u64 =
	XDF_IGNORE_WHITESPACE |
	XDF_IGNORE_WHITESPACE_CHANGE |
	XDF_IGNORE_WHITESPACE_AT_EOL;
pub const XDF_WHITESPACE_FLAGS: u64 =
	XDF_IGNORE_WHITESPACE_WITHIN |
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
pub const XDL_MERGE_DIFF3: usize = 1;
pub const XDL_MERGE_ZEALOUS_DIFF3: usize = 2;


#[repr(C)]
pub struct mmfile {
    pub ptr: *const libc::c_char,
    pub size: libc::c_long,
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

