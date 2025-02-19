use std::{
    collections::HashMap,
    hash::Hash,
    sync::{LazyLock, RwLock},
};

use crate::connectors::base::TaprootSpendInfoCache;

const DEFAULT_CACHE_SIZE: usize = 200;
pub(crate) static TAPROOT_SPEND_INFO_CACHE: LazyLock<RwLock<Cache<String, TaprootSpendInfoCache>>> =
    LazyLock::new(|| RwLock::new(Cache::new(DEFAULT_CACHE_SIZE)));

pub struct Cache<K: Eq + Hash, V>(HashMap<K, V>);

impl<K, V> Cache<K, V>
where
    K: Eq + Hash,
{
    fn new(capacity: usize) -> Self { Self(HashMap::with_capacity(capacity)) }

    pub fn push(&mut self, key: K, value: V) -> Option<V> { self.0.insert(key, value) }

    pub fn get(&self, key: &K) -> Option<&V>
    {
        self.0.get(key)
    }

    pub fn contains(&self, key: &K) -> bool
    {
        self.0.contains_key(key)
    }
}
