//! Unicode utilities: percent encoding/decoding, character classification,
//! and domain-name ASCII conversion.
//!
//! Key design principle: functions use `Cow<'_, str>` so that when no
//! transformation is needed the original string slice is borrowed
//! rather than copied.

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

#[cfg(not(feature = "std"))]
use alloc::{borrow::Cow, string::String, vec::Vec};
#[cfg(feature = "std")]
use std::{borrow::Cow, string::String, vec::Vec};

use crate::character_sets::{self, bit_at};

// ---------------------------------------------------------------------------
// Character classification
// ---------------------------------------------------------------------------

#[inline]
#[allow(dead_code)]
pub const fn is_tabs_or_newline(c: u8) -> bool {
    c == b'\r' || c == b'\n' || c == b'\t'
}

#[inline]
#[allow(dead_code)]
pub fn has_tabs_or_newline(s: &str) -> bool {
    #[allow(unreachable_code)]
    s.bytes().any(is_tabs_or_newline)
}

#[inline]
pub const fn is_ascii_digit(c: u8) -> bool {
    c >= b'0' && c <= b'9'
}

#[inline]
pub const fn is_ascii_hex_digit(c: u8) -> bool {
    (c >= b'0' && c <= b'9') || (c >= b'A' && c <= b'F') || (c >= b'a' && c <= b'f')
}

#[inline]
pub const fn is_alnum_plus(c: u8) -> bool {
    (c >= b'0' && c <= b'9')
        || (c >= b'a' && c <= b'z')
        || (c >= b'A' && c <= b'Z')
        || c == b'+'
        || c == b'-'
        || c == b'.'
}

#[inline]
#[allow(dead_code)]
pub const fn is_ascii_tab_or_newline(c: u8) -> bool {
    c == b'\t' || c == b'\n' || c == b'\r'
}

#[inline]
pub const fn is_c0_control_or_space(c: u8) -> bool {
    c <= b' '
}

// ---------------------------------------------------------------------------
// 256-byte lookup tables — O(1) per byte, branch-free
// ---------------------------------------------------------------------------

/// Forbidden host code point table (single bool per byte).
const IS_FORBIDDEN_HOST: [bool; 256] = {
    let mut t = [false; 256];
    let chars: &[u8] = &[
        0, 9, 10, 13, 32, 35, 47, 58, 60, 62, 63, 64, 91, 92, 93, 94, 124,
    ];
    let mut i = 0;
    while i < chars.len() {
        t[chars[i] as usize] = true;
        i += 1;
    }
    t
};

/// Combined domain-check table — eliminates two separate per-byte tests:
///   bit 0 → forbidden domain code point
///   bit 1 → ASCII uppercase letter
///
/// Matching what Ada C++ does: a single table lookup replaces a chain of
/// range/bitwise comparisons and an uppercase test.
pub(crate) const DOMAIN_CHECK: [u8; 256] = {
    let mut t = [0u8; 256];
    // forbidden: ≤ 0x20
    let mut c = 0usize;
    while c <= 32 {
        t[c] |= 1;
        c += 1;
    }
    // forbidden: ≥ 0x7F
    let mut c = 127usize;
    while c < 256 {
        t[c] |= 1;
        c += 1;
    }
    // forbidden: specific ASCII chars
    let extra: &[u8] = b"#/:<>?@[\\]^|%";
    let mut i = 0;
    while i < extra.len() {
        t[extra[i] as usize] |= 1;
        i += 1;
    }
    // uppercase A-Z
    let mut c = b'A';
    while c <= b'Z' {
        t[c as usize] |= 2;
        c += 1;
    }
    t
};

#[inline]
pub fn is_forbidden_host_code_point(c: u8) -> bool {
    IS_FORBIDDEN_HOST[c as usize]
}

#[inline]
pub fn is_forbidden_domain_code_point(c: u8) -> bool {
    DOMAIN_CHECK[c as usize] & 1 != 0
}

