use std::alloc::Layout;
use std::cmp::Ordering;
use std::hash::{BuildHasher, Hash, Hasher, RandomState};
use std::marker::PhantomData;

pub trait HashEq<K> {

    fn hash(&self, key: &K) -> u64;

    fn eq(&self, lhs: &K, rhs: &K) -> bool;

}


pub trait Comparator<T: Ord> {
    fn cmp(lhs: &T, rhs: &T) -> Ordering;
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

pub struct DefaultHashEq<K> {
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





struct FixedMapEntry<K, V> {
    key_hash: u64,
    key: K,
    value: V,
    next: *mut FixedMapEntry<K, V>,
}

pub struct FixedMap<'a, K, V, HE: HashEq<K>> {
    head: &'a mut [*mut FixedMapEntry<K, V>],
    entry: &'a mut [FixedMapEntry<K, V>],
    head_layout: Layout,
    entry_layout: Layout,
    count: usize,
    capacity: usize,
    mask: usize,
    he: HE,
}


impl<'a, K, V, HE: HashEq<K>> Drop for FixedMap<'a, K, V, HE> {
    fn drop(&mut self) {
        unsafe {
            if std::mem::needs_drop::<V>() {
                for i in 0..self.head.len() {
                    let mut cur = self.head[i];
                    while !cur.is_null() {
                        let kv = &mut *cur;
                        std::ptr::drop_in_place(&mut kv.value);
                        cur = kv.next;
                    }
                }
            }

            std::alloc::dealloc(self.head.as_mut_ptr() as *mut u8, self.head_layout);
            std::alloc::dealloc(self.entry.as_mut_ptr() as *mut u8, self.entry_layout);
        }
    }
}


impl<'a, K: Hash + Eq, V> FixedMap<'a, K, V, DefaultHashEq<K>> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_and_hash_eq(capacity, DefaultHashEq::new())
    }
}


impl<'a, K, V, HE: HashEq<K>> FixedMap<'a, K, V, HE> {

    pub fn with_capacity_and_hash_eq(capacity: usize, inst: HE) -> Self
    {
        let po2 = capacity.next_power_of_two();
        let head_layout = Layout::array::<*mut FixedMapEntry<K, V>>(po2).unwrap();
        let entry_layout = Layout::array::<FixedMapEntry<K, V>>(po2).unwrap();

        let ptr1 = unsafe { std::alloc::alloc_zeroed(head_layout) };
        let ptr2 = unsafe { std::alloc::alloc(entry_layout) };

        Self {
            head: unsafe { std::slice::from_raw_parts_mut(ptr1 as *mut *mut FixedMapEntry<K, V>, po2) },
            entry: unsafe { std::slice::from_raw_parts_mut(ptr2 as *mut FixedMapEntry<K, V>, po2) },
            head_layout,
            entry_layout,
            count: 0,
            capacity,
            mask: po2 - 1,
            he: inst,
        }
    }


    fn _push(&mut self, key: K, hash: u64, value: V)  -> *mut FixedMapEntry<K, V> {
        let i = hash as usize & self.mask;
        let dst = &mut self.entry[self.count];
        unsafe {
            std::ptr::write(dst, FixedMapEntry {
                key_hash: hash,
                key,
                value,
                next: self.head[i],
            });
        }
        self.head[i] = dst;
        self.count += 1;
        dst
    }

    fn _find_entry(&self, key: &K, hash: u64) -> *mut FixedMapEntry<K, V> {
        let mut cur = self.head[hash as usize & self.mask];
        while !cur.is_null() {
            let entry = unsafe { &mut *cur };
            if entry.key_hash == hash && self.he.eq(&entry.key, key) {
                return cur;
            }
            cur = entry.next;
        }

        std::ptr::null_mut()
    }

    pub fn contains_key(&self, key: &K) -> bool {
        let hash = self.he.hash(key);
        let entry = self._find_entry(key, hash);
        !entry.is_null()
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let hash = self.he.hash(key);
        let entry = self._find_entry(key, hash);
        if !entry.is_null() {
            Some(unsafe { &(*entry).value })
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let hash = self.he.hash(key);
        let entry = self._find_entry(key, hash);
        if !entry.is_null() {
            Some(unsafe { &mut (*entry).value })
        } else {
            None
        }
    }

    pub fn get_or_insert(&mut self, key: &K, value: V) -> &mut V
    where K: Clone
    {
        let hash = self.he.hash(key);
        let entry = self._find_entry(key, hash);
        if !entry.is_null() {
            unsafe { &mut (*entry).value }
        } else {
            unsafe { &mut (*self._push(key.clone(), hash, value)).value }
        }
    }

    pub fn get_or_default(&mut self, key: &K) -> &mut V
    where K: Clone, V: Default
    {
        let hash = self.he.hash(key);
        let entry = self._find_entry(key, hash);
        if !entry.is_null() {
            unsafe { &mut (*entry).value }
        } else {
            unsafe { &mut (*self._push(key.clone(), hash, V::default())).value }
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> &mut V {
        let hash = self.he.hash(&key);
        let entry = self._find_entry(&key, hash);
        if !entry.is_null() {
            unsafe {
                (*entry).value = value;
                &mut (*entry).value
            }
        } else {
            let node = self._push(key, hash, value);
            unsafe { &mut (*node).value }
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn len(&self) -> usize {
        self.count
    }

}










#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::hash::Hash;
    use std::io::BufRead;
    use std::path::PathBuf;
    use crate::mock::helper::read_test_file;
    use crate::maps::{HashEq, FixedMap};
    use crate::xtypes::{xrecord, xrecord_he};

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
    fn test_chain_map() {
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
