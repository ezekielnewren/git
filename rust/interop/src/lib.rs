pub mod ivec;

pub unsafe extern "C" fn xmalloc(size: usize) -> *mut libc::c_void {
    let t = libc::malloc(size);
    if t.is_null() {
        panic!("malloc failed: Out of memory");
    }
    t
}

pub unsafe extern "C" fn xrealloc(ptr: *mut libc::c_void, size: usize) -> *mut  libc::c_void {
    let t = libc::realloc(ptr, size);
    if t.is_null() {
        panic!("realloc failed: Out of memory");
    }
    t
}

pub unsafe extern "C" fn xcalloc(nmemb: usize, size: usize) -> *mut libc::c_void {
    let t = libc::calloc(nmemb, size);
    if t.is_null() {
        panic!("calloc failed: Out of memory");
    }
    t
}