#[inline]
pub fn contains_forbidden_domain_code_point(s: &[u8]) -> bool {
    // Unrolled 4-at-a-time loop — same pattern Ada C++ uses
    let mut acc = 0u8;
    let mut i = 0;
    while i + 4 <= s.len() {
        acc |= DOMAIN_CHECK[s[i] as usize]
            | DOMAIN_CHECK[s[i + 1] as usize]
            | DOMAIN_CHECK[s[i + 2] as usize]
            | DOMAIN_CHECK[s[i + 3] as usize];
        i += 4;
    }
    while i < s.len() {
        acc |= DOMAIN_CHECK[s[i] as usize];
        i += 1;
    }
    acc & 1 != 0
}

/// Returns a flags byte with no branches:
///   bit 0 → at least one forbidden domain code point  
///   bit 1 → at least one ASCII uppercase letter
#[inline]
pub fn contains_forbidden_domain_code_point_or_upper(s: &[u8]) -> u8 {
    #[cfg(feature = "nightly-simd")]
    {
        return crate::portable_simd_impl::contains_forbidden_domain_code_point_or_upper(s);
    }
    #[cfg(not(feature = "nightly-simd"))]
    {
        let mut acc = 0u8;
        let mut i = 0;
        while i + 4 <= s.len() {
            acc |= DOMAIN_CHECK[s[i] as usize]
                | DOMAIN_CHECK[s[i + 1] as usize]
                | DOMAIN_CHECK[s[i + 2] as usize]
                | DOMAIN_CHECK[s[i + 3] as usize];
            i += 4;
        }
        while i < s.len() {
            acc |= DOMAIN_CHECK[s[i] as usize];
            i += 1;
        }
        acc
    }
}

// ---------------------------------------------------------------------------
// Case conversion
// ---------------------------------------------------------------------------

/// ASCII-lowercase `buf` in-place. Returns `true` iff the entire slice is ASCII.
///
/// With the `simd` feature uses Ada's SWAR `ascii_map` technique (8 bytes/iter).
#[inline]
pub fn to_lower_ascii(buf: &mut [u8]) -> bool {
    #[cfg(feature = "nightly-simd")]
    return crate::portable_simd_impl::to_lower_ascii(buf);

    // SWAR (SIMD Within A Register): Ada C++ `ascii_map` formula, 8 bytes/iter.
    // Always faster than byte-by-byte on all platforms; no feature flag needed.
    #[cfg(not(feature = "nightly-simd"))]
    {
        const M80: u64 = 0x8080_8080_8080_8080;
        const AP: u64 = 0x3f3f_3f3f_3f3f_3f3f; // broadcast(128-b'A')=63
        const ZP: u64 = 0x2525_2525_2525_2525; // broadcast(128-b'Z'-1)=37
        let n = buf.len();
        let ptr = buf.as_mut_ptr();
        let mut non_ascii: u64 = 0;
        let mut i = 0usize;
        while i + 8 <= n {
            let mut w: u64 = 0;
            unsafe { core::ptr::copy_nonoverlapping(ptr.add(i), &mut w as *mut u64 as *mut u8, 8) };
            non_ascii |= w & M80;
            w ^= (((w.wrapping_add(AP)) ^ (w.wrapping_add(ZP))) & M80) >> 2;
            unsafe { core::ptr::copy_nonoverlapping(&w as *const u64 as *const u8, ptr.add(i), 8) };
            i += 8;
        }
        if i < n {
            let rem = n - i;
            let mut w: u64 = 0;
            unsafe {
                core::ptr::copy_nonoverlapping(ptr.add(i), &mut w as *mut u64 as *mut u8, rem)
            };
            non_ascii |= w & M80;
            w ^= (((w.wrapping_add(AP)) ^ (w.wrapping_add(ZP))) & M80) >> 2;
            unsafe {
                core::ptr::copy_nonoverlapping(&w as *const u64 as *const u8, ptr.add(i), rem)
            };
        }
        non_ascii == 0
    }
}

// ---------------------------------------------------------------------------
// Hex conversion
// ---------------------------------------------------------------------------

