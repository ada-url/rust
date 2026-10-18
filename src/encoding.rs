//! WHATWG percent-encoding helpers.

use alloc::{borrow::Cow, string::String, vec::Vec};
use core::{fmt, str::Bytes};

const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// A WHATWG percent-encode set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum PercentEncodeSet {
    /// C0 controls and all non-ASCII bytes.
    C0Control,
    /// The fragment percent-encode set.
    Fragment,
    /// The query percent-encode set.
    Query,
    /// The special-query percent-encode set.
    SpecialQuery,
    /// The path percent-encode set.
    Path,
    /// The userinfo percent-encode set.
    UserInfo,
    /// The component percent-encode set.
    Component,
    /// The `application/x-www-form-urlencoded` percent-encode set.
    FormUrlencoded,
}

impl PercentEncodeSet {
    #[inline]
    const fn mask(self) -> &'static [u32; 4] {
        match self {
            Self::C0Control => &C0_CONTROL_MASK,
            Self::Fragment => &FRAGMENT_MASK,
            Self::Query => &QUERY_MASK,
            Self::SpecialQuery => &SPECIAL_QUERY_MASK,
            Self::Path => &PATH_MASK,
            Self::UserInfo => &USERINFO_MASK,
            Self::Component => &COMPONENT_MASK,
            Self::FormUrlencoded => &FORM_URLENCODED_MASK,
        }
    }
}

const fn add(mut mask: [u32; 4], byte: u8) -> [u32; 4] {
    mask[byte as usize / 32] |= 1 << (byte as usize % 32);
    mask
}

const fn add_bytes(mut mask: [u32; 4], bytes: &[u8]) -> [u32; 4] {
    let mut index = 0;
    while index < bytes.len() {
        mask = add(mask, bytes[index]);
        index += 1;
    }
    mask
}

#[inline]
const fn mask_contains(mask: &[u32; 4], byte: u8) -> bool {
    mask[byte as usize / 32] & (1 << (byte as usize % 32)) != 0
}

const C0_CONTROL_MASK: [u32; 4] = [u32::MAX, 0, 0, 1 << 31];
const FRAGMENT_MASK: [u32; 4] = add_bytes(C0_CONTROL_MASK, b" \"<>`");
const QUERY_MASK: [u32; 4] = add_bytes(C0_CONTROL_MASK, b" \"#<>");
const SPECIAL_QUERY_MASK: [u32; 4] = add(QUERY_MASK, b'\'');
const PATH_MASK: [u32; 4] = add_bytes(C0_CONTROL_MASK, b" \"#<>?^`{}");
const USERINFO_MASK: [u32; 4] = add_bytes(PATH_MASK, b"/:;=@[\\]|");
const COMPONENT_MASK: [u32; 4] = add_bytes(USERINFO_MASK, b"$%&+,");

const fn form_urlencoded_mask() -> [u32; 4] {
    let mut mask = [u32::MAX; 4];
    let mut byte = b'0';
    while byte <= b'9' {
        mask[byte as usize / 32] &= !(1 << (byte as usize % 32));
        byte += 1;
    }
    byte = b'A';
    while byte <= b'Z' {
        mask[byte as usize / 32] &= !(1 << (byte as usize % 32));
        byte += 1;
    }
    byte = b'a';
    while byte <= b'z' {
        mask[byte as usize / 32] &= !(1 << (byte as usize % 32));
        byte += 1;
    }
    for_remove(mask, b"*-._")
}

const fn for_remove(mut mask: [u32; 4], bytes: &[u8]) -> [u32; 4] {
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        mask[byte as usize / 32] &= !(1 << (byte as usize % 32));
        index += 1;
    }
    mask
}

const FORM_URLENCODED_MASK: [u32; 4] = form_urlencoded_mask();

/// Percent-encodes UTF-8 using a WHATWG encode set.
#[must_use]
pub fn percent_encode(input: &str, set: PercentEncodeSet) -> String {
    let mut output = String::with_capacity(input.len());
    append_percent_encoded(&mut output, input, set);
    output
}

/// Appends a percent-encoded UTF-8 string without an intermediate allocation.
#[inline]
pub(crate) fn append_percent_encoded(output: &mut String, input: &str, set: PercentEncodeSet) {
    let mask = set.mask();
    // Normalized web URLs overwhelmingly contain no byte from the active
    // encode set. Scan bytes first so this case becomes one vectorizable pass
    // followed by one bulk copy instead of UTF-8 decoding every character.
    if input
        .as_bytes()
        .iter()
        .all(|byte| *byte < 0x80 && !mask_contains(mask, *byte))
    {
        output.push_str(input);
        return;
    }

    let mut copied = 0;
    for (index, character) in input.char_indices() {
        let byte = character as u32;
        if byte >= 0x80 || mask_contains(mask, byte as u8) {
            output.push_str(&input[copied..index]);
            let mut utf8 = [0; 4];
            for &encoded in character.encode_utf8(&mut utf8).as_bytes() {
                output.push('%');
                output.push(char::from(HEX[usize::from(encoded >> 4)]));
                output.push(char::from(HEX[usize::from(encoded & 0x0f)]));
            }
            copied = index + character.len_utf8();
        }
    }
    output.push_str(&input[copied..]);
}

