use crate::xdiff::{XDF_IGNORE_WHITESPACE, XDF_IGNORE_WHITESPACE_AT_EOL, XDF_IGNORE_WHITESPACE_CHANGE, XDF_IGNORE_WHITESPACE_WITHIN};
use crate::xrecord::xrecord;

pub(crate) fn XDL_ISSPACE(v: u8) -> bool {
    match v {
        b'\t' | b'\n' | b'\r' | b' ' => true,
        _ => false,
    }
}

pub struct LineReader {
    cur: *const u8,
    size: usize,
}

impl LineReader {
    pub fn new(file: &[u8]) -> Self {
        Self {
            cur: file.as_ptr(),
            size: file.len(),
        }
    }
}

impl Iterator for LineReader {
    type Item = xrecord;

    fn next(&mut self) -> Option<Self::Item> {
        if self.size == 0 {
            return None;
        }

        let cur = self.cur;
        unsafe {
            self.cur = libc::memchr(self.cur as *mut libc::c_void, b'\n' as libc::c_int, self.size) as *const u8;
        }
        let no_eol: usize;
        let with_eol: usize;
        if !self.cur.is_null() {
            no_eol = unsafe { self.cur.sub(cur as usize) } as usize;
            with_eol = no_eol + 1;
            self.size -= with_eol;
            self.cur = unsafe { self.cur.add(1) };
        } else {
            no_eol = self.size;
            with_eol = self.size;
            self.size = 0;
        }
        #[cfg(test)]
        let view = unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(cur, no_eol))
        };

        Some(xrecord::new(cur, no_eol, with_eol))
    }
}


pub struct WhitespaceIter<'a> {
    line: &'a [u8],
    index: usize,
    flags: u64,
}


impl<'a> WhitespaceIter<'a> {
    pub fn new(line: &'a [u8], flags: u64) -> Self {
        Self {
            line,
            index: 0,
            flags,
        }
    }
}

impl<'a> Iterator for WhitespaceIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.line.len() {
            return None;
        }

        if (self.flags & XDF_IGNORE_WHITESPACE_WITHIN) == 0 {
            self.index = self.line.len();
            return Some(self.line);
        }

        loop {
            let start = self.index;
            if self.index == self.line.len() {
                return None;
            }

            /* return contiguous run of not space bytes */
            while self.index < self.line.len() {
                if XDL_ISSPACE(self.line[self.index]) {
                    break;
                }
                self.index += 1;
            }
            if self.index > start {
                return Some(&self.line[start..self.index]);
            }
            /* the current byte had better be a space */
            if !XDL_ISSPACE(self.line[self.index]) {
                panic!("xdl_line_iter_next XDL_ISSPACE() is false")
            }

            while self.index < self.line.len() && XDL_ISSPACE(self.line[self.index]) {
                self.index += 1;
            }


            if self.index <= start {
                panic!("XDL_ISSPACE() cannot simultaneously be true and false");
            }

            if (self.flags & XDF_IGNORE_WHITESPACE_AT_EOL) != 0
                && self.index == self.line.len()
            {
                return None;
            }
            if (self.flags & XDF_IGNORE_WHITESPACE) != 0 {
                continue;
            }
            if (self.flags & XDF_IGNORE_WHITESPACE_CHANGE) != 0 {
                if self.index == self.line.len() {
                    continue;
                }
                return Some(" ".as_bytes());
            }
            return Some(&self.line[start..self.index]);
        }
    }
}


#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Read;
    use std::path::PathBuf;
    use crate::xdiff::{XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE, XDF_IGNORE_WHITESPACE_AT_EOL, XDF_IGNORE_WHITESPACE_CHANGE};
    use crate::xutils::{LineReader, WhitespaceIter};

    fn extract_string<'a>(line: &[u8], flags: u64, buffer: &'a mut Vec<u8>) -> &'a str {
        let it;
        if line.len() > 0 && line[line.len() - 1] == b'\n' {
            it = WhitespaceIter::new(&line[0..line.len() - 1], flags);
        } else {
            it = WhitespaceIter::new(line, flags);
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
    fn test_read_file() {
        let wd = PathBuf::from(".").canonicalize().unwrap();

        let mut fd = File::open(PathBuf::from("tests/data/xhistogram/salutations-before")).unwrap();

        let mut buff = Vec::new();
        fd.read_to_end(&mut buff).unwrap();
        drop(fd);

        for record in LineReader::new(buff.as_slice()) {
            assert!(record.as_ptr() as usize > 0);
        }

        assert!(wd.capacity() > 0);
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

            // ("1", "1\r", XDF_IGNORE_CR_AT_EOL),
            ("1", "1\r", XDF_IGNORE_WHITESPACE_CHANGE),

            // ("\r \t a ", "\r \t a \r", XDF_IGNORE_CR_AT_EOL),
            // ("\r a ", "\r a \r", XDF_IGNORE_CR_AT_EOL),
            // ("", "\r", XDF_IGNORE_CR_AT_EOL),
            // ("", "", XDF_IGNORE_CR_AT_EOL),
            // ("\r a ", "\r a ", XDF_IGNORE_CR_AT_EOL),

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
