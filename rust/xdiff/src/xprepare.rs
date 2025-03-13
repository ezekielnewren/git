use crate::xtypes::xdfile;
use crate::xdiff::{mmfile, XDF_IGNORE_CR_AT_EOL};
use crate::xutils::LineReader;

#[no_mangle]
unsafe extern "C" fn xdl_file_prepare(mf: *const mmfile, flags: u64, file: *mut xdfile) {
    let mf = mmfile::from_raw(mf);
    let file = xdfile::from_raw_mut(file, true);

    for record in LineReader::new(mf) {
        file.record.push(record);
    }
    file.record.shrink_to_fit();

    if (flags & XDF_IGNORE_CR_AT_EOL) != 0 {
        for rec in file.record.as_mut_slice() {
            if rec.size_no_eol > 0 && rec.as_ref()[rec.size_no_eol - 1] == b'\r' {
                rec.size_no_eol -= 1;
            }
        }
    }

}