#[inline]
pub const fn convert_hex_to_binary(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'A'..=b'F' => c - b'A' + 10,
        b'a'..=b'f' => c - b'a' + 10,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Percent decoding
// ---------------------------------------------------------------------------

/// Percent-decode `input`. The caller must supply `first_percent`, the index
/// of the first `%` byte. This avoids scanning for `%` twice.
#[allow(dead_code)]
pub fn percent_decode(input: &str, first_percent: usize) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    out.push_str(&input[..first_percent]);
    let mut i = first_percent;
    while i < bytes.len() {
        let ch = bytes[i];
        if ch == b'%'
            && i + 2 < bytes.len()
            && is_ascii_hex_digit(bytes[i + 1])
            && is_ascii_hex_digit(bytes[i + 2])
        {
            let val =
                convert_hex_to_binary(bytes[i + 1]) * 16 + convert_hex_to_binary(bytes[i + 2]);
            // SAFETY: each decoded byte is written as a single char;
            // the result may not be valid UTF-8 for multi-byte sequences,
            // but percent_decode is only used on byte-level data here.
            out.push(val as char);
            i += 3;
        } else {
            out.push(ch as char);
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Percent encoding — Copy-on-Write
// ---------------------------------------------------------------------------

/// Return the index of the first byte in `input` that must be percent-encoded.
/// Returns `input.len()` when no encoding is required.
#[inline]
pub fn percent_encode_index(input: &str, character_set: &[u8; 32]) -> usize {
    // When nightly-simd is available, delegate to specialised SIMD variants
    // for the three hottest character sets; fall through to generic for others.
    // The scalar bit-table lookup is already extremely fast (1 array index + 1 bit test
    // per byte).  The nightly-simd generic version adds function-call overhead without
    // measurably helping for the short strings common in URL parsing.
    // Specialised callers (update_base_search_with_encode) call the SIMD variants
    // from portable_simd_impl directly when nightly-simd is enabled.
    input
        .as_bytes()
        .iter()
        .position(|&b| bit_at(character_set, b))
        .unwrap_or(input.len())
}

/// Percent-encode `input` using `character_set`.
///
/// **Zero-copy fast path**: returns `Cow::Borrowed(input)` when no byte needs
/// encoding, so the caller pays no allocation cost in the common case.
#[inline]
pub fn percent_encode<'a>(input: &'a str, character_set: &[u8; 32]) -> Cow<'a, str> {
    let idx = percent_encode_index(input, character_set);
    if idx == input.len() {
        Cow::Borrowed(input)
    } else {
        Cow::Owned(percent_encode_from(input, character_set, idx))
    }
}

/// Build the percent-encoded string starting from `start_idx`.
/// Bytes `0..start_idx` are copied verbatim (caller guarantees they need no encoding).
pub fn percent_encode_from(input: &str, character_set: &[u8; 32], start_idx: usize) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len() + 32);
    out.push_str(&input[..start_idx]);
    for &b in &bytes[start_idx..] {
        if bit_at(character_set, b) {
            let off = (b as usize) * 4;
            // SAFETY: HEX is a compile-time &[u8; 1024] of valid ASCII "%XX\0" entries
            let s = unsafe { core::str::from_utf8_unchecked(&character_sets::HEX[off..off + 3]) };
            out.push_str(s);
        } else {
            out.push(b as char);
        }
    }
    out
}

