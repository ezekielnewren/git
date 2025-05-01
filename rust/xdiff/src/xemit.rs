use crate::xdiff::{xdemitcb, xdemitconf};
use crate::xdiffi::xdchange;
use crate::xtypes::xdpair;

pub(crate) type emit_func_t = unsafe extern "C" fn(pair: *mut xdpair, xscr: *mut xdchange, ecb: *const xdemitcb,
			   xecfg: *const xdemitconf) -> i32;
