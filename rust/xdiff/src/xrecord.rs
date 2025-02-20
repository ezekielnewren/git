#![allow(non_camel_case_types)]

use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use ahash::AHasher;
use crate::xdiff::{INVALID_INDEX, XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE, XDF_IGNORE_WHITESPACE_AT_EOL, XDF_IGNORE_WHITESPACE_CHANGE, XDF_WHITESPACE_FLAGS};
use crate::xtypes::DJB2a;
use crate::xutils::XDL_ISSPACE;

#[repr(C)]
#[derive(Clone)]
pub struct xrecord_t {
    pub ptr: *const u8,
    pub size: usize,
    pub hash: u64,
    pub flags: u64,
}


impl Debug for xrecord_t {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<Self> for xrecord_t {
    fn eq(&self, other: &Self) -> bool {
        debug_assert_eq!(self.flags, other.flags);
        debug_assert_ne!(0, self.hash);
        debug_assert_ne!(0, other.hash);

        if self.hash != other.hash {
            return false;
        }

        if (self.flags&XDF_WHITESPACE_FLAGS) == 0 {
            self.as_ref() == other.as_ref()
        } else {
            self.iter().eq(other.iter())
        }
    }
}

impl Eq for xrecord_t {}

impl Hash for xrecord_t {
    fn hash<H: Hasher>(&self, state: &mut H) {
        debug_assert_ne!(0, self.hash);
        self.hash.hash(state);
    }
}

impl xrecord_t {

    pub fn new(slice: &[u8], eol_len: usize, flags: u64) -> Self {
        let mut line = Self {
            ptr: slice.as_ptr(),
            size: slice.len() + eol_len,
            hash: 0,
            flags,
        };
        let mut state;
        #[cfg(test)]
        {
            state = DJB2a::default();
        }
        #[cfg(not(test))]
        {
            state = AHasher::default();
        }
        if (flags & XDF_WHITESPACE_FLAGS) == 0 {
            slice.hash(&mut state);
        } else {
            for b in line.iter() {
                b.hash(&mut state);
            }
        }
        line.hash = state.finish();
        line
    }

    pub fn as_ref(&self) -> &[u8] {
        unsafe {
            let len = match self.size > 0 && *self.ptr == b'\n' {
                true => self.size - 1,
                false => self.size,
            };
            std::slice::from_raw_parts(self.ptr, len)
        }
    }

    pub fn as_str(&self) -> &str {
        unsafe {
            std::str::from_utf8_unchecked(self.as_ref())
        }
    }

    pub fn is_blank_line(&self) -> bool {
        for _ in self.iter() {
            return false;
        }
        true
    }

    pub fn iter(&self) -> Box<dyn Iterator<Item = &u8> + '_> {
        let line = self.as_ref();
        if (self.flags & XDF_WHITESPACE_FLAGS) == 0 {
            Box::new(line.iter())
        } else {
            Box::new(Iter {
                line,
                flags: self.flags,
                index: 0,
                whiterun_end: INVALID_INDEX,
            })
        }
    }
}


struct Iter<'a> {
    line: &'a [u8],
    flags: u64,
    index: usize,
    whiterun_end: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a u8;
    fn next(&mut self) -> Option<Self::Item> {
        let _off = self.index;
        while self.index < self.line.len() {
            let v = &self.line[self.index];
            if !XDL_ISSPACE(*v) {
                self.whiterun_end = INVALID_INDEX;
                self.index += 1;
            } else {
                // find the end of the whitespace run
                if self.whiterun_end > self.line.len() {
                    self.whiterun_end = self.line.len();
                    for i in self.index..self.line.len() {
                        if !XDL_ISSPACE(self.line[i]) {
                            self.whiterun_end = i;
                            break;
                        }
                    }
                }
                assert!(self.index < self.whiterun_end && self.whiterun_end <= self.line.len());

                // skip whitespace based on the flags
                if (self.flags & XDF_IGNORE_CR_AT_EOL) != 0 {
                    if *v == b'\r' && self.index+1 == self.line.len() {
                        self.index += 1;
                        continue;
                    } else {
                        self.index += 1;
                    }
                }
                if (self.flags & XDF_IGNORE_WHITESPACE_AT_EOL) != 0 {
                    if self.whiterun_end == self.line.len() {
                        self.index = self.whiterun_end;
                        continue;
                    } else {
                        self.index += 1;
                    }
                }
                if (self.flags & XDF_IGNORE_WHITESPACE_CHANGE) != 0 {
                    self.index = self.whiterun_end;
                    return Some(&b' ');
                }
                if (self.flags & XDF_IGNORE_WHITESPACE) != 0 {
                    self.index = self.whiterun_end;
                    continue;
                }
            }

            assert!(_off < self.index);
            return Some(v);
        }

        None
    }
}
