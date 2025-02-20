use libc::memset;
use interop::ivec::IVec;
use crate::xdiff::{XDF_HISTOGRAM_DIFF, XDF_PATIENCE_DIFF};
use crate::xprepare::xdl_prepare_ctx;
use crate::xutils::xdl_hash_record;

pub(crate) mod xutils;
pub(crate) mod xdiff;
pub(crate) mod xprepare;
pub(crate) mod xrecord;
pub(crate) mod xtypes;

#[repr(C)]
pub struct mmfile_t {
    pub ptr: *const libc::c_char,
    pub size: libc::c_long,
}


#[repr(C)]
pub struct xrecord_t {
    pub ptr: *const u8,
    pub size: usize,
    pub hash: u64,
    pub flags: u64,
}


#[repr(C)]
pub struct xdfile_t {
    pub record: IVec<xrecord_t>,
    pub rchg_vec: IVec<u8>,
    pub rindex: IVec<isize>,
    pub hash: IVec<u64>,
    pub dstart: isize,
    pub dend: isize,
    pub rchg: *mut u8,
}

impl Default for xdfile_t {
    fn default() -> Self {
        Self {
            record: IVec::new(),
            rchg_vec: IVec::new(),
            rindex: IVec::new(),
            hash: IVec::new(),
            dstart: 0,
            dend: 0,
            rchg: std::ptr::null_mut(),
        }
    }
}

#[no_mangle]
unsafe extern "C" fn rust_xdl_hash_record(
    data: *mut *const libc::c_char,
    top: *const libc::c_char,
    flags: libc::c_long
) -> u64 {
    let slice: &[u8] = unsafe {
        std::slice::from_raw_parts(*data as *const u8, top.sub(*data as usize) as usize)
    };
    let (line_hash, with_eol) = xdl_hash_record(slice, flags as u64);
    *data = (*data).add(with_eol);
    line_hash
}


#[no_mangle]
unsafe extern "C" fn rust_readlines(_mf: *const mmfile_t, _xdf: *mut xdfile_t, flags: u64) {
    if _mf.is_null() {
        panic!("null pointer");
    }
    let mf = std::slice::from_raw_parts((*_mf).ptr as *const u8, (*_mf).size as usize);

    if _xdf.is_null() {
        panic!("null pointer");
    }
    std::ptr::write(_xdf, xdfile_t::default());
    let xdf = &mut *_xdf;


    let mut off = 0;
    while off < mf.len() {
        let (line_hash, with_eol) = xdl_hash_record(&mf[off..], flags);
        // if with_eol == 0 {
        //     break;
        // }
        let crec = xrecord_t {
            ptr: &mf[off],
            size: with_eol,
            hash: line_hash,
            flags,
        };
        xdf.record.push(crec);
        off += with_eol;
    }

}

#[no_mangle]
unsafe extern "C" fn rust_init(_mf: *const mmfile_t, _xdf: *mut xdfile_t, flags: u64) {
    std::ptr::write(_xdf, xdfile_t::default());
}

#[no_mangle]
unsafe extern "C" fn rust_xdl_prepare_ctx(_mf: *const mmfile_t, _xdf: *mut xdfile_t, flags: u64) -> i32 {
    if _mf.is_null() {
        panic!("null pointer");
    }
	let mf = std::slice::from_raw_parts((*_mf).ptr as *const u8, (*_mf).size as usize);

    if _xdf.is_null() {
        panic!("null pointer");
    }
    std::ptr::write(_xdf, xdfile_t::default());
	let xdf = &mut *_xdf;

	xdl_prepare_ctx(mf, xdf, flags);

    0
}

