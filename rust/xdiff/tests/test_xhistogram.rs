pub mod mock;

#[cfg(test)]
mod tests {
    use std::path::Path;
    use xdiff::xdiff::{mmfile_t, XDF_HISTOGRAM_DIFF, XDF_INDENT_HEURISTIC};
    use crate::mock::helper::{read_test_file};

    #[test]
    pub fn test_xdl_do_histogram_diff() {
        let wd = std::env::current_dir().unwrap();

        let mut xpp: xpparam_t = Default::default();
        xpp.flags |= XDF_HISTOGRAM_DIFF;
        xpp.flags |= XDF_INDENT_HEURISTIC;

        let tv_name = ["salutations"];

        let t = Path::new("xhistogram");
        for tv in tv_name {
            let path = t.join(format!("{}{}", tv, "-before"));
            let mut before = read_test_file(&path).unwrap();

            let path = t.join(format!("{}{}", tv, "-after"));
            let mut after = read_test_file(&path).unwrap();

            let path = t.join(format!("{}{}", tv, "-expect"));
            let expect = read_test_file(&path).unwrap();



            xdl_do_diff(&mf1, &mf2, &xpp);
        }
    }
}
