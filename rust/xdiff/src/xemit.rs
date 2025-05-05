use crate::xdiff::{xdemitcb, xdemitconf};
use crate::xdiffi::xdchange;
use crate::xtypes::xdpair;

pub(crate) type emit_func_t = unsafe extern "C" fn(pair: *mut xdpair, xscr: *mut xdchange, ecb: *const xdemitcb,
			   xecfg: *const xdemitconf) -> i32;


/*
 * Starting at the passed change atom, find the latest change atom to be included
 * inside the differential hunk according to the specified configuration.
 * Also advance xscr if the first changes must be discarded.
 */
#[no_mangle]
pub(crate) unsafe extern "C" fn xdl_get_hunk(xscr: *mut *mut xdchange, xecfg: *const xdemitconf) -> *mut xdchange {
	let max_common = 2 * (*xecfg).ctxlen + (*xecfg).interhunkctxlen;
	let max_ignorable = (*xecfg).ctxlen;
	let mut ignored = 0; /* number of ignored blank lines */

	/* remove ignorable changes that are too far before other changes */
	let mut xchp = *xscr;
	let mut xch: *mut xdchange = std::ptr::null_mut();
	while !xchp.is_null() && (*xchp).ignore {
		xch = (*xchp).next;

		if xch.is_null() || (*xch).i1 - ((*xchp).i1 + (*xchp).chg1) >= max_ignorable {
			*xscr = xch;
		}
		
		xchp = (*xchp).next;
	}

	if (*xscr).is_null() {
		return std::ptr::null_mut();
	}

	let mut lxch = *xscr;
	xchp = *xscr;
	xch = (*xchp).next;
	while !xch.is_null() {
		let distance = (*xch).i1 - ((*xchp).i1 + (*xchp).chg1);
		if distance > max_common {
			break;
		}

		if distance < max_ignorable && (!(*xch).ignore || lxch == xchp) {
			lxch = xch;
			ignored = 0;
		} else if distance < max_ignorable && (*xch).ignore {
			ignored += (*xch).chg2;
		} else if lxch != xchp && (*xch).i1 + ignored - ((*lxch).i1 + (*lxch).chg1) > max_common {
			break;
		} else if !(*xch).ignore {
			lxch = xch;
			ignored = 0;
		} else {
			ignored += (*xch).chg2;
		}
		
		xchp = xch;
		xch = (*xch).next;
	}

	lxch
}
