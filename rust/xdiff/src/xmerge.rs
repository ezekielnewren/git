use std::io::repeat;
use interop::ivec::IVec;
use crate::xdiff::{DEFAULT_CONFLICT_MARKER_SIZE, XDL_MERGE_DIFF3, XDL_MERGE_ZEALOUS_DIFF3};
use crate::xtypes::{xd3way, xrecord};

#[repr(C)]
struct xdmerge {
	next: *mut xdmerge,
	/*
	 * 0 = conflict,
	 * 1 = no conflict, take first,
	 * 2 = no conflict, take second.
	 * 3 = no conflict, take both.
	 */
	mode: u8,
	/*
	 * These point at the respective postimages.  E.g. <i1,chg1> is
	 * how side #1 wants to change the common ancestor; if there is no
	 * overlap, lines before i1 in the postimage of side #1 appear
	 * in the merge result as a region touched by neither side.
	 */
	i1: usize,
    i2: usize,
	chg1: usize,
    chg2: usize,
	/*
	 * These point at the preimage; of course there is just one
	 * preimage, that is from the shared common ancestor.
	 */
	i0: usize,
	chg0: usize,
}


impl xdmerge {

	unsafe fn from_raw_mut<'a>(m: *mut xdmerge) -> &'a mut xdmerge {
		if m.is_null() {
			panic!("null pointer");
		}

		&mut *m
	}

}



#[no_mangle]
unsafe extern "C" fn xdl_cleanup_merge(mut c: *mut xdmerge) -> usize {
	let mut count = 0;
    while !c.is_null() {
		if (*c).mode == 0 {
			count += 1;
        }
		let next_c = (*c).next;
		libc::free(c as *mut libc::c_void);
        c = next_c;
	}

	count
}


#[no_mangle]
unsafe extern "C" fn xdl_merge_lines_equal(three_way: *mut xd3way, i1: usize, i2: usize, line_count: usize) -> bool {
	let three_way = xd3way::from_raw_mut(three_way);

	for i in 0..line_count {
		let mph1 = three_way.side1.minimal_perfect_hash[i1 + i];
		let mph2 = three_way.side2.minimal_perfect_hash[i2 + i];
		if mph1 != mph2 {
			return false;
		}
	}

	true
}


#[no_mangle]
unsafe extern "C" fn xdl_recs_copy(record: *const IVec<xrecord>, off: usize, count: usize, needs_cr: bool, add_nl: bool, buffer: *mut IVec<u8>) {
	let record = IVec::from_raw(record);
	let buffer = IVec::from_raw_mut(buffer);

	if count < 1 {
		return;
	}

	for i in 0..count {
		let line = record[off + i].as_ref();
		buffer.extend_from_slice(line);
	}
	if add_nl {
		let slice = record[off + count - 1].as_ref();
		if slice.len() == 0 || slice[slice.len() - 1] != b'\n' {
			if needs_cr {
				buffer.push(b'\r');
			}
			buffer.push(b'\n');
		}
	}
}


/*
 * Returns 1 if the i'th line ends in CR/LF (if it is the last line and
 * has no eol, the preceding line, if any), 0 if it ends in LF-only, and
 * -1 if the line ending cannot be determined.
 */
#[no_mangle]
unsafe extern "C" fn is_eol_crlf(record: *mut IVec<xrecord>, i: usize) -> i32 {
	let record: &mut IVec<xrecord> = IVec::from_raw_mut(record);
	if record.len() == 0 {
		/* Cannot determine eol style from empty file */
		return -1;
	}

	let mut line: &[u8] = record[i].as_ref();
	if i + 1 < record.len() {
		/* All lines before the last *must* end in LF */
		return (line.len() > 1 && line[line.len() - 2] == b'\r') as i32;
	}
	if line.len() > 0 && line[line.len() - 1] == b'\n' {
		/* Last line; ends in LF; Is it CR/LF? */
		return (line.len() > 1 && line[line.len() - 2] == b'\r') as i32;
	}
	if i == 0 {
		/* The only line has no eol */
		return -1;
	}
	/* Determine eol from second-to-last line */
	line = record[i - 1].as_ref();
	(line.len() > 1 && line[line.len() - 2] == b'\r') as i32
}


