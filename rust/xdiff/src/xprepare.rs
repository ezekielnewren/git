use interop::ivec::IVec;
use crate::xdiff::*;

const XDL_KPDIS_RUN: usize = 4;
const XDL_MAX_EQLIMIT: usize = 1024;
const XDL_SIMSCAN_WINDOW: usize = 100;


#[no_mangle]
unsafe extern "C" fn xdl_clean_mmatch(dis: *mut IVec<u8>, i: usize, mut start: usize, mut end: usize) -> bool {
    let dis = IVec::from_raw_mut(dis);

    /*
     * Limits the window the is examined during the similar-lines
     * scan. The loops below stops when dis[i - r] == 1 (line that
     * has no match), but there are corner cases where the loop
     * proceed all the way to the extremities by causing huge
     * performance penalties in case of big files.
     */
    if i - start > XDL_SIMSCAN_WINDOW {
        start = i - XDL_SIMSCAN_WINDOW;
    }
    if end - i > XDL_SIMSCAN_WINDOW {
        end = i + XDL_SIMSCAN_WINDOW;
    }

    /*
     * Scans the lines before 'i' to find a run of lines that either
     * have no match (dis[j] == 0) or have multiple matches (dis[j] > 1).
     * Note that we always call this function with dis[i] > 1, so the
     * current line (i) is already a multimatch line.
     */
    let mut rdis0 = 0;
    let mut rpdis0 = 1;
    for i0 in (start..i).rev() {
        if dis[i0] != NO {
            rdis0 += 1;
        } else if dis[i0] == TOO_MANY {
            rpdis0 += 1;
        } else {
            break;
        }
    }
    /*
     * If the run before the line 'i' found only multimatch lines, we
     * return 0 and hence we don't make the current line (i) discarded.
     * We want to discard multimatch lines only when they appear in the
     * middle of runs with nomatch lines (dis[j] == 0).
     */
    if rdis0 == 0 {
        return false;
    }
    let mut rdis1 = 0;
    let mut rpdis1 = 1;
    for i1 in i + 1..end {
        if dis[i1] != NO {
            rdis1 += 1;
        } else if dis[i1] == TOO_MANY {
            rpdis1 += 1;
        } else {
            break;
        }
    }
    /*
     * If the run after the line 'i' found only multimatch lines, we
     * return 0 and hence we don't make the current line (i) discarded.
     */
    if rdis1 == 0 {
        return false;
    }
    rdis1 += rdis0;
    rpdis1 += rpdis0;

    rpdis1 * XDL_KPDIS_RUN < (rpdis1 + rdis1)
}