#![allow(non_snake_case)]

use crate::xdiff::{XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE, XDF_IGNORE_WHITESPACE_AT_EOL, XDF_IGNORE_WHITESPACE_CHANGE, XDF_WHITESPACE_FLAGS};

pub(crate) fn XDL_ISSPACE(v: u8) -> bool {
    match v {
        b'\t' | b'\n' | b'\r' | b' ' => true,
        _ => false,
    }
}


unsafe fn xdl_hash_record_with_whitespace(data: &mut *const u8, top: *const u8, flags: u64) -> u64 {
	let mut hash = 5381u64;
	let mut ptr = *data;
	let cr_at_eol_only = (flags & XDF_WHITESPACE_FLAGS) == XDF_IGNORE_CR_AT_EOL;

	while ptr < top && *ptr != b'\n' {
		if cr_at_eol_only {
			/* do not ignore CR at the end of an incomplete line */
			if *ptr == b'\r' && (ptr.add(1) < top && *ptr.add(1) == b'\n') {
				continue;
			}
		} else if XDL_ISSPACE(*ptr) {
			let mut ptr2 = ptr;
			let at_eol: bool;
			while ptr.add(1) < top && XDL_ISSPACE(*ptr.add(1)) && *ptr.add(1) != b'\n' {
				ptr = ptr.add(1);
			}
			at_eol = top <= ptr.add(1) || *ptr.add(1) == b'\n';
			if (flags & XDF_IGNORE_WHITESPACE) != 0 {
				/* already handled */
			} else if (flags & XDF_IGNORE_WHITESPACE_CHANGE) != 0 && !at_eol {
				// hash += hash << 5;
				// hash ^= b' ' as u64;
				hash = hash.overflowing_mul(33).0 ^ b' ' as u64;
			} else if (flags & XDF_IGNORE_WHITESPACE_AT_EOL) != 0 && !at_eol {
				while ptr2 != ptr.add(1) {
					// hash += hash << 5;
					// hash ^= *ptr2 as u64;
					hash = hash.overflowing_mul(33).0 ^ *ptr2 as u64;
					ptr2 = ptr2.add(1);
				}
			}
			ptr = ptr.add(1);
			continue;
		}
		// hash += hash << 5;
		// hash ^= *ptr as u64;
		hash = hash.overflowing_mul(33).0 ^ *ptr as u64;
		ptr = ptr.add(1);
	}
	*data = if ptr < top { ptr.add(1) } else { ptr };

	hash
}

pub(crate) unsafe fn xdl_hash_record(data: &mut *const u8, top: *const u8, flags: u64) -> u64 {
	let mut hash = 5381u64;
	let mut ptr = *data;

	if (flags & XDF_WHITESPACE_FLAGS) != 0 {
		return xdl_hash_record_with_whitespace(data, top, flags);
    }

	while ptr < top && *ptr != b'\n' {
		hash = hash.overflowing_mul(33).0 ^ *ptr as u64;
		// hash += hash << 5;
		// hash ^= *ptr as u64;

		ptr = ptr.add(1);
	}
	*data = if ptr < top { ptr.add(1) } else { ptr };

	hash
}

#[cfg(test)]
mod tests {
	use crate::xutils::xdl_hash_record;

	#[test]
	fn test_xdl_hash_record() {
		let file = "This is\nsome text for \n xdl_hash_record() to \r\nchew on.";
		let slice = file.as_bytes();
		unsafe {
			let mut data = slice.as_ptr();
			let top = data.add(slice.len());
			let line_hash = xdl_hash_record(&mut data, top, 0);
			assert_ne!(0, line_hash);
		}
	}

}

