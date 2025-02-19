#![allow(non_snake_case)]

use crate::xdiff::{XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE, XDF_IGNORE_WHITESPACE_AT_EOL, XDF_IGNORE_WHITESPACE_CHANGE, XDF_WHITESPACE_FLAGS};

pub(crate) fn XDL_ISSPACE(v: u8) -> bool {
    match v {
        b'\t' | b'\n' | b'\r' | b' ' => true,
        _ => false,
    }
}


fn xdl_hash_record_with_whitespace(slice: &[u8], flags: u64) -> (u64, usize) {
	let mut hash = 5381u64;
	let cr_at_eol_only = (flags & XDF_WHITESPACE_FLAGS) == XDF_IGNORE_CR_AT_EOL;

	let mut range = 0..slice.len();
	while range.start < range.end && slice[range.start] != b'\n' {
		if cr_at_eol_only {
			/* do not ignore CR at the end of an incomplete line */
			if slice[range.start] == b'\r' && range.start + 1 < range.end && slice[range.start + 1] == b'\n' {
				continue;
			}
		} else if XDL_ISSPACE(slice[range.start]) {
			let mut ptr2 = range.start;
			let at_eol: bool;
			while range.start + 1 < range.end && XDL_ISSPACE(slice[range.start + 1]) && slice[range.start + 1] != b'\n' {
				range.start += 1;
			}
			at_eol = range.end <= range.start + 1 || slice[range.start + 1] == b'\n';
			if (flags & XDF_IGNORE_WHITESPACE) != 0 {
				/* already handled */
			} else if (flags & XDF_IGNORE_WHITESPACE_CHANGE) != 0 && !at_eol {
				// hash += hash << 5;
				// hash ^= b' ' as u64;
				hash = hash.overflowing_mul(33).0 ^ b' ' as u64;
			} else if (flags & XDF_IGNORE_WHITESPACE_AT_EOL) != 0 && !at_eol {
				while ptr2 != range.start + 1 {
					// hash += hash << 5;
					// hash ^= *ptr2 as u64;
					hash = hash.overflowing_mul(33).0 ^ slice[ptr2] as u64;
					ptr2 += 1;
				}
			}
			range.start += 1;
			continue;
		}
		// hash += hash << 5;
		// hash ^= *ptr as u64;
		hash = hash.overflowing_mul(33).0 ^ slice[range.start] as u64;
		range.start += 1;
	}
	let with_eol = if range.start < range.end { range.start + 1 } else { range.start };

	(hash, with_eol)
}

pub(crate) fn xdl_hash_record(slice: &[u8], flags: u64) -> (u64, usize) {
	let mut hash = 5381u64;

	if (flags & XDF_WHITESPACE_FLAGS) != 0 {
		return xdl_hash_record_with_whitespace(slice, flags);
    }

	let mut range = 0..slice.len();
	while range.start < range.end {
		if slice[range.start] == b'\n' {
			break;
		}
		hash = hash.overflowing_mul(33).0 ^ slice[range.start] as u64;
		range.start += 1;
	}
	let with_eol = if range.start < range.end { range.start + 1 } else { range.start };

	(hash, with_eol)
}

#[cfg(test)]
mod tests {
	use crate::xutils::xdl_hash_record;

	#[test]
	fn test_xdl_hash_record() {
		let file = "This is\nsome text for \n xdl_hash_record() to \r\nchew on.";
		let slice = file.as_bytes();
		unsafe {
			// let mut data = slice.as_ptr();
			// let top = data.add(slice.len());
			let (line_hash, with_eol) = xdl_hash_record(slice, 0);
			assert_ne!(0, line_hash);
		}
	}

}

