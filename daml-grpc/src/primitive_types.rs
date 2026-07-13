use crate::data::DamlError;
use crate::nat::{Nat, Nat10};
use bigdecimal::BigDecimal;
use chrono::{DateTime, NaiveDate, Utc};
use itertools::Itertools;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::convert::TryFrom;
use std::fmt::Formatter;
use std::iter::FromIterator;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

/// Type alias for a Daml `Int`.
pub type DamlInt64 = i64;

/// Type alias for a Daml `Numeric`.
pub type DamlNumeric = BigDecimal;

/// Type alias for a Daml `Numeric 10`
pub type DamlNumeric10 = DamlFixedNumeric<Nat10>;

/// Type alias for a Daml `Text`.
pub type DamlText = String;

/// Type alias for a Daml `Timestamp`.
pub type DamlTimestamp = DateTime<Utc>;

/// Type alias for a Daml `Bool`.
pub type DamlBool = bool;

/// Type alias for a Daml `Unit`.
pub type DamlUnit = ();

/// Type alias for a Daml `Date`.
///
/// Modelled as a [`chrono::NaiveDate`] — Daml's `Date` carries no
/// timezone (it's a calendar date), so the v0.3 bindings dropped
/// the deprecated `chrono::Date<Utc>` alias from v0.2.
pub type DamlDate = NaiveDate;

/// Type alias for a Daml `List a`.
pub type DamlList<T> = Vec<T>;

/// Type alias for a Daml legacy `TextMap a b`.
pub type DamlTextMap<V> = DamlTextMapImpl<V>;

/// Type alias for a Daml `GenMap a b`.
pub type DamlGenMap<K, V> = BTreeMap<K, V>;

/// Type alias for a Daml `Optional a`.
pub type DamlOptional<T> = Option<T>;

/// A Daml `Party`.
#[derive(Debug, Eq, PartialEq, PartialOrd, Ord, Clone)]
pub struct DamlParty {
    pub(crate) party: String,
}

impl DamlParty {
    pub fn new(party: impl Into<String>) -> Self {
        Self {
            party: party.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        self.party.as_str()
    }
}

impl From<&str> for DamlParty {
    fn from(party: &str) -> Self {
        DamlParty::new(party)
    }
}

impl From<String> for DamlParty {
    fn from(party: String) -> Self {
        DamlParty::new(party)
    }
}

impl PartialEq<&DamlParty> for &str {
    fn eq(&self, other: &&DamlParty) -> bool {
        *self == other.party
    }
}

impl PartialEq<&str> for &DamlParty {
    fn eq(&self, other: &&str) -> bool {
        self.party == *other
    }
}

impl PartialEq<DamlParty> for &str {
    fn eq(&self, other: &DamlParty) -> bool {
        self == &other.party
    }
}

impl PartialEq<&str> for DamlParty {
    fn eq(&self, other: &&str) -> bool {
        &self.party == other
    }
}

impl std::fmt::Display for DamlParty {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.party.fmt(f)
    }
}

/// A raw Daml `ContractId` string.
///
/// **Note:** template-specific phantom-typing is provided one layer up
/// via codegen-emitted per-template newtypes (e.g. `FooContractId
/// { contract_id: DamlContractId }`); `DamlContractId` itself is the
/// universal raw-id carrier used inside `DamlValue::ContractId`.
#[derive(Debug, Eq, PartialEq, PartialOrd, Ord, Clone)]
pub struct DamlContractId {
    pub(crate) contract_id: String,
}

impl DamlContractId {
    pub fn new(contract_id: impl Into<String>) -> Self {
        Self {
            contract_id: contract_id.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        self.contract_id.as_str()
    }
}

impl From<&str> for DamlContractId {
    fn from(contract_id: &str) -> Self {
        DamlContractId::new(contract_id)
    }
}

impl From<String> for DamlContractId {
    fn from(contract_id: String) -> Self {
        DamlContractId::new(contract_id)
    }
}

impl PartialEq<&DamlContractId> for &str {
    fn eq(&self, other: &&DamlContractId) -> bool {
        *self == other.contract_id
    }
}

impl PartialEq<&str> for &DamlContractId {
    fn eq(&self, other: &&str) -> bool {
        self.contract_id == *other
    }
}

impl PartialEq<DamlContractId> for &str {
    fn eq(&self, other: &DamlContractId) -> bool {
        self == &other.contract_id
    }
}

impl PartialEq<&str> for DamlContractId {
    fn eq(&self, other: &&str) -> bool {
        &self.contract_id == other
    }
}

impl std::fmt::Display for DamlContractId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.contract_id.fmt(f)
    }
}

