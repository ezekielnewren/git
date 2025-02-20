use crate::xdfenv::xdfile_t;
use crate::xdiff::{XDF_HISTOGRAM_DIFF, XDF_PATIENCE_DIFF};
use crate::xrecord::xrecord_t;
use crate::xutils::{LineReader};


pub(crate) fn xdl_prepare_ctx(mf: &[u8], xdf: &mut xdfile_t, flags: u64) {
    for (line, eol_len) in LineReader::new(mf) {
        let rec = xrecord_t::new(line, eol_len, flags);
        xdf.record.push(rec);
    }

    xdf.rchg_vec.resize(xdf.record.len() + 2, 0);

    if (flags & (XDF_PATIENCE_DIFF | XDF_HISTOGRAM_DIFF)) == 0 {
        xdf.rindex.reserve_exact(xdf.record.len() + 1);
        xdf.hash.reserve_exact(xdf.record.len() + 1);
    }

    xdf.rchg = unsafe { xdf.rchg_vec.as_mut_ptr().add(1) };
    xdf.dstart = 0;
    xdf.dend = xdf.record.len().wrapping_sub(1) as isize;
}


#[cfg(test)]
mod tests {

    #[test]
    fn test_prepare() {

    }

}


