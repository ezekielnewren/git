

#[cfg(test)]
pub mod helper {
    use std::fs::File;
    use std::io::Read;
    use std::path::Path;

    pub const DIR_TESTS_DATA: &str = "tests/data";

    pub fn read_test_file(subpath: &Path) -> std::io::Result<Vec<u8>> {
        let t = Path::new(DIR_TESTS_DATA);
        let path = t.join(subpath);
        let mut fd = File::open(path)?;
        let mut buff = vec![0u8; 0];
        fd.read_to_end(&mut buff)?;
        Ok(buff)
    }

}


