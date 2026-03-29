//! URL validation checkers.

use crate::unicode::is_ascii_digit;

#[inline]
pub const fn is_alpha(c: u8) -> bool {
    (c >= b'a' && c <= b'z') || (c >= b'A' && c <= b'Z')
}

#[inline]
#[allow(dead_code)]
pub const fn is_digit(c: u8) -> bool {
    c >= b'0' && c <= b'9'
}

#[inline]
#[allow(dead_code)]
pub const fn has_hex_prefix(s: &[u8]) -> bool {
    s.len() >= 2 && s[0] == b'0' && (s[1] == b'x' || s[1] == b'X')
}

/// Returns true if `input` starts with a Windows drive letter (e.g. "C:", "C|").
/// Per the WHATWG spec, a string starts with a Windows drive letter if:
/// - length >= 2
/// - first char is an ASCII alpha
/// - second char is ':' or '|'
/// - length == 2 OR third char is '/', '\', '?', '#'
#[inline]
pub fn is_windows_drive_letter(input: &str) -> bool {
    let b = input.as_bytes();
    b.len() >= 2
        && is_alpha(b[0])
        && (b[1] == b':' || b[1] == b'|')
        && (b.len() == 2 || matches!(b[2], b'/' | b'\\' | b'?' | b'#'))
}

/// A normalized Windows drive letter has ':' as the second character (not '|').
#[inline]
pub fn is_normalized_windows_drive_letter(input: &str) -> bool {
    let b = input.as_bytes();
    b.len() >= 2 && is_alpha(b[0]) && b[1] == b':'
}