/// A Daml legacy `TextMap a`.
#[derive(Debug, Eq, Default, Clone)]
pub struct DamlTextMapImpl<T>(pub HashMap<DamlText, T>);

impl<T> DamlTextMapImpl<T> {
    pub fn new() -> Self {
        DamlTextMapImpl(HashMap::new())
    }
}

/// Lexicographic order over sorted `(key, value)` pairs.
///
/// Sorts each side by key first, then compares element-by-element. This
/// gives a deterministic ordering that considers both keys AND values —
/// contrast with the pre-0.4 behaviour, which sorted by keys only and
/// ignored values, silently collapsing `{"a" → 1}` and `{"a" → 999}` as
/// equal for `cmp` / `partial_cmp`.
impl<T: PartialOrd> PartialOrd for DamlTextMapImpl<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.0.len() != other.0.len() {
            return self.0.len().partial_cmp(&other.0.len());
        }
        let sorted_self = self.0.iter().sorted_by(|(a, _), (b, _)| a.cmp(b));
        let sorted_other = other.0.iter().sorted_by(|(a, _), (b, _)| a.cmp(b));
        sorted_self.partial_cmp(sorted_other)
    }
}

// `Ord` intentionally not implemented — `T: Ord` is a stricter bound than
// most callers can satisfy (in particular `DamlValue` is `PartialOrd`
// only). Callers who need a total order can wrap the map in a newtype and
// impl `Ord` themselves against the concrete `T`.

/// Delegates to `HashMap`'s value-aware equality (both keys and values
/// must match). See the `PartialOrd` docstring for the pre-0.4 behaviour
/// this replaces.
impl<T: PartialEq> PartialEq for DamlTextMapImpl<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> From<HashMap<DamlText, T>> for DamlTextMapImpl<T> {
    fn from(m: HashMap<DamlText, T>) -> Self {
        Self(m)
    }
}

impl<T> Deref for DamlTextMapImpl<T> {
    type Target = HashMap<DamlText, T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for DamlTextMapImpl<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<V> IntoIterator for DamlTextMapImpl<V> {
    type IntoIter = <HashMap<DamlText, V> as IntoIterator>::IntoIter;
    type Item = (DamlText, V);

    fn into_iter(self) -> Self::IntoIter {
        HashMap::into_iter(self.0)
    }
}

impl<V> FromIterator<(DamlText, V)> for DamlTextMapImpl<V> {
    fn from_iter<T: IntoIterator<Item = (DamlText, V)>>(iter: T) -> DamlTextMapImpl<V> {
        Self::from(HashMap::from_iter(iter))
    }
}

/// A fixed precision numeric type.  Currently a simple wrapper around a `BigDecimal`.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone)]
pub struct DamlFixedNumeric<T: Nat> {
    phantom: PhantomData<T>,
    pub value: BigDecimal,
}

impl<T: Nat> DamlFixedNumeric<T> {
    pub fn new(value: BigDecimal) -> Self {
        Self {
            phantom: PhantomData::<T>,
            value,
        }
    }

