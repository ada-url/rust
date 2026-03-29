//! Pure-Rust implementation of the WHATWG URLSearchParams API.
//! Ref: <https://url.spec.whatwg.org/#interface-urlsearchparams>

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

#[cfg(not(feature = "std"))]
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
#[cfg(feature = "std")]
use std::{string::String, vec::Vec};

use crate::ParseUrlError;
use crate::character_sets::WWW_FORM_URLENCODED_PERCENT_ENCODE;
use crate::unicode::percent_encode;

// ---------------------------------------------------------------------------
// application/x-www-form-urlencoded decode
// ---------------------------------------------------------------------------

/// Decode a single application/x-www-form-urlencoded byte sequence.
/// '+' → space (before percent-decoding), percent sequences → raw bytes → UTF-8.
fn urldecode(s: &str) -> String {
    use crate::unicode::{convert_hex_to_binary, is_ascii_hex_digit};

    let src = s.as_bytes();
    // Fast path: no '+' and no '%' — return as-is
    if !src.iter().any(|&b| b == b'+' || b == b'%') {
        return s.to_string();
    }

    // Decode to raw bytes first
    let mut raw: Vec<u8> = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        let b = src[i];
        if b == b'+' {
            raw.push(b' ');
            i += 1;
        } else if b == b'%'
            && i + 2 < src.len()
            && is_ascii_hex_digit(src[i + 1])
            && is_ascii_hex_digit(src[i + 2])
        {
            let val = convert_hex_to_binary(src[i + 1]) * 16 + convert_hex_to_binary(src[i + 2]);
            raw.push(val);
            i += 3;
        } else {
            raw.push(b);
            i += 1;
        }
    }
    // Interpret raw bytes as UTF-8; fall back to lossy if invalid
    String::from_utf8(raw).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

// ---------------------------------------------------------------------------
// application/x-www-form-urlencoded encode (WHATWG)
// ---------------------------------------------------------------------------

fn urlencode(s: &str) -> String {
    // First percent-encode using WWW_FORM set, then replace ' ' with '+'
    let encoded = percent_encode(s, &WWW_FORM_URLENCODED_PERCENT_ENCODE);
    // Replace encoded spaces '%20' are handled - but spaces in input (0x20)
    // are NOT in WWW_FORM_URLENCODED set; they should become '+'.
    // The WWW_FORM set does not include space (0x20), so space passes through
    // as ' '. We need to convert those to '+' after encoding.
    encoded
        .chars()
        .map(|c| if c == ' ' { '+' } else { c })
        .collect()
}

// ---------------------------------------------------------------------------
// UrlSearchParams
// ---------------------------------------------------------------------------

/// A list of name-value pairs.
#[derive(Debug, Clone, Hash)]
pub struct UrlSearchParams {
    list: Vec<(String, String)>,
}

impl UrlSearchParams {
    /// Parse a URL search string (with or without leading '?').
    pub fn parse<Input>(input: Input) -> Result<Self, ParseUrlError<Input>>
    where
        Input: AsRef<str>,
    {
        let s = input.as_ref();
        let s = s.strip_prefix('?').unwrap_or(s);
        Ok(Self {
            list: parse_urlencoded(s),
        })
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn sort(&mut self) {
        // Stable sort by name using UTF-16 code unit order (WHATWG spec).
        // This differs from Unicode scalar order for supplementary characters
        // (code points > U+FFFF), where UTF-16 uses surrogate pairs.
        // Compare lazily by iterating both encodings to avoid allocating a
        // Vec<u16> for every comparison call.
        self.list.sort_by(|a, b| {
            let mut ai = a.0.encode_utf16();
            let mut bi = b.0.encode_utf16();
            loop {
                match (ai.next(), bi.next()) {
                    (Some(x), Some(y)) => {
                        let ord = x.cmp(&y);
                        if ord != core::cmp::Ordering::Equal {
                            return ord;
                        }
                    }
                    (None, None) => return core::cmp::Ordering::Equal,
                    (None, _) => return core::cmp::Ordering::Less,
                    (_, None) => return core::cmp::Ordering::Greater,
                }
            }
        });
    }

    pub fn append(&mut self, key: &str, value: &str) {
        self.list.push((key.to_string(), value.to_string()));
    }

    pub fn set(&mut self, key: &str, value: &str) {
        // Replace the first matching entry and remove the rest
        let mut first = true;
        self.list.retain_mut(|(k, v)| {
            if k == key {
                if first {
                    first = false;
                    *v = value.to_string();
                    true
                } else {
                    false
                }
            } else {
                true
            }
        });
        if first {
            // key wasn't found at all
            self.list.push((key.to_string(), value.to_string()));
        }
    }

    pub fn remove_key(&mut self, key: &str) {
        self.list.retain(|(k, _)| k != key);
    }

    pub fn remove(&mut self, key: &str, value: &str) {
        self.list.retain(|(k, v)| !(k == key && v == value));
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.list.iter().any(|(k, _)| k == key)
    }

    pub fn contains(&self, key: &str, value: &str) -> bool {
        self.list.iter().any(|(k, v)| k == key && v == value)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.list
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn get_all(&self, key: &str) -> UrlSearchParamsEntry<'_> {
        let values: Vec<&str> = self
            .list
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .collect();
        UrlSearchParamsEntry { values }
    }

    pub fn keys(&self) -> UrlSearchParamsKeyIterator<'_> {
        UrlSearchParamsKeyIterator {
            list: &self.list,
            index: 0,
        }
    }

