#![allow(non_camel_case_types)]

use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
#[cfg(not(debug_assertions))]
use ahash::AHasher;
use crate::xdiff::{XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE, XDF_IGNORE_WHITESPACE_AT_EOL, XDF_IGNORE_WHITESPACE_CHANGE, XDF_WHITESPACE_FLAGS};
use crate::xtypes::DJB2a;
use crate::xutils::XDL_ISSPACE;

#[repr(C)]
#[derive(Clone)]
pub struct xrecord_t {
    pub ptr: *const u8,
    pub size: usize,
    pub line_hash: u64,
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
        debug_assert_ne!(0, self.line_hash);
        debug_assert_ne!(0, other.line_hash);

        if self.line_hash != other.line_hash {
            return false;
        }

        if (self.flags&XDF_WHITESPACE_FLAGS) == 0 {
            self.as_ref() == other.as_ref()
        } else {
            let mut lhs = Vec::new();
            for run in IterWhiteSpace::new(self.as_ref(), self.flags) {
                lhs.extend_from_slice(run);
            }

            let mut rhs = Vec::new();
            for run in IterWhiteSpace::new(other.as_ref(), other.flags) {
                rhs.extend_from_slice(run);
            }

            lhs.as_slice() == rhs.as_slice()
        }
    }
}

impl Eq for xrecord_t {}

impl Hash for xrecord_t {
    fn hash<H: Hasher>(&self, state: &mut H) {
        debug_assert_ne!(0, self.line_hash);
        state.write_u64(self.line_hash);
    }
}

impl xrecord_t {

    pub fn hash(slice: &[u8], flags: u64) -> u64 {
        let mut state;
        #[cfg(debug_assertions)]
        {
            state = DJB2a::default();
        }
        #[cfg(not(debug_assertions))]
        {
            state = AHasher::default();
        }
        if (flags & XDF_WHITESPACE_FLAGS) == 0 {
            state.write(slice);
        } else {
            for run in IterWhiteSpace::new(slice, flags) {
                #[cfg(test)]
                let _view = unsafe { std::str::from_utf8_unchecked(run) };
                state.write(run);
            }
        }
        state.finish()
    }

    pub fn new(slice: &[u8], eol_len: usize, flags: u64) -> Self {
        Self {
            ptr: slice.as_ptr(),
            size: slice.len() + eol_len,
            line_hash: Self::hash(slice, flags),
            flags,
        }
    }

    pub fn has_lf(&self) -> bool {
        self.size > 0 && unsafe { *self.ptr.add(self.size - 1) } == b'\n'
    }

    pub fn len_no_eol(&self) -> usize {
        match self.has_lf() {
            true => self.size - 1,
            false => self.size,
        }
    }

    pub fn len_with_eol(&self) -> usize {
        self.size
    }

    pub fn as_ref(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.ptr, self.len_no_eol())
        }
    }

    pub fn as_str(&self) -> &str {
        unsafe {
            std::str::from_utf8_unchecked(self.as_ref())
        }
    }

    pub fn is_blank_line(&self) -> bool {
        if (self.flags & XDF_WHITESPACE_FLAGS) == 0 {
            return self.as_ref().len() == 0;
        } else {
            for _ in self.iter() {
                return false;
            }
        }
        true
    }

    pub fn iter(&self) -> IterWhiteSpace {
        let line = self.as_ref();
        IterWhiteSpace::new(line, self.flags)
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
