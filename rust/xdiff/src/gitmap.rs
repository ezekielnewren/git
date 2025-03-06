use std::alloc::{Layout};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher, Hash, Hasher, RandomState};
use std::marker::PhantomData;
use std::ops::{Index, IndexMut, Range};
use xxhash_rust::xxh3::Xxh3Builder;
use crate::xdiff::INVALID_INDEX;
use crate::xrecord::xrecord_t;

pub trait HashEq<K> {

    fn hash(&self, key: &K) -> u64;

    fn eq(&self, lhs: &K, rhs: &K) -> bool;

}


pub trait Comparator<T: Ord> {
    fn cmp(lhs: &T, rhs: &T) -> Ordering;
}

enum ProbeResult {
    Found(usize),
    Empty(usize),
    OutOfMemory,
}

struct Entry<K, V> {
    key: K,
    value: V,
}


pub struct FixedMap<'a, K, V, HE: HashEq<K>> {
    meta: &'a mut [u64],
    data: &'a mut [Entry<K, V>],
    meta_layout: Layout,
    data_layout: Layout,
    size: usize,
    mask: usize,
    he: HE,
}


impl<'a, K, V, HE: HashEq<K>> Drop for FixedMap<'a, K, V, HE> {
    fn drop(&mut self) {
        unsafe {
            if std::mem::needs_drop::<V>() {
                for i in 0..self.meta.len() {
                    if self.meta[i] != 0 {
                        std::ptr::drop_in_place(&mut self.data[i]);
                    }
                }
            }

            std::alloc::dealloc(self.meta.as_mut_ptr() as *mut u8, self.meta_layout);
            std::alloc::dealloc(self.data.as_mut_ptr() as *mut u8, self.data_layout);
        }
    }
}


impl<'a, K, V, HE: HashEq<K>> FixedMap<'a, K, V, HE> {

    pub fn with_capacity_and_hash_eq(capacity: usize, inst: HE) -> Self
    {
        let po2 = (capacity*2).next_power_of_two();
        let meta_layout = Layout::array::<u64>(po2).unwrap();
        let data_layout = Layout::array::<Entry<K, V>>(po2).unwrap();

        let ptr1 = unsafe { std::alloc::alloc_zeroed(meta_layout) };
        let ptr2 = unsafe { std::alloc::alloc(data_layout) };

        Self {
            meta: unsafe { std::slice::from_raw_parts_mut(ptr1 as *mut u64, po2) },
            data: unsafe { std::slice::from_raw_parts_mut(ptr2 as *mut Entry<K, V>, po2) },
            meta_layout,
            data_layout,
            size: 0,
            mask: po2 - 1,
            he: inst,
        }
    }


    fn _hash(&self, key: &K) -> u64 {
        /*
         * or with 1 << 63 to ensure valid hashes are never 0
         */
        self.he.hash(&key) | (1 << 63)
    }

    fn _probe_slots(&self, key: &K, hash: u64, range: Range<usize>) -> Result<usize, usize> {
        for i in range {
            match self.meta[i] {
                0 => return Err(i),
                h if h == hash && self.he.eq(&self.data[i].key, key) => return Ok(i),
                _ => continue,
            }
        }
        Err(INVALID_INDEX)
    }

    fn _find_entry(&self, key: &K, hash: u64) -> ProbeResult {
        let start = hash as usize & self.mask;
        let mut index = self._probe_slots(key, hash, start..self.meta.len());
        if let Err(i) = index {
            if i == INVALID_INDEX {
                index = self._probe_slots(key, hash, 0..start);
            }
        }
        match index {
            Ok(i) => ProbeResult::Found(i),
            Err(i) => {
                if i == INVALID_INDEX {
                    ProbeResult::OutOfMemory
                } else {
                    ProbeResult::Empty(i)
                }
            }
        }
    }

