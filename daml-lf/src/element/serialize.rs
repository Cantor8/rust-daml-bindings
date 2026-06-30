use itertools::Itertools;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use std::collections::HashMap;

/// Custom serialization for `HashMap` with stable ordering of keys.
///
/// Sorts (k, v) references in-place via itertools rather than
/// materialising an intermediate `BTreeMap`.
pub fn serialize_map<S, K: Ord + Serialize, V: Serialize>(
    value: &HashMap<K, V>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map = serializer.serialize_map(Some(value.len()))?;
    for (k, v) in value.iter().sorted_by_key(|(k, _)| *k) {
        map.serialize_entry(k, v)?;
    }
    map.end()
}