/// Appends one raw filesystem path segment for a special `file:` URL.
#[cfg(feature = "std")]
pub(crate) fn append_file_path_segment(output: &mut String, input: &[u8]) {
    for &byte in input {
        if byte >= 0x80
            || byte <= 0x1f
            || byte == 0x7f
            || matches!(
                byte,
                b' ' | b'"' | b'#' | b'%' | b'/' | b'<' | b'>' | b'?' | b'\\' | b'`' | b'{' | b'}'
            )
        {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        } else {
            output.push(char::from(byte));
        }
    }
}

/// Lazily percent-encodes UTF-8 for direct extension into a destination.
pub(crate) fn utf8_percent_encode(input: &str, set: PercentEncodeSet) -> PercentEncoded<'_> {
    PercentEncoded {
        bytes: input.bytes(),
        mask: set.mask(),
        pending: [0; 2],
        pending_index: 2,
    }
}

pub(crate) struct PercentEncoded<'a> {
    bytes: Bytes<'a>,
    mask: &'static [u32; 4],
    pending: [u8; 2],
    pending_index: usize,
}

impl Clone for PercentEncoded<'_> {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            mask: self.mask,
            pending: self.pending,
            pending_index: self.pending_index,
        }
    }
}

impl Iterator for PercentEncoded<'_> {
    type Item = char;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.pending_index < 2 {
            let byte = self.pending[self.pending_index];
            self.pending_index += 1;
            return Some(char::from(byte));
        }
        let byte = self.bytes.next()?;
        if byte < 0x80 && !mask_contains(self.mask, byte) {
            return Some(char::from(byte));
        }
        self.pending = [HEX[usize::from(byte >> 4)], HEX[usize::from(byte & 0x0f)]];
        self.pending_index = 0;
        Some('%')
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.bytes.len() + (2 - self.pending_index);
        (remaining, remaining.checked_mul(3))
    }
}

impl fmt::Display for PercentEncoded<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for character in self.clone() {
            let mut utf8 = [0; 4];
            formatter.write_str(character.encode_utf8(&mut utf8))?;
        }
        Ok(())
    }
}

/// Appends one `application/x-www-form-urlencoded` component.
pub(crate) fn append_form_component(output: &mut String, input: &str) {
    let mask = PercentEncodeSet::FormUrlencoded.mask();
    for byte in input.bytes() {
        if byte == b' ' {
            output.push('+');
        } else if byte >= 0x80 || mask_contains(mask, byte) {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        } else {
            output.push(char::from(byte));
        }
    }
}

/// Percent-decodes a UTF-8 string, replacing malformed UTF-8 with U+FFFD.
#[must_use]
pub fn percent_decode(input: &str) -> String {
    match decode_bytes(input) {
        Cow::Borrowed(_) => String::from(input),
        Cow::Owned(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
    }
}

/// Strictly percent-decodes UTF-8, borrowing when no escapes are present.
pub(crate) fn percent_decode_utf8(input: &str) -> Option<Cow<'_, str>> {
    match decode_bytes(input) {
        Cow::Borrowed(_) => Some(Cow::Borrowed(input)),
        Cow::Owned(bytes) => String::from_utf8(bytes).ok().map(Cow::Owned),
    }
}

fn decode_bytes(input: &str) -> Cow<'_, [u8]> {
    let bytes = input.as_bytes();
    let Some(first_percent) = bytes.iter().position(|byte| *byte == b'%') else {
        return Cow::Borrowed(bytes);
    };
    let mut output = Vec::with_capacity(bytes.len());
    output.extend_from_slice(&bytes[..first_percent]);
    let mut index = first_percent;
    while index < bytes.len() {
        if index + 2 < bytes.len()
            && bytes[index] == b'%'
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    Cow::Owned(output)
}

#[inline]
const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{PercentEncodeSet, percent_decode, percent_encode};

    #[test]
    fn encodes_named_sets() {
        assert_eq!(
            percent_encode("a b/☃", PercentEncodeSet::Path),
            "a%20b/%E2%98%83"
        );
        assert_eq!(
            percent_encode("a b+c", PercentEncodeSet::FormUrlencoded),
            "a%20b%2Bc"
        );
        assert_eq!(percent_decode("caf%C3%A9"), "café");
    }
}
