use interop::ivec::IVec;
use crate::xrecord::xrecord;

pub struct xdfile {
    pub minimal_perfect_hash: IVec<u64>,
    pub record: IVec<xrecord>,
}

