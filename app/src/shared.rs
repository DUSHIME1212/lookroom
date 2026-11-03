//! Two small structures the app leans on for speed.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// Copy-on-write data. Copying is one reference count, so an undo step or a snapshot for
/// another thread costs nothing, and only what changes gets duplicated. Two values that
/// were never edited apart compare equal by pointer, without looking inside.
#[derive(Debug)]
pub struct Shared<T>(Arc<T>);

impl<T> Shared<T> {
    pub fn new(v: T) -> Self {
        Self(Arc::new(v))
    }

    /// Whether these are one and the same data, which is cheaper than comparing it.
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// Any write takes its own copy first, so nothing sharing the old value sees the change.
impl<T: Clone> DerefMut for Shared<T> {
    fn deref_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }
}

impl<T: PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        self.same(other) || *self.0 == *other.0
    }
}

impl<T: Default> Default for Shared<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for Shared<T> {
    fn from(v: T) -> Self {
        Self::new(v)
    }
}

impl<T: Serialize> Serialize for Shared<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Shared<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(d).map(Self::new)
    }
}

/// Keeps the most recently used values and drops the oldest. Lookup is a hash; finding
/// the oldest is the first key of an ordered map, so neither gets slower as it fills.
pub struct Lru<K, V> {
    limit: usize,
    tick: u64,
    values: HashMap<K, (V, u64)>,
    ages: BTreeMap<u64, K>,
}

impl<K: Hash + Eq + Copy, V: Clone> Lru<K, V> {
    pub fn new(limit: usize) -> Self {
        Self { limit: limit.max(1), tick: 0, values: HashMap::new(), ages: BTreeMap::new() }
    }

    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    pub fn get(&mut self, key: &K) -> Option<V> {
        let tick = self.next_tick();
        let (v, age) = self.values.get_mut(key)?;
        self.ages.remove(age);
        *age = tick;
        self.ages.insert(tick, *key);
        Some(v.clone())
    }

    pub fn contains(&self, key: &K) -> bool {
        self.values.contains_key(key)
    }

    pub fn put(&mut self, key: K, value: V) {
        let tick = self.next_tick();
        if let Some((_, old)) = self.values.insert(key, (value, tick)) {
            self.ages.remove(&old);
        }
        self.ages.insert(tick, key);
        while self.values.len() > self.limit {
            let Some((_, oldest)) = self.ages.pop_first() else { break };
            self.values.remove(&oldest);
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.values.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_is_the_same_data_until_one_side_writes() {
        let a = Shared::new(vec![1, 2, 3]);
        let mut b = a.clone();
        assert!(a.same(&b));
        b.push(4);
        assert!(!a.same(&b));
        assert_eq!(*a, [1, 2, 3]);
        assert_eq!(*b, [1, 2, 3, 4]);
    }

    #[test]
    fn equal_contents_compare_equal_even_when_apart() {
        let a = Shared::new(vec![1, 2]);
        let mut b = a.clone();
        b.push(3);
        b.pop();
        assert!(!a.same(&b));
        assert_eq!(a, b);
    }

    #[test]
    fn it_saves_as_plain_data() {
        let a = Shared::new(vec![1, 2]);
        let text = serde_json::to_string(&a).unwrap();
        assert_eq!(text, "[1,2]");
        assert_eq!(serde_json::from_str::<Shared<Vec<i32>>>(&text).unwrap(), a);
    }

    #[test]
    fn the_oldest_value_goes_first() {
        let mut c = Lru::new(2);
        c.put(1, "a");
        c.put(2, "b");
        c.put(3, "c");
        assert!(!c.contains(&1));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn using_a_value_keeps_it() {
        let mut c = Lru::new(2);
        c.put(1, "a");
        c.put(2, "b");
        assert_eq!(c.get(&1), Some("a"));
        c.put(3, "c");
        assert!(c.contains(&1));
        assert!(!c.contains(&2));
    }

    #[test]
    fn replacing_a_value_doesnt_leave_a_stale_age() {
        let mut c = Lru::new(2);
        c.put(1, "a");
        c.put(1, "b");
        c.put(2, "c");
        c.put(3, "d");
        assert_eq!(c.len(), 2);
        assert!(!c.contains(&1));
    }
}
