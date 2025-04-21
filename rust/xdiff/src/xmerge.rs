use std::io::repeat;
use std::marker::PhantomData;
use interop::ivec::IVec;
use interop::xmalloc;
use crate::xdiff::{xpparam_t, DEFAULT_CONFLICT_MARKER_SIZE, XDL_MERGE_DIFF3, XDL_MERGE_ZEALOUS_DIFF3};
use crate::xdiffi::{xdchange, xdl_build_script, xdl_change_compact, xdl_free_script};
use crate::xdl_do_diff;
use crate::xprepare::safe_2way_slice;
use crate::xtypes::{xd2way, xd3way, xdpair, xrecord, FileContext};
use crate::xutils::XDL_ISALNUM;

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


#[no_mangle]
unsafe extern "C" fn xdl_fill_merge_buffer(three_way: *mut xd3way,
										   name1: *const u8,
										   name2: *const u8,
										   ancestor_name: *const u8,
										   favor: u8,
										   mut merge: *mut xdmerge, buffer: *mut IVec<u8>, style: u64,
										   marker_size: usize
) {
	let three_way = xd3way::from_raw_mut(three_way);
	let buffer = IVec::from_raw_mut(buffer);

	let mut i = 0;
	while !merge.is_null() {
		let m = &mut *merge;
		if favor != 0 && m.mode == 0 {
			m.mode = favor;
		}

		if m.mode == 0 {
			fill_conflict_hunk(three_way, name1, name2,
						  ancestor_name,
						  i, style, m, buffer,
						  marker_size);
		} else if (m.mode & 3) != 0 {
			/* Before conflicting part */
			xdl_recs_copy(&three_way.side1.record, i, m.i1 - i, false, false, buffer);
			/* Postimage from side #1 */
			if (m.mode & 1) != 0 {
				let needs_cr = is_cr_needed(three_way, m);

				xdl_recs_copy(&three_way.side1.record, m.i1, m.chg1, needs_cr, (m.mode & 2) != 0, buffer);
			}
			/* Postimage from side #2 */
			if (m.mode & 2) != 0 {
				xdl_recs_copy(&three_way.side2.record, m.i2, m.chg2, false, false, buffer);
			}
		} else {
			merge = m.next;
			continue;
		}
		i = m.i1 + m.chg1;

		merge = m.next;
	}
	xdl_recs_copy(&three_way.side1.record, i, three_way.side1.record.len() - i, false, false, buffer);
}

/*
 * Remove any common lines from the beginning and end of the conflicted region.
 */
#[no_mangle]
unsafe extern "C" fn xdl_refine_zdiff3_conflicts(three_way: *mut xd3way, mut merge: *mut xdmerge) {
	let three_way = xd3way::from_raw_mut(three_way);

	let mph1 = three_way.side1.minimal_perfect_hash.as_slice();
	let mph2 = three_way.side2.minimal_perfect_hash.as_slice();

	while !merge.is_null() {
		let m = xdmerge::from_raw_mut(merge);
		/* let's handle just the conflicts */
		if m.mode != 0 {
			merge = m.next;
			continue;
		}

		while m.chg1 != 0 && m.chg2 != 0 && mph1[m.i1] == mph2[m.i2] {
			m.chg1 -= 1;
			m.chg2 -= 1;
			m.i1 += 1;
			m.i2 += 1;
		}
		while m.chg1 != 0 && m.chg2 != 0 && mph1[m.i1 + m.chg1 - 1] == mph2[m.i2 + m.chg2 - 1] {
			m.chg1 -= 1;
			m.chg2 -= 1;
		}

		merge = m.next;
	}
}


struct xdmerge_iter<'a> {
	cur: *mut xdmerge,
	_marker: PhantomData<&'a mut xdmerge>,
}

impl<'a> Iterator for xdmerge_iter<'a> {
	type Item = &'a mut xdmerge;

	fn next(&mut self) -> Option<Self::Item> {
		if self.cur.is_null() {
			return None;
		}

		let t = self.cur;
		self.cur = unsafe { (*self.cur).next };
		Some(unsafe { &mut *t })
	}
}

impl<'a> xdmerge_iter<'a> {
	fn new(start: *mut xdmerge) -> Self {
		Self {
			cur: start,
			_marker: PhantomData,
		}
	}

	fn set(&mut self, immediate: *mut xdmerge) -> &mut xdmerge {
		self.cur = immediate;
		unsafe { &mut *self.cur }
	}

}



/*
 * Sometimes, changes are not quite identical, but differ in only a few
 * lines. Try hard to show only these few lines as conflicting.
 */
