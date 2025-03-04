#![allow(non_camel_case_types)]

use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use crate::mphb::HashAndEq;
use crate::xdiff::{XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE, XDF_IGNORE_WHITESPACE_AT_EOL, XDF_IGNORE_WHITESPACE_CHANGE, XDF_IGNORE_WHITESPACE_WITHIN, XDF_WHITESPACE_FLAGS};
use crate::xtypes::DJB2a;
use crate::xutils::{chunked_iter_equal, XDL_ISSPACE};

#[repr(C)]
#[derive(Clone)]
pub struct xrecord_t {
    pub ptr: *const u8,
    pub size_no_eol: usize,
    pub size_with_eol: usize,
}


impl Debug for xrecord_t {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}


pub struct xrecord_he {
    flags: u64
}

impl xrecord_he {
    pub(crate) fn new(flags: u64) -> Self {
        Self {
            flags,
        }
    }
}

impl HashAndEq<xrecord_t> for xrecord_he {
    fn hash(&self, key: &xrecord_t) -> u64 {
        if (self.flags & XDF_IGNORE_WHITESPACE_WITHIN) == 0 {
            #[cfg(debug_assertions)]
            {
                let mut state = DJB2a::default();
                state.write(key.as_ref());
                state.finish()
            }
            #[cfg(not(debug_assertions))]
            xxhash_rust::xxh3::xxh3_64(key.as_ref())
        } else {
            #[cfg(debug_assertions)]
            let mut state = DJB2a::default();
            #[cfg(not(debug_assertions))]
            let mut state = xxhash_rust::xxh3::Xxh3::default();
            for run in IterWhiteSpace::new(key.as_ref(), self.flags) {
                #[cfg(test)]
                let _view = unsafe { std::str::from_utf8_unchecked(run) };
                state.write(run);
            }
            state.finish()
        }
    }

    fn eq(&self, lhs: &xrecord_t, rhs: &xrecord_t) -> bool {
        if (self.flags & XDF_IGNORE_WHITESPACE_WITHIN) == 0 {
            lhs.as_ref() == rhs.as_ref()
        } else {
            let lhs = IterWhiteSpace::new(lhs.as_ref(), self.flags);
            let rhs = IterWhiteSpace::new(rhs.as_ref(), self.flags);
            chunked_iter_equal(lhs, rhs)
        }
    }
}

impl xrecord_t {

    pub fn new(ptr: *const u8, no_eol: usize, with_eol: usize) -> Self {
        Self {
            ptr,
            size_no_eol: no_eol,
            size_with_eol: with_eol,
        }
    }

    pub fn len_no_eol(&self) -> usize {
        self.size_no_eol
    }

    pub fn len_with_eol(&self) -> usize {
        self.size_with_eol
    }

    pub fn as_ref(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.ptr, self.size_no_eol)
        }
    }

    pub fn as_str(&self) -> &str {
        unsafe {
            std::str::from_utf8_unchecked(self.as_ref())
        }
    }

    pub fn is_blank_line(&self, flags: u64) -> bool {
        if (flags & XDF_WHITESPACE_FLAGS) == 0 {
            return self.as_ref().len() == 0;
        } else {
            for _ in self.iter(flags) {
                return false;
            }
        }
        true
    }

    pub fn iter(&self, flags: u64) -> IterWhiteSpace {
        let line = self.as_ref();
        IterWhiteSpace::new(line, flags)
    }

}


pub(crate) struct IterWhiteSpace<'a> {
    line: &'a [u8],
    end: usize,
    flags: u64,
    index: usize,
    #[cfg(test)]
    view: &'a str,
}

impl<'a> IterWhiteSpace<'a> {
    pub(crate) fn new(line: &'a [u8], flags: u64) -> Self {
        if (flags & XDF_WHITESPACE_FLAGS) == 0 {
            panic!("no whitespace flags present, use as_ref() instead")
        }
        let end = if (flags & XDF_IGNORE_CR_AT_EOL) != 0
            && line.len() > 0 && line[line.len() - 1] == b'\r'
        {
            line.len() - 1
        } else {
            line.len()
        };
        Self {
            line,
            end,
            flags,
            index: 0,
            #[cfg(test)]
            view: unsafe { std::str::from_utf8_unchecked(line) },
        }
    }
}

impl<'a> Iterator for IterWhiteSpace<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.end {
            return None;
        }

        if (self.flags & XDF_WHITESPACE_FLAGS) == XDF_IGNORE_CR_AT_EOL {
            self.index = self.end;
            return Some(&self.line[0..self.end]);
        }

        loop {
            let start = self.index;
            if self.index >= self.end {
                return None;
            }

            /* return contiguous run of not space bytes */
            while self.index < self.end {
                if XDL_ISSPACE(self.line[self.index]) {
                    break;
                }
                self.index += 1;
            }
            if self.index > start {
                return Some(&self.line[start..self.index]);
            }
            /* the current byte had better be a space */
            debug_assert!(XDL_ISSPACE(self.line[self.index]));

            while self.index < self.end {
                if !XDL_ISSPACE(self.line[self.index]) {
                    break;
                }
                self.index += 1;
            }

            debug_assert!(self.index > start, "XDL_ISSPACE() cannot simultaneously be true and false");
            if (self.flags & XDF_IGNORE_WHITESPACE_AT_EOL) != 0
                && self.index >= self.end
            {
                return None;
            }
            if (self.flags & XDF_IGNORE_WHITESPACE) != 0 {
                continue;
            }
            if (self.flags & XDF_IGNORE_WHITESPACE_CHANGE) != 0 {
                if self.index >= self.end {
                    continue;
                }
                return Some(&[b' ']);
            }
            return Some(&self.line[start..self.index]);
        }
    }
}


