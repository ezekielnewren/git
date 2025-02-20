use crate::xdiff::{XDF_HISTOGRAM_DIFF, XDF_PATIENCE_DIFF};
use crate::{xdfile_t, xrecord_t};
use crate::xutils::xdl_hash_record;


pub(crate) fn xdl_prepare_ctx(mf: &[u8], xdf: &mut xdfile_t, flags: u64) {
    let mut off = 0;
    while off < mf.len() {
        let (line_hash, with_eol) = xdl_hash_record(&mf[off..], flags);
        let crec = xrecord_t {
            ptr: &mf[off],
            size: with_eol,
            hash: line_hash,
            flags,
        };
        xdf.record.push(crec);
        off += with_eol;
    }

    xdf.rchg_vec.resize(xdf.record.len() + 2, 0);

    if (flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0 {
        xdf.rindex.reserve_exact(xdf.record.len() + 1);
        xdf.hash.reserve_exact(xdf.record.len() + 1);
    }

    xdf.rchg = unsafe { xdf.rchg_vec.as_mut_ptr().add(1) };
    xdf.dstart = 0;
    xdf.dend = (xdf.record.len() - 1) as isize;
}