#[no_mangle]
unsafe extern "C" fn xdl_refine_conflicts(three_way: *mut xd3way, merge: *mut xdmerge, xpp: *const xpparam_t) -> i32 {
	let three_way = xd3way::from_raw_mut(three_way);
	let xpp = &*xpp;

	let mut it = xdmerge_iter::new(merge);
	while let Some(mut m) = it.next() {

		let mut two_way = xd2way::default();
		let mut xscr: *mut xdchange = std::ptr::null_mut();

		let i1 = m.i1;
		let i2 = m.i2;

		/* let's handle just the conflicts */
		if m.mode != 0 {
			continue;
		}

		/* no sense refining a conflict when one side is empty */
		if m.chg1 == 0 || m.chg2 == 0 {
			continue;
		}

		let range1 = m.i1..m.i1 + m.chg1;
		let range2 = m.i2..m.i2 + m.chg2;

		let lhs = FileContext::new(&mut three_way.pair1.rhs);
		let rhs = FileContext::new(&mut three_way.pair2.rhs);

		safe_2way_slice(&lhs, range1, &rhs, range2, three_way.minimal_perfect_hash_size, &mut two_way);
		if xdl_do_diff(xpp, &mut two_way.pair) < 0 {
			return -1;
		}

		xdl_change_compact(&mut two_way.pair.lhs, &mut two_way.pair.rhs, xpp.flags);
		xdl_change_compact(&mut two_way.pair.rhs, &mut two_way.pair.lhs, xpp.flags);
		xdl_build_script(&mut two_way.pair, &mut xscr);

		let start = xscr;
		m.i1 = (*xscr).i1 as usize + i1;
		m.chg1 = (*xscr).chg1 as usize;
		m.i2 = (*xscr).i2 as usize + i2;
		m.chg2 = (*xscr).chg2 as usize;
		while !(*xscr).next.is_null() {
			let m2 = unsafe { &mut *(xmalloc(size_of::<xdmerge>()) as *mut xdmerge) };

			xscr = (*xscr).next;
			m2.next = m.next;
			m.next = m2;
			m = it.set(m2); // i.e. m = m2;

			m.mode = 0;
			m.i1 = (*xscr).i1 as usize + i1;
			m.chg1 = (*xscr).chg1 as usize;
			m.i2 = (*xscr).i2 as usize + i2;
			m.chg2 = (*xscr).chg2 as usize;
		}
		xdl_free_script(start);
	}

	0
}


#[no_mangle]
unsafe extern "C" fn lines_contain_alnum(pair: *mut xdpair, i: usize, chg: usize) -> bool {
	let pair = xdpair::from_raw_mut(pair);

	for record in &(*pair.rhs.record).as_slice()[i..i + chg] {
		if record.as_ref().iter().any(|c| XDL_ISALNUM(*c)) {
			return true;
		}
	}

	false
}


/*
 * This function merges m and m->next, marking everything between those hunks
 * as conflicting, too.
 */
#[no_mangle]
unsafe extern "C" fn xdl_merge_two_conflicts(m: *mut xdmerge) {
	let m = &mut *m;

	let next_m = &mut *m.next;
	m.chg1 = next_m.i1 + next_m.chg1 - m.i1;
	m.chg2 = next_m.i2 + next_m.chg2 - m.i2;
	m.next = next_m.next;

	libc::free(next_m as *mut xdmerge as *mut libc::c_void);
}


/*
 * If there are less than 3 non-conflicting lines between conflicts,
 * it appears simpler -- because it takes up less (or as many) lines --
 * if the lines are moved into the conflicts.
 */
#[no_mangle]
unsafe extern "C" fn xdl_simplify_non_conflicts(pair1: *mut xdpair, m: *mut xdmerge,
				      simplify_if_no_alnum: bool) -> i32 {
	let mut result: i32 = 0;

	if m.is_null() {
		return result;
	}

	let mut m = &mut *m;
	loop {
		if m.next.is_null() {
			return result;
		}
		let next_m = unsafe { &mut *m.next };

		let begin = m.i1 + m.chg1;
		let end = next_m.i1;

		if m.mode != 0 || next_m.mode != 0 ||
			(end - begin > 3 &&
			(!simplify_if_no_alnum ||
			lines_contain_alnum(pair1, begin, end - begin))) {
			m = next_m;
		} else {
			result += 1;
			xdl_merge_two_conflicts(m);
		}
	}
}


#[no_mangle]
unsafe extern "C" fn xdl_append_merge(merge: *mut *mut xdmerge, mode: u8,
			    i0: usize, chg0: usize,
			    i1: usize, chg1: usize,
			    i2: usize, chg2: usize
) -> i32 {
	let mut m = *merge;
	if !m.is_null() && (i1 <= (*m).i1 + (*m).chg1 || i2 <= (*m).i2 + (*m).chg2) {
		if (mode != (*m).mode) {
			(*m).mode = 0;
		}
		(*m).chg0 = i0 + chg0 - (*m).i0;
		(*m).chg1 = i1 + chg1 - (*m).i1;
		(*m).chg2 = i2 + chg2 - (*m).i2;
	} else {
		m = xmalloc(size_of::<xdmerge>()) as *mut xdmerge;
		(*m).next = std::ptr::null_mut();
		(*m).mode = mode;
		(*m).i0 = i0;
		(*m).chg0 = chg0;
		(*m).i1 = i1;
		(*m).chg1 = chg1;
		(*m).i2 = i2;
		(*m).chg2 = chg2;
		if !(*merge).is_null() {
			(*(*merge)).next = m;
		}
		*merge = m;
	}

	0
}

