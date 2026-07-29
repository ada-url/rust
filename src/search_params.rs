//! WHATWG URLSearchParams.

use alloc::{boxed::Box, string::String, vec::Vec};
use core::{
    fmt,
    hash::{Hash, Hasher},
};

use crate::ParseUrlError;

/// An ordered list of URL query name/value pairs.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct UrlSearchParams {
    pairs: Vec<(String, String)>,
}

impl UrlSearchParams {
    /// Parses an `application/x-www-form-urlencoded` query.
    pub fn parse<Input>(input: Input) -> Result<Self, ParseUrlError<Input>>
    where
        Input: AsRef<str>,
    {
        Ok(Self::new(input.as_ref()))
    }

    /// Parses an `application/x-www-form-urlencoded` query.
    #[must_use]
    pub fn new(input: &str) -> Self {
        let input = input.strip_prefix('?').unwrap_or(input);
        let pairs = url::form_urlencoded::parse(input.as_bytes())
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        Self { pairs }
    }

    /// Returns the number of pairs.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Returns whether there are no pairs.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Appends a pair.
    pub fn append(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.pairs.push((name.into(), value.into()));
    }

    /// Removes every pair with `name`.
    pub fn delete(&mut self, name: &str) {
        self.pairs.retain(|(key, _)| key != name);
    }

    /// Removes every pair matching both `name` and `value`.
    pub fn delete_value(&mut self, name: &str, value: &str) {
        self.pairs
            .retain(|(key, candidate)| key != name || candidate != value);
    }

    /// Removes every pair with `name`.
    pub fn remove_key(&mut self, name: &str) {
        self.delete(name);
    }

    /// Removes every pair matching both `name` and `value`.
    pub fn remove(&mut self, name: &str, value: &str) {
        self.delete_value(name, value);
    }

    /// Returns the first value associated with `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find_map(|(key, value)| (key == name).then_some(value.as_str()))
    }

    /// Returns all values associated with `name`.
    pub fn get_all<'a>(&'a self, name: &'a str) -> UrlSearchParamsEntry<'a> {
        let values = self
            .pairs
            .iter()
            .filter_map(move |(key, value)| (key == name).then_some(value.as_str()))
            .collect();
        UrlSearchParamsEntry {
            values,
            position: 0,
        }
    }

    /// Returns whether at least one pair has `name`.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.pairs.iter().any(|(key, _)| key == name)
    }

    /// Returns whether a pair matches both `name` and `value`.
    #[must_use]
    pub fn has_value(&self, name: &str, value: &str) -> bool {
        self.pairs
            .iter()
            .any(|(key, candidate)| key == name && candidate == value)
    }

    /// Returns whether at least one pair has `name`.
    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.has(name)
    }

    /// Returns whether a pair matches both `name` and `value`.
    #[must_use]
    pub fn contains(&self, name: &str, value: &str) -> bool {
        self.has_value(name, value)
    }

    /// Replaces the first value for `name` and removes later duplicates.
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        let value = value.into();
        let mut found = false;
        self.pairs.retain_mut(|(key, candidate)| {
            if key != &name {
                return true;
            }
            if found {
                return false;
            }
            *candidate = value.clone();
            found = true;
            true
        });
        if !found {
            self.pairs.push((name, value));
        }
    }

    /// Sorts pairs stably by name using UTF-16 code units.
    pub fn sort(&mut self) {
        self.pairs
            .sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
    }

    /// Iterates over pairs in insertion order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &str)> {
        self.pairs
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    /// Iterates over names in insertion order.
    pub fn keys(&self) -> UrlSearchParamsKeyIterator<'_> {
        UrlSearchParamsKeyIterator {
            inner: self.pairs.iter(),
        }
    }

    /// Iterates over values in insertion order.
    pub fn values(&self) -> UrlSearchParamsValueIterator<'_> {
        UrlSearchParamsValueIterator {
            inner: self.pairs.iter(),
        }
    }

    /// Iterates over pairs in insertion order.
    pub fn entries(&self) -> UrlSearchParamsEntryIterator<'_> {
        UrlSearchParamsEntryIterator {
            inner: self.pairs.iter(),
        }
    }

    /// Returns the first pair.
    #[must_use]
    pub fn front(&self) -> Option<(&str, &str)> {
        self.pairs
            .first()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    /// Returns the last pair.
    #[must_use]
    pub fn back(&self) -> Option<(&str, &str)> {
        self.pairs
            .last()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    /// Returns a pair by insertion-order index.
    #[must_use]
    pub fn get_index(&self, index: usize) -> Option<(&str, &str)> {
        self.pairs
            .get(index)
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    /// Replaces all pairs by parsing a new query.
    pub fn reset(&mut self, input: &str) {
        *self = Self::new(input);
    }
}

impl fmt::Display for UrlSearchParams {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        serializer.extend_pairs(self.iter());
        formatter.write_str(&serializer.finish())
    }
}

impl core::str::FromStr for UrlSearchParams {
    type Err = ParseUrlError<Box<str>>;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input).map_err(|ParseUrlError { input }| ParseUrlError {
            input: input.into(),
        })
    }
}

