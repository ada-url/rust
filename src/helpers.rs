//! Helper functions for URL parsing: path shortening, delimiter searching,
//! whitespace trimming, and path normalization.

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

#[cfg(not(feature = "std"))]
use alloc::{borrow::Cow, string::String};
#[cfg(feature = "std")]
use std::{borrow::Cow, string::String};

use crate::character_sets::PATH_PERCENT_ENCODE;
use crate::checkers::{
    is_normalized_windows_drive_letter, is_windows_drive_letter, path_signature,
};
use crate::scheme::SchemeType;
use crate::unicode::{
    is_c0_control_or_space, is_double_dot_path_segment, is_single_dot_path_segment,
    percent_encode_append,
};

// ---------------------------------------------------------------------------
// Tab / newline removal — CoW: zero allocation when nothing to strip
// ---------------------------------------------------------------------------

/// Strip ASCII tab (`\t`), line feed (`\n`) and carriage return (`\r`) from
/// `s`.  Returns `Cow::Borrowed(s)` unchanged when none are present — the
/// common case — so no allocation is performed.
///
/// Note: we compare full `char` values, **not** truncated `u8` casts, so
/// multi-byte code points whose low byte equals `\r`/`\n`/`\t` (e.g.
/// U+200D ZERO WIDTH JOINER, whose low byte is 0x0D) are never removed.
#[inline]
pub fn strip_tabs_newlines(s: &str) -> Cow<'_, str> {
    // Fast presence check — SIMD-accelerated when the `simd` feature is on.
    #[cfg(feature = "simd")]
    let has_special = crate::simd::has_tabs_or_newline(s.as_bytes());
    #[cfg(not(feature = "simd"))]
    let has_special = s.bytes().any(|b| matches!(b, b'\t' | b'\n' | b'\r'));
    if !has_special {
        return Cow::Borrowed(s); // zero allocation — common path
    }
    // Slow path: build an owned copy without those characters.
    // Retain operates on chars so multi-byte code points are handled safely.
    let mut owned = String::from(s);
    owned.retain(|c| c != '\t' && c != '\n' && c != '\r');
    Cow::Owned(owned)
}

/// In-place variant kept for internal use where an owned `String` is already
/// available and we want to strip in place without a CoW wrapper.
#[inline]
#[allow(dead_code)]
pub fn remove_ascii_tab_or_newline(s: &mut String) {
    if s.bytes().any(|b| matches!(b, b'\t' | b'\n' | b'\r')) {
        s.retain(|c| c != '\t' && c != '\n' && c != '\r');
    }
}

// ---------------------------------------------------------------------------
// C0 whitespace trimming — borrow the trimmed slice, no allocation
// ---------------------------------------------------------------------------

/// Trim leading and trailing C0 control characters and ASCII space.
/// Returns a `&str` slice into the original — **zero allocation**.
#[inline]
#[allow(dead_code)]
pub fn trim_c0_whitespace(s: &str) -> &str {
    let start = s
        .as_bytes()
        .iter()
        .position(|&b| !is_c0_control_or_space(b))
        .unwrap_or(s.len());
    let end = s
        .as_bytes()
        .iter()
        .rposition(|&b| !is_c0_control_or_space(b))
        .map(|i| i + 1)
        .unwrap_or(0);
    if start >= end { "" } else { &s[start..end] }
}

// ---------------------------------------------------------------------------
// Path shortening
// ---------------------------------------------------------------------------

