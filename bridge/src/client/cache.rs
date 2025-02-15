use std::{borrow::Borrow, collections::HashMap, hash::Hash, sync::LazyLock};

use crate::connectors::connector_c::TaprootSpendInfoCache;

pub(crate) static TAPROOT_SPEND_INFO_CACHE: LazyLock<Cache<String, TaprootSpendInfoCache>> =
    LazyLock::new(|| Cache::new());

#[derive(PartialEq, Clone)]
pub struct Cache<K: Eq + Hash, V>(HashMap<K, V>);

impl<K, V> Cache<K, V>
where
    K: Eq + Hash,
{
    fn new() -> Self { Self(HashMap::with_capacity(200)) }

    pub fn push(&mut self, key: K, value: V) -> Option<V> { self.0.insert(key, value) }

    pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.0.get(key)
    }
}
