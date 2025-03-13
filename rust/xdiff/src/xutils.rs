#![allow(non_snake_case)]

use crate::xdiff::{XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE, XDF_IGNORE_WHITESPACE_AT_EOL, XDF_IGNORE_WHITESPACE_CHANGE, XDF_IGNORE_WHITESPACE_WITHIN};
use crate::xtypes::xrecord;

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

pub fn strip_eol(line: &[u8], flags: u64) -> &[u8] {
    let mut end = line.len();
    if end > 0 && line[end - 1] == b'\n' {
        end -= 1;
    }
    if (flags & XDF_IGNORE_CR_AT_EOL) != 0 && end > 0 && line[end - 1] == b'\r' {
        end -= 1;
    }
    &line[0..end]
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


pub fn xdl_bogosqrt(mut n: u64) -> u64 {
	let mut i = 1;

	/*
	 * Classical integer square root approximation using shifts.
	 */
    while n > 0 {
        i <<= 1;
        n >>= 2;
    }

	i
}


#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Read;
    use std::iter::Map;
    use std::slice::Iter;
    use std::path::PathBuf;
    use crate::xdiff::{XDF_IGNORE_CR_AT_EOL, XDF_IGNORE_WHITESPACE, XDF_IGNORE_WHITESPACE_AT_EOL, XDF_IGNORE_WHITESPACE_CHANGE};
    use crate::xutils::{chunked_iter_equal, strip_eol, LineReader, WhitespaceIter};

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

    fn get_str_it<'a>(vec: &'a Vec<&str>) -> Map<Iter<'a, &'a str>, fn(&'a &str) -> &'a [u8]> {
        vec.iter().map(|v| (*v).as_bytes())
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
            ("a", "\r \t a \r\n", XDF_IGNORE_WHITESPACE),
            ("a", "\r a \r\n", XDF_IGNORE_WHITESPACE),
            ("", "\r\n", XDF_IGNORE_WHITESPACE),
            ("", "\n", XDF_IGNORE_WHITESPACE),
            ("a", "\r a \n", XDF_IGNORE_WHITESPACE),
            ("", "     \n", XDF_IGNORE_WHITESPACE),
            ("a", "a     \n", XDF_IGNORE_WHITESPACE),
            ("aasdf", "  a  \t  asdf  \t \r\n", XDF_IGNORE_WHITESPACE),
            ("ab", "\t a  b  \t \n", XDF_IGNORE_WHITESPACE),
            ("ab", "  a b \t \r\n", XDF_IGNORE_WHITESPACE),
            ("a", "\t  a \n", XDF_IGNORE_WHITESPACE),
            ("a", "\t\t\ta\t\n", XDF_IGNORE_WHITESPACE),
            ("a", "a\n", XDF_IGNORE_WHITESPACE),
            ("a", "\ta\n", XDF_IGNORE_WHITESPACE),

            ("1", "1\r\n", XDF_IGNORE_CR_AT_EOL),
            ("1", "1\r\n", XDF_IGNORE_WHITESPACE_CHANGE),

            ("\r \t a ", "\r \t a \r\n", XDF_IGNORE_CR_AT_EOL),
            ("\r a ", "\r a \r\n", XDF_IGNORE_CR_AT_EOL),
            ("", "\r\n", XDF_IGNORE_CR_AT_EOL),
            ("", "\n", XDF_IGNORE_CR_AT_EOL),
            ("\r a ", "\r a \n", XDF_IGNORE_CR_AT_EOL),

            ("", "     \n", XDF_IGNORE_WHITESPACE_AT_EOL),
            ("a", "a     \n", XDF_IGNORE_WHITESPACE_AT_EOL),
            ("  a  \t  asdf", "  a  \t  asdf  \t \r\n", XDF_IGNORE_WHITESPACE_AT_EOL),
            ("\t a  b", "\t a  b  \t \n", XDF_IGNORE_WHITESPACE_AT_EOL),

            (" a b", "  a b \t \r\n", XDF_IGNORE_WHITESPACE_CHANGE),
            (" a", "\t  a \n", XDF_IGNORE_WHITESPACE_CHANGE),
            (" a", "\t\t\ta\t\n", XDF_IGNORE_WHITESPACE_CHANGE),
            ("a", "a\n", XDF_IGNORE_WHITESPACE_CHANGE),
            (" a", "\ta\n", XDF_IGNORE_WHITESPACE_CHANGE),

            ("ab", "  a b \t \r\n", XDF_IGNORE_WHITESPACE | XDF_IGNORE_WHITESPACE_CHANGE),
            ("a", "\t  a \n", XDF_IGNORE_WHITESPACE | XDF_IGNORE_WHITESPACE_CHANGE),
            ("a", "\t\t\ta\t\n", XDF_IGNORE_WHITESPACE | XDF_IGNORE_WHITESPACE_CHANGE),
            ("a", "a\n", XDF_IGNORE_WHITESPACE | XDF_IGNORE_WHITESPACE_CHANGE),
            ("a", "\ta\n", XDF_IGNORE_WHITESPACE | XDF_IGNORE_WHITESPACE_CHANGE),
        ];

        let mut buffer = Vec::<u8>::new();
        for (expected, input, flags) in tv_individual {
            let actual = extract_string(strip_eol(input.as_bytes(), flags), flags, &mut buffer);
            assert_eq!(expected, actual, "input: {:?} flags: 0x{:x}", input, flags);
        }
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

}
