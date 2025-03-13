use interop::ivec::IVec;
use crate::xrecord::xrecord;

#[repr(C)]
#[derive(Default)]
pub struct xdfile {
    pub minimal_perfect_hash: IVec<u64>,
    pub record: IVec<xrecord>,
}



impl xdfile {

    pub unsafe fn from_raw_mut<'a>(file: *mut xdfile, do_init: bool) -> &'a mut xdfile {
        if do_init {
            std::ptr::write(file, xdfile::default());
        }

        &mut *file
    }

}



