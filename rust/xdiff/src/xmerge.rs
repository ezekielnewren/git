

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

