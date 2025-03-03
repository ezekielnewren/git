use std::alloc::{Layout};
use std::ops::{Index, IndexMut};
use typed_arena::Arena;


pub struct Array<'a, T> {
    slice: &'a mut [T],
}


impl<'a, T> Array<'a, T> {

    pub fn new(capacity: usize, zero: bool) -> Self {
        let lay = Layout::array::<T>(capacity).unwrap();
        unsafe {
            let ptr = if zero {
                std::alloc::alloc_zeroed(lay)
            } else {
                std::alloc::alloc(lay)
            };
            Self {
                slice: std::slice::from_raw_parts_mut(ptr as *mut T, capacity),
            }
        }
    }

}

impl<'a, T> Drop for Array<'a, T> {
    fn drop(&mut self) {
        let lay = Layout::array::<T>(self.slice.len()).unwrap();
        unsafe {
            std::alloc::dealloc(self.slice.as_mut_ptr() as *mut u8, lay);
        }
    }
}


struct MinimalPerfectHashBuilderNode<K> {
    key: K,
    hash: u64,
    mph: u64,
    next: *mut MinimalPerfectHashBuilderNode<K>,
}


pub struct MinimalPerfectHashBuilder<'a, HE: HashAndEq<K>, K> {
    he: HE,
    mask: usize,
    head: Array<'a, *mut MinimalPerfectHashBuilderNode<K>>,
    entry: Arena<MinimalPerfectHashBuilderNode<K>>,
    mph: u64,
}

impl<'a, HE: HashAndEq<K>, K> MinimalPerfectHashBuilder<'a, HE, K> {

    pub fn new(capacity: usize, inst: HE) -> Self {
        let po2 = capacity.next_power_of_two();
        Self {
            he: inst,
            mask: po2 - 1,
            head: Array::new(po2, true),
            entry: Arena::with_capacity(capacity),
            mph: 0,
        }
    }

    pub fn hash(&mut self, key: &K) -> u64
    where K: Clone
    {
        let hash = self.he.hash(&key);
        let bucket = hash as usize & self.mask;
        let mut cur = self.head.slice[bucket];
        while !cur.is_null() {
            let node = unsafe { &mut *cur };
            if node.hash == hash && self.he.eq(&node.key, &key) {
                break;
            }

            cur = node.next;
        }

        if cur.is_null() {
            let out = self.mph;
            let bucket = hash as usize & self.mask;
            self.head.slice[bucket] = self.entry.alloc(MinimalPerfectHashBuilderNode {
                hash,
                key: key.clone(),
                mph: self.mph,
                next: self.head.slice[bucket],
            });
            self.mph += 1;
            out
        } else {
            unsafe { &mut *cur }.mph
        }
    }

    pub fn finish(self) -> usize {
        let out = self.mph;
        drop(self);
        out as usize
    }
}


pub trait HashAndEq<T> {

    fn hash(&self, key: &T) -> u64;

    fn eq(&self, lhs: &T, rhs: &T) -> bool;

}



#[cfg(test)]
mod tests {
    use crate::mphb::{HashAndEq, MinimalPerfectHashBuilder};
    use crate::xrecord::{xrecord_he, xrecord_t};

    #[test]
    fn test_new() {
        let flags = 0;

        let rec = xrecord_t::new("".as_bytes(), 1, flags);

        let he = xrecord_he::new(flags);
        let mut lu = MinimalPerfectHashBuilder::<xrecord_he, xrecord_t>::new(500, he);

        lu.hash(&rec);

        let mph_size = lu.finish();
    }

}

