pub mod ivec;
#[cfg(not(feature = "c_alt"))]
extern "C" {
    pub fn xmalloc(size: usize) -> *mut libc::c_void;
    pub fn xrealloc(ptr: *mut libc::c_void, size: usize) -> *mut  libc::c_void;
    pub fn xcalloc(nmemb: usize, size: usize) -> *mut libc::c_void;
}


#[cfg(feature = "c_alt")]
pub unsafe extern "C" fn xmalloc(size: usize) -> *mut libc::c_void {
    libc::malloc(size)
}

#[cfg(feature = "c_alt")]
pub unsafe extern "C" fn xrealloc(ptr: *mut libc::c_void, size: usize) -> *mut  libc::c_void {
    libc::realloc(ptr, size)
}

#[cfg(feature = "c_alt")]
pub unsafe extern "C" fn xcalloc(nmemb: usize, size: usize) -> *mut libc::c_void {
    libc::calloc(nmemb, size)
}