    fn _overwrite(&mut self, index: usize, hash: u64, key: K, value: V) {
        self.meta[index] = hash;
        unsafe {
            std::ptr::write(&mut self.data[index], Entry {
                key,
                value,
            });
        }
        self.size += 1;
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let hash = self._hash(key);
        let index = self._find_entry(key, hash);
        match index {
            ProbeResult::Found(i) => Some(&self.data[i].value),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let hash = self._hash(key);
        let index = self._find_entry(key, hash);
        match index {
            ProbeResult::Found(i) => Some(&mut self.data[i].value),
            _ => None,
        }
    }

    pub fn get_or_insert(&mut self, key: &K, value: V) -> &mut V
    where K: Clone, V: Default
    {
        let hash = self._hash(key);
        let index = self._find_entry(key, hash);
        match index {
            ProbeResult::Found(i) => &mut self.data[i].value,
            ProbeResult::Empty(i) => {
                self._overwrite(i, hash, key.clone(), value);
                &mut self.data[i].value
            }
            ProbeResult::OutOfMemory => panic!("FixedMap ran out of memory"),
        }

    }

    pub fn get_or_default(&mut self, key: &K) -> &mut V
    where K: Clone, V: Default
    {
        let hash = self._hash(key);
        let index = self._find_entry(key, hash);
        match index {
            ProbeResult::Found(i) => &mut self.data[i].value,
            ProbeResult::Empty(i) => {
                self._overwrite(i, hash, key.clone(), V::default());
                &mut self.data[i].value
            }
            ProbeResult::OutOfMemory => panic!("FixedMap ran out of memory"),
        }

    }

    pub fn insert(&mut self, key: &K, value: V)
    where K: Clone
    {
        let hash = self._hash(key);
        let index = self._find_entry(key, hash);
        match index {
            ProbeResult::Found(i) => {
                self.data[i].value = value;
            }
            ProbeResult::Empty(i) => {
                self._overwrite(i, hash, key.clone(), value);
            }
            ProbeResult::OutOfMemory => panic!("FixedMap ran out of memory"),
        }
    }

    pub fn len(&self) -> usize {
        self.size
    }

}

impl<'a, K: Hash + Eq, V> FixedMap<'a, K, V, DefaultHashEq<K>> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hash_eq(capacity, DefaultHashEq::new())
    }
}


struct DefaultComparator<T> {
    _phantom: PhantomData<T>,
}

impl<T> Default for DefaultComparator<T> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData::default()
        }
    }
}

impl<T> DefaultComparator<T> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: Ord> Comparator<T> for DefaultComparator<T> {
    fn cmp(lhs: &T, rhs: &T) -> Ordering {
        lhs.cmp(rhs)
    }
}


struct HashEqHasher<K, B: BuildHasher> {
    builder: B,
    _phantom: PhantomData<K>,
}


impl<K, B: BuildHasher> HashEqHasher<K, B> {
    pub fn new(builder: B) -> Self {
        Self {
            builder,
            _phantom: PhantomData::default(),
        }
    }
}

impl<K: Hash + Eq, B: BuildHasher> HashEq<K> for HashEqHasher<K, B> {
    fn hash(&self, key: &K) -> u64 {
        let mut state = self.builder.build_hasher();
        key.hash(&mut state);
        state.finish()
    }

    fn eq(&self, lhs: &K, rhs: &K) -> bool {
        lhs == rhs
    }
}

struct DefaultHashEq<K> {
    hasher: HashEqHasher<K, RandomState>,
}

impl<K> DefaultHashEq<K> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<K> Default for DefaultHashEq<K> {
    fn default() -> Self {
        Self {
            hasher: HashEqHasher::new(RandomState::new()),
        }
    }
}

impl<K: Hash + Eq> HashEq<K> for DefaultHashEq<K> {
    fn hash(&self, key: &K) -> u64 {
        let mut hasher = self.hasher.builder.build_hasher();
        key.hash(&mut hasher);
        hasher.finish()
    }

    fn eq(&self, lhs: &K, rhs: &K) -> bool {
        lhs == rhs
    }
}





pub struct KeyWrapper<'a, K, HE: HashEq<K>> {
    key: &'a K,
    he: &'a HE,
}


impl<'a, K, HE: HashEq<K>> Hash for KeyWrapper<'a, K, HE> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let hash = self.he.hash(self.key);
        state.write_u64(hash);
    }
}

impl<'a, K, HE: HashEq<K>> PartialEq for KeyWrapper<'a, K, HE> {
    fn eq(&self, other: &Self) -> bool {
        self.he.eq(self.key, other.key)
    }
}