/// Iterator over URL search-parameter keys.
pub struct UrlSearchParamsKeyIterator<'a> {
    inner: core::slice::Iter<'a, (String, String)>,
}

impl<'a> Iterator for UrlSearchParamsKeyIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(name, _)| name.as_str())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for UrlSearchParamsKeyIterator<'_> {}

impl Hash for UrlSearchParamsKeyIterator<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for (name, _) in self.inner.as_slice() {
            name.hash(state);
        }
    }
}

impl Drop for UrlSearchParamsKeyIterator<'_> {
    fn drop(&mut self) {}
}

/// Iterator over URL search-parameter values.
pub struct UrlSearchParamsValueIterator<'a> {
    inner: core::slice::Iter<'a, (String, String)>,
}

impl<'a> Iterator for UrlSearchParamsValueIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(_, value)| value.as_str())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for UrlSearchParamsValueIterator<'_> {}

impl Hash for UrlSearchParamsValueIterator<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for (_, value) in self.inner.as_slice() {
            value.hash(state);
        }
    }
}

impl Drop for UrlSearchParamsValueIterator<'_> {
    fn drop(&mut self) {}
}

/// Iterator over URL search-parameter pairs.
pub struct UrlSearchParamsEntryIterator<'a> {
    inner: core::slice::Iter<'a, (String, String)>,
}

impl<'a> Iterator for UrlSearchParamsEntryIterator<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl ExactSizeIterator for UrlSearchParamsEntryIterator<'_> {}

impl Hash for UrlSearchParamsEntryIterator<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.as_slice().hash(state);
    }
}

impl Drop for UrlSearchParamsEntryIterator<'_> {
    fn drop(&mut self) {}
}

/// Values associated with one URL search-parameter key.
pub struct UrlSearchParamsEntry<'a> {
    values: Vec<&'a str>,
    position: usize,
}

impl UrlSearchParamsEntry<'_> {
    /// Returns whether no values matched.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns the number of matched values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns a matched value by index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&str> {
        self.values.get(index).copied()
    }
}

impl<'a> Iterator for UrlSearchParamsEntry<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let value = self.values.get(self.position).copied();
        self.position += usize::from(value.is_some());
        value
    }
}

impl<'a> From<UrlSearchParamsEntry<'a>> for Vec<&'a str> {
    fn from(entry: UrlSearchParamsEntry<'a>) -> Self {
        entry.values
    }
}

impl<'a> IntoIterator for &'a UrlSearchParams {
    type Item = (&'a str, &'a str);
    type IntoIter = core::iter::Map<
        core::slice::Iter<'a, (String, String)>,
        fn(&(String, String)) -> (&str, &str),
    >;

    fn into_iter(self) -> Self::IntoIter {
        fn as_pair(pair: &(String, String)) -> (&str, &str) {
            (&pair.0, &pair.1)
        }
        self.pairs.iter().map(as_pair)
    }
}

impl IntoIterator for UrlSearchParams {
    type Item = (String, String);
    type IntoIter = alloc::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.pairs.into_iter()
    }
}

impl<K, V> FromIterator<(K, V)> for UrlSearchParams
where
    K: Into<String>,
    V: Into<String>,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self {
            pairs: iter
                .into_iter()
                .map(|(name, value)| (name.into(), value.into()))
                .collect(),
        }
    }
}

impl<K, V> Extend<(K, V)> for UrlSearchParams
where
    K: Into<String>,
    V: Into<String>,
{
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        self.pairs.extend(
            iter.into_iter()
                .map(|(name, value)| (name.into(), value.into())),
        );
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for UrlSearchParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for UrlSearchParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let input = <String as serde::Deserialize>::deserialize(deserializer)?;
        Ok(Self::new(&input))
    }
}

#[cfg(test)]
mod tests {
    use alloc::{string::ToString, vec::Vec};

    use super::UrlSearchParams;

    #[test]
    fn parses_and_serializes() {
        let mut params = UrlSearchParams::new("?a=b+c&a=d&empty");
        assert_eq!(params.get("a"), Some("b c"));
        assert_eq!(params.get_all("a").collect::<Vec<_>>(), ["b c", "d"]);
        params.set("a", "x");
        params.append("snow", "☃");
        assert_eq!(params.to_string(), "a=x&empty=&snow=%E2%98%83");
    }

    #[test]
    fn sort_is_stable() {
        let mut params = UrlSearchParams::new("z=1&a=first&a=second");
        params.sort();
        assert_eq!(params.to_string(), "a=first&a=second&z=1");
    }
}
