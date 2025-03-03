#![allow(non_snake_case)]

use std::collections::{Bound, HashMap};
use std::hash::{BuildHasher, Hash};
use std::ops::{Range, RangeBounds};
use crate::xdiff::{XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE, XDF_IGNORE_WHITESPACE_AT_EOL, XDF_IGNORE_WHITESPACE_CHANGE, XDF_WHITESPACE_FLAGS};

pub fn line_length(data: &[u8], ignore_cr_at_eol: bool) -> (usize, usize) {
	let (mut no_eol, mut with_eol) = (data.len(), data.len());
	for i in 0..data.len() {
		if data[i] == b'\n' {
			no_eol = i;
			with_eol = i+1;
			break;
		}
	}

	if ignore_cr_at_eol && 0 < no_eol && data[no_eol - 1] == b'\r' {
		no_eol -= 1;
	}

	(no_eol, with_eol)
}


pub struct LineReader<'a> {
	content: &'a [u8],
	off: usize,
	ignore_cr_at_eol: bool,
}

impl<'a> LineReader<'a> {
	pub fn new(content: &'a [u8], ignore_cr_at_eol: bool) -> Self {
		Self {
			content,
			off: 0,
			ignore_cr_at_eol,
		}
	}
}


impl<'a> Iterator for LineReader<'a> {
	type Item = (&'a [u8], usize);

	fn next(&mut self) -> Option<Self::Item> {
		if self.off == self.content.len() {
			return None;
		}

		let (no_eol, with_eol) = line_length(&self.content[self.off..], self.ignore_cr_at_eol);
		let slice = &self.content[self.off..self.off+no_eol];
		self.off += with_eol;
		let eol_len = with_eol-no_eol;
		Some((slice, eol_len))
	}
}

pub(crate) fn xdl_bogosqrt(mut n: u64) -> u64 {
	/*
	 * Classical integer square root approximation using shifts.
	 */
	let mut i = 1;
	while n > 0 {
		i <<= 1;
		n >>= 2;
	}

	i
}

pub(crate) fn XDL_ISSPACE(v: u8) -> bool {
	match v {
		b'\t' | b'\n' | b'\r' | b' ' => true,
		_ => false,
	}
}


/// HashMap.entry(key).or_default() is discouraged because it requires an owned key
/// this function only clones the key if it doesn't already exist
pub(crate) fn get_or_default<'b, K, V, S>(map: &'b mut HashMap<K, V, S>, key: &K) -> &'b mut V
where K: Clone, K: Eq, K: Hash, V: Default, S: BuildHasher
{
	if !map.contains_key(key) {
		map.insert(key.clone(), Default::default());
	}
	map.get_mut(key).unwrap()
}


pub fn get_index_range<R>(bound: R, or_else: Range<usize>) -> Range<usize>
where R: RangeBounds<usize>
{
	let range = if or_else.start >= or_else.end {
		or_else
	} else {
		let s = match bound.start_bound() {
			Bound::Included(v) => *v,
			Bound::Excluded(v) => *v + 1,
			Bound::Unbounded => or_else.start,
		};

		let e = match bound.end_bound() {
			Bound::Included(v) => *v + 1,
			Bound::Excluded(v) => *v,
			Bound::Unbounded => or_else.end,
		};

		s..e
	};
	if range.start > range.end {
		panic!("start must be <= end");
	}
	range
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
			while range.start + 1 < range.end && XDL_ISSPACE(slice[range.start + 1]) && slice[range.start + 1] != b'\n' {
				range.start += 1;
			}
			let at_eol = range.end <= range.start + 1 || slice[range.start + 1] == b'\n';
			if (flags & XDF_IGNORE_WHITESPACE) != 0 {
				/* already handled */
			} else if (flags & XDF_IGNORE_WHITESPACE_CHANGE) != 0 && !at_eol {
				hash = hash.overflowing_mul(33).0 ^ b' ' as u64;
			} else if (flags & XDF_IGNORE_WHITESPACE_AT_EOL) != 0 && !at_eol {
				while ptr2 != range.start + 1 {
					hash = hash.overflowing_mul(33).0 ^ slice[ptr2] as u64;
					ptr2 += 1;
				}
			}
			range.start += 1;
			continue;
		}
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