/// Returns true if the input looks like an IPv4 address.
/// Mirrors the algorithm in the bundled Ada C++ `is_ipv4`:
///
/// 1. Strip exactly one trailing dot if present.
/// 2. Quick filter: last character must be 0-9, a-f, or 'x'.
/// 3. Find the last dot. Examine the last label.
/// 4. All-decimal labels are IPv4.
/// 5. Labels that are exactly "0x" or "0x" followed by all lowercase hex digits are IPv4.
///
/// Everything else is NOT IPv4 (treated as domain).
pub fn is_ipv4(input: &str) -> bool {
    // Strip at most one trailing dot
    let s = if let Some(t) = input.strip_suffix('.') {
        if t.is_empty() {
            return false;
        }
        t
    } else {
        input
    };

    let b = s.as_bytes();
    let last_char = b[b.len() - 1];

    // Quick filter: the last character must be a decimal digit, a-f, or 'x'
    let possible =
        last_char.is_ascii_digit() || matches!(last_char, b'a'..=b'f') || last_char == b'x';
    if !possible {
        return false;
    }

    // Extract the last label (after the last dot, or the whole string)
    let last_label = match s.rfind('.') {
        Some(pos) => &s[pos + 1..],
        None => s,
    };
    let lb = last_label.as_bytes();
    if lb.is_empty() {
        return false;
    }

    // All decimal digits → IPv4
    if lb.iter().all(u8::is_ascii_digit) {
        return true;
    }

    // Single char, not all-decimal → not IPv4
    if lb.len() == 1 {
        return false;
    }

    // Must start with "0x"
    if lb[0] != b'0' || lb[1] != b'x' {
        return false;
    }

    // Just "0x" (zero hex) → IPv4
    if lb.len() == 2 {
        return true;
    }

    // "0x" + all lowercase hex digits → IPv4
    lb[2..]
        .iter()
        .all(|&c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

/// Pre-computed path-signature lookup table — replaces the per-byte `match`.
///
/// Bit encoding per byte value:
///   bit 0 (0x01) – needs percent-encoding per PATH_PERCENT_ENCODE  
///   bit 1 (0x02) – backslash `\`
///   bit 2 (0x04) – dot `.`
///   bit 3 (0x08) – percent `%`
pub(crate) const PATH_SIG_TABLE: [u8; 256] = {
    let mut t = [0u8; 256];
    // Needs encoding: C0 controls (0x00-0x1F), DEL (0x7F), high bytes (0x80-0xFF)
    let mut i = 0usize;
    while i <= 0x1F {
        t[i] |= 0x01;
        i += 1;
    }
    let mut i = 0x7Fusize;
    while i < 256 {
        t[i] |= 0x01;
        i += 1;
    }
    // Needs encoding: specific printable ASCII chars
    let enc: &[u8] = b" \"#<>?^`{|}";

    let mut i = 0;
    while i < enc.len() {
        t[enc[i] as usize] |= 0x01;
        i += 1;
    }
    // Special flags
    t[b'\\' as usize] |= 0x02; // backslash
    t[b'.' as usize] |= 0x04; // dot
    t[b'%' as usize] |= 0x08; // percent
    t
};

/// Compute a path-signature byte via Ada's exact 8-at-a-time unrolled lookup.
///
/// Ada C++ uses `for (; i + 7 < size; i += 8)` — we match that exactly.
pub fn path_signature(input: &str) -> u8 {
    #[cfg(feature = "nightly-simd")]
    {
        return crate::portable_simd_impl::path_signature(input);
    }
    #[cfg(not(feature = "nightly-simd"))]
    {
        let b = input.as_bytes();
        let mut acc = 0u8;
        let mut i = 0;
        // 8-at-a-time — Ada C++ uses this exact unroll factor
        while i + 8 <= b.len() {
            acc |= PATH_SIG_TABLE[b[i] as usize]
                | PATH_SIG_TABLE[b[i + 1] as usize]
                | PATH_SIG_TABLE[b[i + 2] as usize]
                | PATH_SIG_TABLE[b[i + 3] as usize]
                | PATH_SIG_TABLE[b[i + 4] as usize]
                | PATH_SIG_TABLE[b[i + 5] as usize]
                | PATH_SIG_TABLE[b[i + 6] as usize]
                | PATH_SIG_TABLE[b[i + 7] as usize];
            i += 8;
        }
        while i < b.len() {
            acc |= PATH_SIG_TABLE[b[i] as usize];
            i += 1;
        }
        acc
    }
}

/// Full WHATWG IPv4 address parser — handles decimal, octal (leading 0),
/// hex (0x/0X), and 1–4 dot-separated parts (e.g. "127.1", "0xc0a80101").
///
/// Returns `Some(packed_u32)` on success, `None` for invalid input.
/// This is used by `try_parse_absolute_fast` to normalise IPv4 addresses in-place.
pub fn parse_ipv4_address(input: &str) -> Option<u32> {
    let input = input.trim_end_matches('.');
    if input.is_empty() {
        return None;
    }
    let mut parts = 0usize; // dot-separated parts consumed so far
    let mut ipv4: u64 = 0;
    let mut rem = input;

    loop {
        if parts >= 4 || rem.is_empty() {
            break;
        }
        let b = rem.as_bytes();

        // Determine radix and parse one part
        let (val, consumed) = if b.len() >= 2 && b[0] == b'0' && (b[1] == b'x' || b[1] == b'X') {
            // Hexadecimal
            if b.len() == 2 || b[2] == b'.' {
                (0u64, 2usize)
            } else {
                let (v, c) = parse_uint_raw(&rem[2..], 16)?;
                (v, 2 + c)
            }
        } else if b[0] == b'0' && b.len() > 1 && b[1] >= b'0' && b[1] <= b'9' {
            // Octal
            let (v, c) = parse_uint_raw(&rem[1..], 8)?;
            (v, 1 + c)
        } else {
            // Decimal
            parse_uint_raw(rem, 10)?
        };

        rem = &rem[consumed..];

        if rem.is_empty() {
            // Final (possibly multi-octet) part
            let bits = 32u32.wrapping_sub(parts as u32 * 8);
            let max = if bits >= 64 { u64::MAX } else { 1u64 << bits };
            if val >= max {
                return None; // overflow
            }
            ipv4 = (ipv4 << bits) | val;
            parts += 1;
            break;
        } else {
            // Intermediate part — must be a single octet ≤ 255 followed by '.'
            if val > 255 || rem.as_bytes()[0] != b'.' {
                return None;
            }
            ipv4 = (ipv4 << 8) | val;
            rem = &rem[1..]; // skip '.'
            parts += 1;
        }
    }

    if !rem.is_empty() || parts == 0 {
        return None;
    }
    Some(ipv4 as u32)
}

/// Parse an unsigned integer of the given radix from the start of `s`.
/// Returns `(value, bytes_consumed)` or `None` if no digits were found.
#[inline]
fn parse_uint_raw(s: &str, radix: u64) -> Option<(u64, usize)> {
    let b = s.as_bytes();
    if b.is_empty() {
        return None;
    }
    let mut v = 0u64;
    let mut c = 0usize;
    for &byte in b {
        let d = match radix {
            16 => match byte {
                b'0'..=b'9' => (byte - b'0') as u64,
                b'a'..=b'f' => (byte - b'a' + 10) as u64,
                b'A'..=b'F' => (byte - b'A' + 10) as u64,
                _ => break,
            },
            8 => match byte {
                b'0'..=b'7' => (byte - b'0') as u64,
                _ => break,
            },
            _ => match byte {
                b'0'..=b'9' => (byte - b'0') as u64,
                _ => break,
            },
        };
        v = v.checked_mul(radix)?.checked_add(d)?;
        c += 1;
    }
    if c == 0 { None } else { Some((v, c)) }
}

/// Check that the domain name length and label lengths are within DNS limits.
pub fn verify_dns_length(input: &str) -> bool {
    let s = input.strip_suffix('.').unwrap_or(input);
    if s.is_empty() || s.len() > 253 {
        return false;
    }
    for label in s.split('.') {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
    }
    true
}

/// Fast-path parser for pure decimal IPv4 addresses (e.g. "192.168.1.1").
/// Returns the packed 32-bit IPv4 as u64 on success, or u64::MAX on failure.
pub fn try_parse_ipv4_fast(input: &str) -> u64 {
    const FAIL: u64 = u64::MAX;
    let b = input.as_bytes();
    let mut pos = 0;
    let mut ipv4: u32 = 0;

    for i in 0..4usize {
        if pos >= b.len() {
            return FAIL;
        }
        let c = b[pos];
        if !is_ascii_digit(c) {
            return FAIL;
        }
        let mut val = (c - b'0') as u32;
        pos += 1;

        if pos < b.len() && is_ascii_digit(b[pos]) {
            if val == 0 {
                return FAIL; // no leading zeros
            }
            val = val * 10 + (b[pos] - b'0') as u32;
            pos += 1;
            if pos < b.len() && is_ascii_digit(b[pos]) {
                val = val * 10 + (b[pos] - b'0') as u32;
                pos += 1;
                if val > 255 {
                    return FAIL;
                }
            }
        }

        ipv4 = (ipv4 << 8) | val;

        if i < 3 {
            if pos >= b.len() || b[pos] != b'.' {
                return FAIL;
            }
            pos += 1;
        }
    }

    if pos == b.len() {
        return ipv4 as u64;
    }
    // Allow trailing dot
    if pos == b.len() - 1 && b[pos] == b'.' {
        return ipv4 as u64;
    }
    FAIL
}
