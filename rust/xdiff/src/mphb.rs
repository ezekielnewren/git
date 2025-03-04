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


struct Entry<K> {
    key: K,
    mph: u64,
}


pub struct MinimalPerfectHashBuilder<'a, HE: HashAndEq<K>, K> {
    meta: Array<'a, u64>,
    data: Array<'a, Entry<K>>,
    mask: usize,
    he: HE,
    monotonic: u64,
}

impl<'a, HE: HashAndEq<K>, K> MinimalPerfectHashBuilder<'a, HE, K> {

    pub fn new(capacity: usize, inst: HE) -> Self {
        let po2 = (capacity << 1).next_power_of_two();
        Self {
            meta: Array::new(po2, true),
            data: Array::new(po2, false),
            mask: po2 - 1,
            he: inst,
            monotonic: 0,
        }
    }

    pub fn hash(&mut self, key: &K) -> u64
    where K: Clone
    {
        /*
         * or with 1 to ensure valid hashes are never 0
         */
        let hash = self.he.hash(&key) | 1;
        let start = hash as usize & self.mask;
        let matchit = |v: (usize, &u64)| *v.1 == 0 || (*v.1 == hash && self.he.eq(&self.data.slice[v.0].key, key));
        let mut position = self.meta.slice[start..].iter().enumerate().position(matchit);
        if position.is_none() {
            position = self.meta.slice[..start].iter().enumerate().position(matchit);
        }
        let index = position.unwrap();

        if self.meta.slice[index] == 0 {
            let mph = self.monotonic;
            self.data.slice[index] = Entry {
                key: key.clone(),
                mph,
            };
            self.monotonic += 1;
            mph
        } else {
            self.data.slice[index].mph
        }
    }

    pub fn finish(self) -> usize {
        self.monotonic as usize
    }
}


pub trait HashAndEq<T> {

    fn hash(&self, key: &T) -> u64;

    fn eq(&self, lhs: &T, rhs: &T) -> bool;

}



#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::hash::Hash;
    use xxhash_rust::xxh3::xxh3_64;
    use crate::mphb::{Entry, HashAndEq, MinimalPerfectHashBuilder};
    use crate::xrecord::{xrecord_he, xrecord_t};

    const FURNITURE: [&str; 40] = [
        "Chair", "Table", "Sofa", "Couch", "Bench", "Stool", "Recliner", "Armchair",
        "Ottoman", "Loveseat", "Desk", "Bookshelf", "Cabinet", "Dresser", "Wardrobe",
        "Nightstand", "Bed", "Headboard", "Bunk bed", "Futon", "Crib", "High chair",
        "Rocking chair", "Barstool", "Chaise lounge", "Side table", "Coffee table",
        "End table", "Dining table", "Console table", "Buffet", "Hutch", "TV stand",
        "Entertainment center", "Vanity", "Workbench", "Filing cabinet",
        "Chest of drawers", "Curio cabinet", "Hall tree"
    ];


    struct StringHE {}

    impl HashAndEq<String> for StringHE {
        fn hash(&self, key: &String) -> u64 {
            xxh3_64(key.as_bytes())
        }

        fn eq(&self, lhs: &String, rhs: &String) -> bool {
            lhs == rhs
        }
    }


    struct MPHBSimple<K: Hash + Eq> {
        map: HashMap<K, u64>,
        monotonic: u64,
    }

    impl<K: Hash + Eq> MPHBSimple<K> {
        fn hash(&mut self, key: &K) -> u64
        where K: Clone
        {
            if !self.map.contains_key(key) {
                let mph = self.monotonic;
                self.monotonic += 1;
                self.map.insert(key.clone(), mph);
                mph
            } else {
                self.map[key]
            }
        }
    }


    #[test]
    fn test_new() {
        let flags = 0;

        let mut mphb_simple = MPHBSimple {
            map: HashMap::new(),
            monotonic: 0,
        };

        let he = StringHE{};
        let mut lu = MinimalPerfectHashBuilder::<StringHE, String>::new(FURNITURE.len(), he);
        for v in FURNITURE.iter() {
            let key = String::from(*v);
            let expected = (key.clone(), mphb_simple.hash(&key));
            let actual = (key.clone(), lu.hash(&key));
            assert_eq!(expected, actual);
        }

        let mph_size = lu.finish();
        assert_eq!(mphb_simple.map.len(), mph_size);
    }

}