pub fn chunked_iter_equal<'a, T, IT0, IT1>(mut it0: IT0, mut it1: IT1) -> bool
where
	T: Eq + 'a,
	IT0: Iterator<Item = &'a [T]>,
	IT1: Iterator<Item = &'a [T]>,
{
	let mut run_option0: Option<&[T]> = it0.next();
	let mut run_option1: Option<&[T]> = it1.next();
	let mut i0 = 0;
	let mut i1 = 0;

	while let (Some(run0), Some(run1)) = (run_option0, run_option1) {
		while i0 < run0.len() && i1 < run1.len() {
			if run0[i0] != run1[i1] {
				return false;
			}

			i0 += 1;
			i1 += 1;
		}

		if i0 == run0.len() {
			i0 = 0;
			run_option0 = it0.next();
		}
		if i1 == run1.len() {
			i1 = 0;
			run_option1 = it1.next();
		}
	}

	while let Some(run0) = run_option0 {
		if run0.len() == 0 {
			run_option0 = it0.next();
		} else {
			break;
		}
	}

	while let Some(run1) = run_option1 {
		if run1.len() == 0 {
			run_option1 = it1.next();
		} else {
			break;
		}
	}

	run_option0.is_none() && run_option1.is_none()
}


#[cfg(test)]
mod tests {
	use std::iter::Map;
	use std::slice::Iter;
	use crate::xutils::{chunked_iter_equal, xdl_hash_record};

	fn get_str_it<'a>(vec: &'a Vec<&str>) -> Map<Iter<'a, &'a str>, fn(&'a &str) -> &'a [u8]> {
		vec.iter().map(|v| (*v).as_bytes())
	}

	#[test]
	fn test_chunked_iter_equal() {
		let tv_str: Vec<(Vec<&str>, Vec<&str>)> = vec![
			/* equal cases */
			(vec!["", "", "abc"],         vec!["", "abc"]),
			(vec!["c", "", "a"],          vec!["c", "a"]),
			(vec!["a", "", "b", "", "c"], vec!["a", "b", "c"]),
			(vec!["", "", "a"],           vec!["a"]),
			(vec!["", "a"],               vec!["a"]),
			(vec![""],                    vec![]),
			(vec!["", ""],                vec![""]),
			(vec!["a"],                   vec!["", "", "a"]),
			(vec!["a"],                   vec!["", "a"]),
			(vec![],                      vec![""]),
			(vec![""],                    vec!["", ""]),
			(vec!["hello ", "world"],     vec!["hel", "lo wo", "rld"]),
			(vec!["hel", "lo wo", "rld"], vec!["hello ", "world"]),
			(vec!["hello world"],         vec!["hello world"]),
			(vec!["abc", "def"],          vec!["def", "abc"]),
			(vec![],                      vec![]),

			/* different cases */
			(vec!["abc"],       vec![]),
			(vec!["", "", ""],  vec!["", "a"]),
			(vec!["", "a"],     vec!["b", ""]),
			(vec!["abc"],       vec!["abc", "de"]),
			(vec!["abc", "de"], vec!["abc"]),
			(vec![],            vec!["a"]),
			(vec!["a"],         vec![]),
			(vec!["abc", "kj"], vec!["abc", "de"]),
		];

		for (lhs, rhs) in tv_str.iter() {
			let a: Vec<u8> = get_str_it(lhs).flatten().copied().collect();
			let b: Vec<u8> = get_str_it(rhs).flatten().copied().collect();
			let expected = a.as_slice() == b.as_slice();

			let it0 = get_str_it(lhs);
			let it1 = get_str_it(rhs);
			let actual = chunked_iter_equal(it0, it1);
			assert_eq!(expected, actual);
		}
	}

	#[test]
	fn test_xdl_hash_record() {
		let file = "This is\nsome text for \n xdl_hash_record() to \r\nchew on.";
		let slice = file.as_bytes();
		let (line_hash, _with_eol) = xdl_hash_record(slice, 0);
		assert_ne!(0, line_hash);
	}

}