impl<'a, K, HE: HashEq<K>> Eq for KeyWrapper<'a, K, HE> {}


pub struct MPHB<'a, K, HE: HashEq<K>> {
    map: HashMap<KeyWrapper<'a, K, HE>, u64>,
    he: &'a HE,
    monotonic: u64,
}

impl<'a, K, HE: HashEq<K>> MPHB<'a, K, HE> {

    pub fn new(capacity: usize, he: &'a HE) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
            he,
            monotonic: 0,
        }
    }


    pub fn hash(&mut self, key: &'a K) -> u64 {
        let kw = KeyWrapper {
            key,
            he: self.he,
        };

        if let Some(mph) = self.map.get(&kw) {
            *mph
        } else {
            let mph = self.monotonic;
            self.monotonic += 1;
            self.map.insert(kw, mph);
            mph
        }
    }

    pub fn finish(self) -> usize {
        self.map.len()
    }

}



#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::hash::Hash;
    use std::io::BufRead;
    use std::path::PathBuf;
    use xxhash_rust::xxh3::{xxh3_64, Xxh3Builder};
    use crate::mock::helper::read_test_file;
    use crate::gitmap::{DefaultHashEq, HashEq, HashEqHasher, FixedMap, MPHB};
    use crate::xrecord::{xrecord_he, xrecord_t};

    const FURNITURE: [&str; 41] = [
        "Chair", "Table", "Sofa", "Couch", "Bench", "Stool", "Recliner", "Armchair",
        "Ottoman", "Loveseat", "Desk", "Bookshelf", "Cabinet", "Dresser", "Wardrobe",
        "Nightstand", "Bed", "Headboard", "Bunk bed", "Futon", "Crib", "High chair",
        "Rocking chair", "Barstool", "Chaise lounge", "Side table", "Coffee table",
        "End table", "Dining table", "Dining table", "Console table", "Buffet", "Hutch", "TV stand",
        "Entertainment center", "Vanity", "Workbench", "Filing cabinet",
        "Chest of drawers", "Curio cabinet", "Hall tree"
    ];

    const FRUIT: [&str; 8] = [
        "apple", "apple", "apple", "cherry", "cherry", "orange", "apple", "cherry"
    ];

    // struct StringHE {}
    //
    // impl HashEq<String> for StringHE {
    //     fn hash(&self, key: &String) -> u64 {
    //         xxh3_64(key.as_bytes())
    //     }
    //
    //     fn eq(&self, lhs: &String, rhs: &String) -> bool {
    //         lhs == rhs
    //     }
    // }


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
    fn test_fixed_map() {
        let flags = 0;
        let he = xrecord_he::new(flags);

        let data = "alsdfkjalsdvnlas";

        let key = xrecord_t::new(data.as_ptr(), data.len(), data.len());

        let mut table = FixedMap::with_capacity_and_hash_eq(300, he);
        table.insert(&key, 0u64);





    }

    #[test]
    fn test_new() {
        let flags = 0u64;

        let mut list_vec: Vec<Vec<String>> = Vec::new();
        list_vec.push(FRUIT.iter().map(|s| s.to_string()).collect());
        list_vec.push(FURNITURE.iter().map(|s| s.to_string()).collect());
        let data = read_test_file(&PathBuf::from("xhistogram/gitdump.txt")).unwrap();
        let dump: Vec<String> = data.lines().map(|v| v.unwrap()).collect();
        list_vec.push(dump);

        for list in list_vec {
            let mut mphb_simple = MPHBSimple {
                map: HashMap::new(),
                monotonic: 0,
            };

            let mut monotonic = 0u64;

            let mut fm = FixedMap::with_capacity(list.len());
            for key in list.iter() {
                let expected = (key.clone(), mphb_simple.hash(&key));
                let mph = *fm.get_or_insert(key, monotonic);
                if mph == monotonic {
                    monotonic += 1;
                }
                let actual = (key.clone(), mph);
                assert_eq!(expected, actual);
            }

            let mph_size = fm.len();
            assert_eq!(mphb_simple.map.len(), mph_size);
        }
    }

}

