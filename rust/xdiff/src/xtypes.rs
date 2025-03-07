#![allow(non_snake_case)]
#![allow(non_camel_case_types)]

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use crate::xrecord::xrecord_t;


pub type XdiffHasher = NOOPHasher;
pub type XdiffBuildHasherDefault = BuildHasherDefault<XdiffHasher>;
pub type XdiffHashMap<K, V> = std::collections::HashMap<K, V, XdiffBuildHasherDefault>;


#[derive(Default)]
pub struct NOOPHasher {
	pub hash: u64,
}

impl Hasher for NOOPHasher {
	fn finish(&self) -> u64 {
		self.hash
	}

	fn write(&mut self, _: &[u8]) {
		unimplemented!();
	}

	fn write_u64(&mut self, hash: u64) {
		self.hash = hash
	}
}


#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
pub enum ConsiderLine {
	NO,
	YES,
	TOO_MANY,
}

impl PartialEq<u8> for ConsiderLine {
	fn eq(&self, other: &u8) -> bool {
		*self as u8 == *other
	}
}

impl PartialEq<ConsiderLine> for u8 {
	fn eq(&self, other: &ConsiderLine) -> bool {
		*self == *other as u8
	}
}


impl Into<u8> for ConsiderLine {
	fn into(self) -> u8 {
		self as u8
	}
}


#[repr(C)]
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Occurrence {
	pub file1: usize,
	pub file2: usize,
}

impl Occurrence {
	pub fn increment(&mut self, xdf_idx: usize) {
		match xdf_idx {
			0 => self.file1 += 1,
			1 => self.file2 += 1,
			_ => panic!("illegal xdf_idx"),
		}
	}

	pub fn get(&self, xdf_idx: usize) -> usize {
		match xdf_idx {
			0 => self.file1,
			1 => self.file2,
			_ => panic!("illegal xdf_idx"),
		}
	}
}

/// This is the same hash algorithm that was used in the c version of xdiff
pub struct DJB2a {
	hash: u64,
}

impl Hasher for DJB2a {
	fn finish(&self) -> u64 {
		self.hash
	}

	fn write(&mut self, bytes: &[u8]) {
		for b in bytes {
			self.write_u8(*b);
		}
	}

	fn write_u8(&mut self, value: u8) {
		self.hash = self.hash.wrapping_mul(33) ^ value as u64;
	}
}

impl Default for DJB2a {
	fn default() -> Self {
		Self {
			hash: 5381,
		}
	}
}


#[cfg(test)]
mod tests {
	use std::hash::{Hash, Hasher};
	use crate::xtypes::{DJB2a};

	#[test]
	fn test_djb2a() {
		let tv = [
			(2697798502297004026, "void bye(void)"),
			(15439469216637218887, "    printf(\"goodbye\\n\");"),
			(4885930574453166566, "void hello(void)"),
		];

		for (expected, input) in tv {
			let mut hasher = DJB2a::default();
			hasher.write(input.as_bytes());
			let hash = hasher.finish();
			assert_eq!(expected, hash);

			let mut hasher = DJB2a::default();
			for b in input.as_bytes().iter() {
				hasher.write_u8(*b);
			}
			let hash = hasher.finish();
			assert_eq!(expected, hash);

			/*
			 * For the purposes of git hashing, this is the wrong way.
			 */
			// let mut hasher = DJB2a::default();
			// input.as_bytes().hash(&mut hasher);
			// let hash = hasher.finish();
			// assert_eq!(expected, hash);
		}
	}

}