/// Append percent-encoded `input` to `out`.
///
/// Returns `false` and does **not** touch `out` when no encoding is needed —
/// callers can skip any follow-up work in that case.
/// Returns `true` when at least one byte was percent-encoded and `out` was modified.
pub fn percent_encode_append(input: &str, character_set: &[u8; 32], out: &mut String) -> bool {
    let idx = percent_encode_index(input, character_set);
    if idx == input.len() {
        return false; // nothing to encode, zero allocation
    }
    out.push_str(&input[..idx]);
    let bytes = input.as_bytes();
    for &b in &bytes[idx..] {
        if bit_at(character_set, b) {
            let off = (b as usize) * 4;
            let s = unsafe { core::str::from_utf8_unchecked(&character_sets::HEX[off..off + 3]) };
            out.push_str(s);
        } else {
            out.push(b as char);
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Path segment helpers
// ---------------------------------------------------------------------------

#[inline]
pub fn is_double_dot_path_segment(s: &str) -> bool {
    matches!(
        s,
        ".." | ".%2e" | ".%2E" | "%2e." | "%2E." | "%2e%2e" | "%2E%2E" | "%2E%2e" | "%2e%2E"
    )
}

#[inline]
pub fn is_single_dot_path_segment(s: &str) -> bool {
    matches!(s, "." | "%2e" | "%2E")
}

// ---------------------------------------------------------------------------
// IDNA / ASCII domain conversion — Copy-on-Write
// ---------------------------------------------------------------------------

/// Convert `plain` to its ACE/Punycode ASCII form, percent-decoding first if
/// `first_percent` is `Some`.
///
/// **Zero-copy fast path**: returns `Cow::Borrowed(plain)` unchanged when the
/// domain is already pure-ASCII and passes all validity checks.
///
/// Returns `None` on failure (invalid IDNA label or forbidden code points in
/// the result).
pub fn to_ascii<'a>(plain: &'a str, first_percent: Option<usize>) -> Option<Cow<'a, str>> {
    let bytes = plain.as_bytes();
    let needs_decode = first_percent.is_some();
    let has_non_ascii = bytes.iter().any(|&b| b >= 0x80);
    let has_upper = !has_non_ascii && bytes.iter().any(|b| b.is_ascii_uppercase());
    let has_xn = contains_xn_prefix(plain);

    // Fast path: already canonical lowercase ASCII with no percent-encoded chars
    // → borrow directly, zero allocation.
    if !needs_decode
        && !has_non_ascii
        && !has_upper
        && !has_xn
        && !contains_forbidden_domain_code_point(bytes)
    {
        return Some(Cow::Borrowed(plain));
    }

    // Slow path: run full IDNA processing.
    // Percent-decode to raw bytes first, then validate as UTF-8 before passing
    // to the IDNA library (mirrors what the Ada C++ implementation does).
    let decoded_string;
    let input: &str = if let Some(pos) = first_percent {
        decoded_string = percent_decode_to_utf8(plain, pos)?;
        &decoded_string
    } else {
        plain
    };

    match crate::idna_impl::domain_to_ascii(input) {
        Some(result)
            if !result.is_empty() && !contains_forbidden_domain_code_point(result.as_bytes()) =>
        {
            Some(Cow::Owned(result))
        }
        _ => None,
    }
}

/// Percent-decode `input` (starting from `first_percent`) to raw bytes, then
/// validate those bytes as UTF-8.  Returns `None` when the decoded byte
/// sequence is not valid UTF-8.
fn percent_decode_to_utf8(input: &str, first_percent: usize) -> Option<String> {
    let src = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(input.len());
    out.extend_from_slice(&src[..first_percent]);
    let mut i = first_percent;
    while i < src.len() {
        let b = src[i];
        if b == b'%'
            && i + 2 < src.len()
            && is_ascii_hex_digit(src[i + 1])
            && is_ascii_hex_digit(src[i + 2])
        {
            let val = convert_hex_to_binary(src[i + 1]) * 16 + convert_hex_to_binary(src[i + 2]);
            out.push(val);
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[inline]
pub fn contains_xn_prefix_pub(s: &str) -> bool {
    contains_xn_prefix(s)
}

#[inline]
fn contains_xn_prefix(s: &str) -> bool {
    // Check if any dot-separated label starts with "xn-" (case-insensitive)
    let s = s.trim_end_matches('.');
    s.split('.').any(|label| {
        label.len() >= 3
            && (label.as_bytes()[0] | 0x20) == b'x'
            && (label.as_bytes()[1] | 0x20) == b'n'
            && label.as_bytes()[2] == b'-'
    })
}
