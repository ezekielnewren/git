use interop::ivec::IVec;
use crate::xdiff::LINE_SHIFT;
use crate::xtypes::{xdpair, FileContext};

#[repr(C)]
struct record {
	ptr: usize,
    cnt: usize,
	next: *mut record,
}

impl Default for record {
	fn default() -> Self {
		Self {
			ptr: 0,
			cnt: 0,
			next: std::ptr::null_mut(),
		}
	}
}


#[repr(C)]
struct histindex {
	record_storage: IVec<record>,
	record: IVec<*mut record>,
	line_map: IVec<*mut record>,
	next_ptrs: IVec<usize>,
	max_chain_length: usize,
	ptr_shift: usize,
	cnt: usize,
	has_common: bool,
}


#[repr(C)]
struct region {
	begin1: usize,
	end1: usize,
	begin2: usize,
	end2: usize,
}


#[no_mangle]
unsafe extern "C" fn scanA(index: *mut histindex, pair: *mut xdpair, line1: usize, count1: usize) -> i32 {
    let index = &mut *index;
    let pair = xdpair::from_raw_mut(pair);

    let lhs = FileContext::new(&mut pair.lhs);

	// for (usize ptr = line1 + count1 - 1; line1 <= ptr; ptr--) {
    for ptr in (line1..=line1 + count1 - 1).rev() {
		let mut continue_scan = false;
		let tbl_idx = lhs.minimal_perfect_hash[ptr - LINE_SHIFT] as usize;
		let rec_chain: *mut *mut record = &mut index.record[tbl_idx];
		let mut rec: *mut record = *rec_chain;

		let mut chain_len = 0;
		while !rec.is_null() {
			continue_scan = false;
			let mph1 = lhs.minimal_perfect_hash[(*rec).ptr - LINE_SHIFT];
			let mph2 = lhs.minimal_perfect_hash[ptr - LINE_SHIFT];
			if mph1 == mph2 {
				/*
				 * ptr is identical to another element. Insert
				 * it onto the front of the existing element
				 * chain.
				 */
				index.next_ptrs[ptr - index.ptr_shift] = (*rec).ptr;
				(*rec).ptr = ptr;
				(*rec).cnt = (*rec).cnt + 1;
				index.line_map[ptr - index.ptr_shift] = rec;
				continue_scan = true;
				break;
			}

			rec = (*rec).next;
			chain_len += 1;
		}

		if continue_scan {
			continue;
		}

		if chain_len == index.max_chain_length {
			return -1;
		}

		/*
		 * This is the first time we have ever seen this particular
		 * element in the sequence. Construct a new chain for it.
		 */
		let last = index.record_storage.len();
		index.record_storage.push(record::default());
		rec = &mut index.record_storage[last];
		(*rec).ptr = ptr;
		(*rec).cnt = 1;
		(*rec).next = *rec_chain;
		*rec_chain = rec;
		index.line_map[ptr - index.ptr_shift] = rec;
	}

	0
}