    pub fn values(&self) -> UrlSearchParamsValueIterator<'_> {
        UrlSearchParamsValueIterator {
            list: &self.list,
            index: 0,
        }
    }

    pub fn entries(&self) -> UrlSearchParamsEntryIterator<'_> {
        UrlSearchParamsEntryIterator {
            list: &self.list,
            index: 0,
        }
    }

    /// Serialize to application/x-www-form-urlencoded.
    pub fn to_string_impl(&self) -> String {
        let mut out = String::new();
        for (i, (key, value)) in self.list.iter().enumerate() {
            if i > 0 {
                out.push('&');
            }
            out.push_str(&urlencode(key));
            out.push('=');
            out.push_str(&urlencode(value));
        }
        out
    }
}

fn parse_urlencoded(s: &str) -> Vec<(String, String)> {
    if s.is_empty() {
        return Vec::new();
    }
    let mut list = Vec::new();
    for pair in s.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = if let Some(eq) = pair.find('=') {
            (&pair[..eq], &pair[eq + 1..])
        } else {
            (pair, "")
        };
        list.push((urldecode(name), urldecode(value)));
    }
    list
}

// ---------------------------------------------------------------------------
// String serialization
// ---------------------------------------------------------------------------

impl core::fmt::Display for UrlSearchParams {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_string_impl())
    }
}

// ---------------------------------------------------------------------------
// FromStr
// ---------------------------------------------------------------------------

#[cfg(feature = "std")]
impl core::str::FromStr for UrlSearchParams {
    type Err = ParseUrlError<Box<str>>;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).map_err(|ParseUrlError { input }| ParseUrlError {
            input: input.into(),
        })
    }
}

// ---------------------------------------------------------------------------
// Extend / FromIterator
// ---------------------------------------------------------------------------

#[cfg(feature = "std")]
impl<Input> Extend<(Input, Input)> for UrlSearchParams
where
    Input: AsRef<str>,
{
    fn extend<T: IntoIterator<Item = (Input, Input)>>(&mut self, iter: T) {
        for (k, v) in iter {
            self.append(k.as_ref(), v.as_ref());
        }
    }
}

#[cfg(feature = "std")]
impl<Input> FromIterator<(Input, Input)> for UrlSearchParams
where
    Input: AsRef<str>,
{
    fn from_iter<T: IntoIterator<Item = (Input, Input)>>(iter: T) -> Self {
        let mut params = UrlSearchParams { list: Vec::new() };
        for (k, v) in iter {
            params.append(k.as_ref(), v.as_ref());
        }
        params
    }
}

// ---------------------------------------------------------------------------
// Iterators
// ---------------------------------------------------------------------------

/// A snapshot of all values for a given key.
pub struct UrlSearchParamsEntry<'a> {
    values: Vec<&'a str>,
}

impl<'a> UrlSearchParamsEntry<'a> {
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<&str> {
        self.values.get(index).copied()
    }
}

#[cfg(feature = "std")]
impl<'a> From<UrlSearchParamsEntry<'a>> for Vec<&'a str> {
    fn from(val: UrlSearchParamsEntry<'a>) -> Self {
        val.values
    }
}

// Key iterator
#[derive(Hash)]
pub struct UrlSearchParamsKeyIterator<'a> {
    list: &'a Vec<(String, String)>,
    index: usize,
}

impl<'a> Iterator for UrlSearchParamsKeyIterator<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.list.get(self.index)?;
        self.index += 1;
        Some(&item.0)
    }
}

// Value iterator
#[derive(Hash)]
pub struct UrlSearchParamsValueIterator<'a> {
    list: &'a Vec<(String, String)>,
    index: usize,
}

impl<'a> Iterator for UrlSearchParamsValueIterator<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.list.get(self.index)?;
        self.index += 1;
        Some(&item.1)
    }
}

// Entry iterator
#[derive(Hash)]
pub struct UrlSearchParamsEntryIterator<'a> {
    list: &'a Vec<(String, String)>,
    index: usize,
}

impl<'a> Iterator for UrlSearchParamsEntryIterator<'a> {
    type Item = (&'a str, &'a str);
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.list.get(self.index)?;
        self.index += 1;
        Some((&item.0, &item.1))
    }
}