/// Remove the last path segment from `path`. Returns `true` if anything changed.
/// For `file:` URLs with a single normalized Windows drive letter, does nothing.
pub fn shorten_path(path: &mut String, scheme_type: SchemeType) -> bool {
    if scheme_type == SchemeType::File
        && !path.is_empty()
        && path[1..].find('/').is_none()
        && is_normalized_windows_drive_letter(&path[1..])
    {
        return false;
    }
    if let Some(pos) = path.rfind('/') {
        path.truncate(pos);
        true
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Authority delimiter searching
// ---------------------------------------------------------------------------

/// Next authority delimiter for special URLs: `@`, `/`, `\`, `?`
#[inline]
pub fn find_authority_delimiter_special(view: &str) -> usize {
    #[cfg(feature = "simd")]
    return crate::simd::find_authority_delimiter_special(view);
    #[allow(unreachable_code)]
    view.bytes()
        .position(|b| matches!(b, b'@' | b'/' | b'\\' | b'?'))
        .unwrap_or(view.len())
}

/// Next authority delimiter for non-special URLs: `@`, `/`, `?`
#[inline]
pub fn find_authority_delimiter(view: &str) -> usize {
    #[cfg(feature = "simd")]
    return crate::simd::find_authority_delimiter(view);
    #[allow(unreachable_code)]
    view.bytes()
        .position(|b| matches!(b, b'@' | b'/' | b'?'))
        .unwrap_or(view.len())
}

// ---------------------------------------------------------------------------
// Host delimiter searching
// ---------------------------------------------------------------------------

#[inline]
fn find_next_host_delimiter_special(view: &str, from: usize) -> usize {
    #[cfg(feature = "simd")]
    return crate::simd::find_next_host_delimiter_special(view, from);
    #[allow(unreachable_code)]
    view.as_bytes()[from..]
        .iter()
        .position(|b| matches!(*b, b':' | b'/' | b'\\' | b'?' | b'['))
        .map(|p| from + p)
        .unwrap_or(view.len())
}

#[inline]
fn find_next_host_delimiter(view: &str, from: usize) -> usize {
    #[cfg(feature = "simd")]
    return crate::simd::find_next_host_delimiter(view, from);
    #[allow(unreachable_code)]
    view.as_bytes()[from..]
        .iter()
        .position(|b| matches!(*b, b':' | b'/' | b'?' | b'['))
        .map(|p| from + p)
        .unwrap_or(view.len())
}

/// Returns `(delimiter_position, found_colon_outside_brackets, trimmed_host_slice)`.
///
/// `trimmed_host_slice` borrows from `view` — no allocation.
pub fn get_host_delimiter_location(is_special: bool, view: &str) -> (usize, bool, &str) {
    let view_size = view.len();
    let mut location = 0;
    let mut found_colon = false;

    loop {
        let next = if is_special {
            find_next_host_delimiter_special(view, location)
        } else {
            find_next_host_delimiter(view, location)
        };

        if next >= view_size {
            location = view_size;
            break;
        }

        let b = view.as_bytes()[next];
        if b == b'[' {
            // skip to matching ']'
            match view[next..].find(']') {
                Some(end) => {
                    location = next + end + 1;
                    continue;
                }
                None => {
                    location = view_size;
                    break;
                }
            }
        } else {
            found_colon = b == b':';
            location = next;
            break;
        }
    }

    (location, found_colon, &view[..location])
}

// ---------------------------------------------------------------------------
// Opaque-path trailing-space stripping
// ---------------------------------------------------------------------------

pub fn strip_trailing_spaces_from_opaque_path(path: &mut String) {
    while path.ends_with(' ') {
        path.pop();
    }
}

// ---------------------------------------------------------------------------
// Path segment parsing
// ---------------------------------------------------------------------------

/// Parse a normalized path string `input` (without a leading `/`) and append
/// the resulting canonical segments to `path`.
///
/// Uses `Cow` internally so that path segments that need no percent-encoding
/// are referenced directly from `input` rather than copied.
pub fn parse_prepared_path(input: &str, scheme_type: SchemeType, path: &mut String) {
    const NEED_ENCODING: u8 = 1;
    const BACKSLASH: u8 = 2;
    const DOT: u8 = 4;
    const PERCENT: u8 = 8;

    let acc = path_signature(input);
    let special = scheme_type != SchemeType::NotSpecial;
    let may_need_slow = scheme_type == SchemeType::File && is_windows_drive_letter(input);

    let mut trivial = (if special {
        acc == 0
    } else {
        (acc & (NEED_ENCODING | DOT | PERCENT)) == 0
    }) && !may_need_slow;

    if acc == DOT && !may_need_slow && !input.is_empty() && input.as_bytes()[0] != b'.' {
        // Only dots present — check for /. or /.. sequences
        let mut slashdot = 0;
        let mut dot_is_file = true;
        loop {
            match input[slashdot..].find("/.") {
                None => break,
                Some(p) => {
                    slashdot += p + 2;
                    let rest = &input[slashdot..];
                    dot_is_file &=
                        !(rest.is_empty() || rest.starts_with('.') || rest.starts_with('/'));
                }
            }
        }
        trivial = dot_is_file;
    }

    if trivial {
        path.push('/');
        path.push_str(input);
        return;
    }

    let fast = special
        && (acc & (NEED_ENCODING | BACKSLASH | PERCENT)) == 0
        && scheme_type != SchemeType::File;

    if fast {
        parse_path_fast(input, scheme_type, path);
        return;
    }

    // Slow path: handle backslash, percent encoding, dots, Windows drives
    let needs_encoding = (acc & NEED_ENCODING) != 0;
    let mut remaining = input;
    let mut tmp = String::new(); // reused scratch buffer — only allocated once

    loop {
        let delim = if special && (acc & BACKSLASH) != 0 {
            remaining.bytes().position(|b| b == b'/' || b == b'\\')
        } else {
            remaining.bytes().position(|b| b == b'/')
        };

        let (segment, rest) = match delim {
            Some(pos) => (&remaining[..pos], &remaining[pos + 1..]),
            None => (remaining, ""),
        };
        let is_final = delim.is_none();
        remaining = rest;

        // Percent-encode the segment if needed — CoW: borrow when possible
        let encoded: Cow<'_, str> = if needs_encoding {
            tmp.clear();
            if percent_encode_append(segment, &PATH_PERCENT_ENCODE, &mut tmp) {
                Cow::Borrowed(tmp.as_str()) // tmp is owned but we borrow it
            } else {
                Cow::Borrowed(segment) // no encoding needed → borrow input
            }
        } else {
            Cow::Borrowed(segment)
        };
        let path_buffer: &str = &encoded;

        if is_double_dot_path_segment(path_buffer) {
            shorten_path(path, scheme_type);
            if is_final {
                path.push('/');
            }
        } else if is_single_dot_path_segment(path_buffer) {
            if is_final {
                path.push('/');
            }
        } else {
            // Windows drive letter normalization
            if scheme_type == SchemeType::File
                && path.is_empty()
                && is_windows_drive_letter(path_buffer)
            {
                path.push('/');
                let pb = path_buffer.as_bytes();
                path.push(pb[0] as char);
                path.push(':');
                path.push_str(&path_buffer[2..]);
            } else {
                path.push('/');
                path.push_str(path_buffer);
            }
        }

        if is_final {
            break;
        }
    }
}

/// Fast path: only dots, no encoding or backslash or percent chars needed.
fn parse_path_fast(input: &str, scheme_type: SchemeType, path: &mut String) {
    let mut remaining = input;
    loop {
        let (segment, rest, is_final) = match remaining.find('/') {
            None => (remaining, "", true),
            Some(pos) => (&remaining[..pos], &remaining[pos + 1..], false),
        };
        remaining = rest;

        if segment == ".." {
            shorten_path(path, scheme_type);
            if is_final {
                path.push('/');
            }
        } else if segment == "." {
            if is_final {
                path.push('/');
            }
        } else {
            path.push('/');
            path.push_str(segment); // borrow from `input`, no copy
        }

        if is_final {
            break;
        }
    }
}