#[no_mangle]
unsafe extern "C" fn is_cr_needed(three_way: *mut xd3way, m: *mut xdmerge) -> bool {
	let three_way = xd3way::from_raw_mut(three_way);
	let m = xdmerge::from_raw_mut(m);

	/* Match post-images' preceding, or first, lines' end-of-line style */
	let mut result = is_eol_crlf(&mut three_way.side1.record, m.i1.checked_sub(1).unwrap_or(0));
	if result != 0 {
		result = is_eol_crlf(&mut three_way.side2.record, m.i2.checked_sub(1).unwrap_or(0));
	}
	/* Look at pre-image's first line, unless we already settled on LF */
	if result != 0 {
		result = is_eol_crlf(&mut three_way.base.record, 0);
	}
	/* If still undecided, use LF-only */
	result > 0
}


#[no_mangle]
unsafe extern "C" fn fill_conflict_hunk(three_way: *mut xd3way,
										name1: *const u8,
										name2: *const u8,
										name3: *const u8,
										i: usize, style: u64,
										m: *mut xdmerge, buffer: *mut IVec<u8>, mut marker_size: usize) {
	let three_way = xd3way::from_raw_mut(three_way);
	let m = xdmerge::from_raw_mut(m);
	let buffer = IVec::from_raw_mut(buffer);

	let to_slice = |v: *const u8| {
		if v.is_null() {
			None
		} else {
			let size = libc::strlen(v as *const libc::c_char) as usize;
			Some(std::slice::from_raw_parts(v, size))
		}
	};

	let name1 = to_slice(name1);
	let name2 = to_slice(name2);
	let name3 = to_slice(name3);
	let needs_cr = is_cr_needed(three_way, m);

	if marker_size == 0 {
		marker_size = DEFAULT_CONFLICT_MARKER_SIZE;
	}

	/* Before conflicting part */
	xdl_recs_copy(&mut three_way.side1.record, i, m.i1 - i, false, false, buffer);

	buffer.extend(std::iter::repeat(b'<').take(marker_size));
	if let Some(name) = name1 {
		buffer.push(b' ');
		buffer.extend_from_slice(name);
	}
	if needs_cr {
		buffer.push(b'\r');
	}
	buffer.push(b'\n');

	/* Postimage from side #1 */
	xdl_recs_copy(&three_way.side1.record, m.i1, m.chg1, needs_cr, true, buffer);

	if style == XDL_MERGE_DIFF3 || style == XDL_MERGE_ZEALOUS_DIFF3 {
		/* Shared preimage */
		buffer.extend(std::iter::repeat(b'|').take(marker_size));
		if let Some(name) = name3 {
			buffer.push(b' ');
			buffer.extend_from_slice(name);
		}
		if needs_cr {
			buffer.push(b'\r');
		}
		buffer.push(b'\n');
		xdl_recs_copy(&three_way.base.record, m.i0, m.chg0, needs_cr, true, buffer);
	}

	buffer.extend(std::iter::repeat(b'=').take(marker_size));
	if needs_cr {
		buffer.push(b'\r');
	}
	buffer.push(b'\n');

	/* Postimage from side #2 */
	xdl_recs_copy(&three_way.side2.record, m.i2, m.chg2, needs_cr, true, buffer);

	buffer.extend(std::iter::repeat(b'>').take(marker_size));
	if let Some(name) = name2 {
		buffer.push(b' ');
		buffer.extend_from_slice(name);
	}
	if needs_cr {
		buffer.push(b'\r');
	}
	buffer.push(b'\n');
}