#[cfg(test)]
mod tests {
    use crate::xdiff::{XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE, XDF_IGNORE_WHITESPACE_AT_EOL, XDF_IGNORE_WHITESPACE_CHANGE};
    use crate::xrecord::{IterWhiteSpace};

    fn extract_string<'a>(line: &[u8], flags: u64, buffer: &'a mut Vec<u8>) -> &'a str {
        let it;
        if line.len() > 0 && line[line.len() - 1] == b'\n' {
            it = IterWhiteSpace::new(&line[0..line.len() - 1], flags);
        } else {
            it = IterWhiteSpace::new(line, flags);
        }
        buffer.clear();
        for run in it {
            #[cfg(test)]
            let _view = unsafe { std::str::from_utf8_unchecked(run) };
            buffer.extend_from_slice(run);
        }
        unsafe { std::str::from_utf8_unchecked(buffer.as_slice()) }
    }

    #[test]
    fn test_ignore_space() {
        let tv_individual = vec![
            ("a", "\r \t a \r", XDF_IGNORE_WHITESPACE),
            ("a", "\r a \r", XDF_IGNORE_WHITESPACE),
            ("", "\r", XDF_IGNORE_WHITESPACE),
            ("", "", XDF_IGNORE_WHITESPACE),
            ("a", "\r a ", XDF_IGNORE_WHITESPACE),
            ("", "     ", XDF_IGNORE_WHITESPACE),
            ("a", "a     ", XDF_IGNORE_WHITESPACE),
            ("aasdf", "  a  \t  asdf  \t \r", XDF_IGNORE_WHITESPACE),
            ("ab", "\t a  b  \t ", XDF_IGNORE_WHITESPACE),
            ("ab", "  a b \t \r", XDF_IGNORE_WHITESPACE),
            ("a", "\t  a ", XDF_IGNORE_WHITESPACE),
            ("a", "\t\t\ta\t", XDF_IGNORE_WHITESPACE),
            ("a", "a", XDF_IGNORE_WHITESPACE),
            ("a", "\ta", XDF_IGNORE_WHITESPACE),

            ("1", "1\r", XDF_IGNORE_CR_AT_EOL),
            ("1", "1\r", XDF_IGNORE_WHITESPACE_CHANGE),

            ("\r \t a ", "\r \t a \r", XDF_IGNORE_CR_AT_EOL),
            ("\r a ", "\r a \r", XDF_IGNORE_CR_AT_EOL),
            ("", "\r", XDF_IGNORE_CR_AT_EOL),
            ("", "", XDF_IGNORE_CR_AT_EOL),
            ("\r a ", "\r a ", XDF_IGNORE_CR_AT_EOL),

            ("", "     ", XDF_IGNORE_WHITESPACE_AT_EOL),
            ("a", "a     ", XDF_IGNORE_WHITESPACE_AT_EOL),
            ("  a  \t  asdf", "  a  \t  asdf  \t \r", XDF_IGNORE_WHITESPACE_AT_EOL),
            ("\t a  b", "\t a  b  \t ", XDF_IGNORE_WHITESPACE_AT_EOL),

            (" a b", "  a b \t \r", XDF_IGNORE_WHITESPACE_CHANGE),
            (" a", "\t  a ", XDF_IGNORE_WHITESPACE_CHANGE),
            (" a", "\t\t\ta\t", XDF_IGNORE_WHITESPACE_CHANGE),
            ("a", "a", XDF_IGNORE_WHITESPACE_CHANGE),
            (" a", "\ta", XDF_IGNORE_WHITESPACE_CHANGE),

            ("ab", "  a b \t \r", XDF_IGNORE_WHITESPACE | XDF_IGNORE_WHITESPACE_CHANGE),
            ("a", "\t  a ", XDF_IGNORE_WHITESPACE | XDF_IGNORE_WHITESPACE_CHANGE),
            ("a", "\t\t\ta\t", XDF_IGNORE_WHITESPACE | XDF_IGNORE_WHITESPACE_CHANGE),
            ("a", "a", XDF_IGNORE_WHITESPACE | XDF_IGNORE_WHITESPACE_CHANGE),
            ("a", "\ta", XDF_IGNORE_WHITESPACE | XDF_IGNORE_WHITESPACE_CHANGE),
        ];

        let mut buffer = Vec::<u8>::new();
        for (expected, input, flags) in tv_individual {
            let actual = extract_string(input.as_bytes(), flags, &mut buffer);
            assert_eq!(expected, actual, "input: {:?} flags: 0x{:x}", input, flags);
        }
    }
}
