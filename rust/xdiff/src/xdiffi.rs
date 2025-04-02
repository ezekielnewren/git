

#[repr(C)]
pub(crate) struct xdpsplit {
    pub(crate) i1: isize,
    pub(crate) i2: isize,
    pub(crate) min_lo: bool,
    pub(crate) min_hi: bool,
}

#[repr(C)]
pub(crate) struct xdalgoenv {
    pub(crate) mxcost: isize,
    pub(crate) snake_cnt: isize,
    pub(crate) heur_min: isize,
}

#[repr(C)]
pub(crate) struct xdchange {
    pub(crate) next: *mut xdchange,
    pub(crate) i1: isize,
    pub(crate) i2: isize,
    pub(crate) chg1: isize,
    pub(crate) chg2: isize,
    pub(crate) ignore: bool,
}