    pub fn try_new(f: f64) -> Result<Self, DamlError> {
        Ok(Self::new(BigDecimal::try_from(f)?))
    }
}

/// Convert a f64 to a `DamlFixedNumeric`.
///
/// Note that this is not a fallible conversion and instead panics if the conversion fails.  Use
/// `DamlFixedNumeric::try_new` instead to construct a `DamlFixedNumerical` with returns an error on invalid input.
///
/// Arguable we could use the `TryFrom` trait here however the code generate currently produces entries such as
/// `my_numeric: impl Into<DamlNumeric10>` rather than `TryInto` which has the nice property of avoiding fallible cases.
#[allow(clippy::fallible_impl_from)]
impl<T: Nat> From<f64> for DamlFixedNumeric<T> {
    fn from(f: f64) -> Self {
        Self::new(match BigDecimal::try_from(f) {
            Ok(bd) => bd,
            Err(err) => panic!("invalid f64: {err}"),
        })
    }
}

impl<T: Nat> FromStr for DamlFixedNumeric<T> {
    type Err = DamlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(FromStr::from_str(s)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numeric_try_new() {
        let num = DamlNumeric10::try_new(1.2_f64).unwrap();
        assert_eq!(DamlNumeric10::new(BigDecimal::try_from(1.2_f64).unwrap()), num);
    }

    #[test]
    fn test_numeric_try_new_nan() {
        let num = DamlNumeric10::try_new(1_f64 / 0_f64);
        assert!(num.is_err());
    }

    #[test]
    fn test_numeric_from() {
        let num = DamlNumeric10::from(1.2_f64);
        assert_eq!(DamlNumeric10::new(BigDecimal::try_from(1.2_f64).unwrap()), num);
    }

    #[test]
    #[should_panic(expected = "invalid f64")]
    fn test_numeric_from_should_panic() {
        let _panic = DamlNumeric10::from(1_f64 / 0_f64);
    }

    #[test]
    fn test_daml_text_map_identical_pairs_are_equal() {
        let map1: DamlTextMap<DamlInt64> = vec![("key1".into(), 10), ("key2".into(), 20)].into_iter().collect();
        let map2: DamlTextMap<DamlInt64> = vec![("key1".into(), 10), ("key2".into(), 20)].into_iter().collect();
        assert_eq!(map1, map2);
    }

    #[test]
    fn test_daml_text_map_same_keys_different_values_are_not_equal() {
        // Pre-0.4 this incorrectly reported the two maps as equal (keys-
        // only comparison). The new PartialEq delegates to HashMap's, so
        // value differences count.
        let map1: DamlTextMap<DamlInt64> = vec![("key1".into(), 10), ("key2".into(), 20)].into_iter().collect();
        let map2: DamlTextMap<DamlInt64> = vec![("key1".into(), 100), ("key2".into(), 200)].into_iter().collect();
        assert_ne!(map1, map2);
    }

    #[test]
    fn test_daml_text_map_not_equal_keys() {
        let map1: DamlTextMap<DamlInt64> = vec![("key1".into(), 10), ("key2".into(), 20)].into_iter().collect();
        let map2: DamlTextMap<DamlInt64> = vec![("key3".into(), 10), ("key4".into(), 20)].into_iter().collect();
        assert_ne!(map1, map2);
    }

    #[test]
    fn test_daml_text_map_partial_cmp_considers_values() {
        // Same keys, different values — partial_cmp now walks
        // (key, value) pairs and reports the true relation instead of
        // returning Equal.
        let map1: DamlTextMap<DamlInt64> = vec![("key1".into(), 10), ("key2".into(), 20)].into_iter().collect();
        let map2: DamlTextMap<DamlInt64> = vec![("key1".into(), 10), ("key2".into(), 20)].into_iter().collect();
        let map3: DamlTextMap<DamlInt64> = vec![("key1".into(), 100), ("key2".into(), 200)].into_iter().collect();
        assert_eq!(Some(Ordering::Equal), map1.partial_cmp(&map2));
        assert_eq!(Some(Ordering::Less), map1.partial_cmp(&map3));
        assert_eq!(Some(Ordering::Greater), map3.partial_cmp(&map1));
    }

    #[test]
    fn test_daml_text_map_partial_cmp_orders_by_size_then_keys() {
        let small: DamlTextMap<DamlInt64> = vec![("k".into(), 1)].into_iter().collect();
        let big: DamlTextMap<DamlInt64> = vec![("a".into(), 1), ("b".into(), 1)].into_iter().collect();
        assert_eq!(Some(Ordering::Less), small.partial_cmp(&big));

        let a_first: DamlTextMap<DamlInt64> = vec![("a".into(), 1)].into_iter().collect();
        let b_first: DamlTextMap<DamlInt64> = vec![("b".into(), 1)].into_iter().collect();
        assert_eq!(Some(Ordering::Less), a_first.partial_cmp(&b_first));
    }
}
