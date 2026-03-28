/// Zero-allocation URL validator used by `Url::can_parse`.
///
/// For the common case of an absolute ASCII URL with no base this avoids
/// building any `String` buffer at all, making `can_parse` roughly 2× faster
/// than `parse`.
#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::checkers::{is_alpha, try_parse_ipv4_fast};
use crate::scheme::{get_scheme_type, SchemeType};
use crate::unicode::{
    contains_forbidden_domain_code_point, contains_forbidden_domain_code_point_or_upper,
    contains_xn_prefix_pub, is_alnum_plus, is_ascii_digit, is_c0_control_or_space,
};

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Fast, zero-allocation validation for an absolute URL (no base).
///
/// Falls back to the full parser only when the host requires IDNA processing
/// (non-ASCII or xn-- labels) — that case is uncommon for real-world domains.
pub fn can_parse_no_base(user_input: &str) -> bool {
    // Trim C0 whitespace — borrow, no alloc
    let b = user_input.as_bytes();
    let start = b.iter().position(|&c| !is_c0_control_or_space(c)).unwrap_or(b.len());
    let end   = b.iter().rposition(|&c| !is_c0_control_or_space(c)).map(|i| i + 1).unwrap_or(0);
    if start >= end { return false; }
    let b = &b[start..end];

    // Skip tabs/newlines while scanning (don't allocate; just flag them)
    let has_specials = b.iter().any(|&c| c == b'\t' || c == b'\n' || c == b'\r');

    // Fast path: no tabs/newlines — validate in-place with zero allocations
    if !has_specials {
        return validate_absolute(b);
    }

    // Rare: need a copy to strip tabs/newlines, then validate
    let cleaned: Vec<u8> = b.iter().copied().filter(|&c| c != b'\t' && c != b'\n' && c != b'\r').collect();
    validate_absolute(&cleaned)
}

// ---------------------------------------------------------------------------
// Main validator
// ---------------------------------------------------------------------------

fn validate_absolute(b: &[u8]) -> bool {
    if b.is_empty() { return false; }

    // Find scheme: must start with alpha, then alnum/+/-/., terminated by ':'
    if !is_alpha(b[0]) { return false; }
    let scheme_end = match b.iter().position(|&c| !is_alnum_plus(c)) {
        Some(p) if b[p] == b':' => p,
        _ => return false,
    };
    let scheme_bytes = &b[..scheme_end];
    // Lowercase up to 5 bytes into a stack buffer for special-scheme classification.
    // All special schemes (http, https, ftp, ws, wss, file) are ≤5 chars; anything
    // longer is definitely non-special, so we skip the copy entirely for those.
    let scheme_type = if scheme_end <= 5 {
        let mut scheme_lower = [0u8; 5];
        for (i, &c) in scheme_bytes.iter().enumerate() {
            scheme_lower[i] = c | 0x20; // ASCII lowercase
        }
        // SAFETY: we only have ASCII alnum/+/-/. bytes
        let scheme_str = core::str::from_utf8(&scheme_lower[..scheme_end]).unwrap_or("");
        get_scheme_type(scheme_str)
    } else {
        SchemeType::NotSpecial
    };

    let rest = &b[scheme_end + 1..]; // everything after ':'

    // Strip fragment from consideration (always valid after '#')
    let rest = match rest.iter().position(|&c| c == b'#') {
        Some(p) => &rest[..p],
        None => rest,
    };

    match scheme_type {
        SchemeType::File => validate_file_url(rest),
        s if s.is_special() => validate_special_authority(rest, scheme_type),
        _ => validate_non_special(rest),
    }
}

// ---------------------------------------------------------------------------
// Special-scheme authority validation
// ---------------------------------------------------------------------------

fn validate_special_authority(rest: &[u8], _scheme_type: SchemeType) -> bool {
    // Consume leading slashes — special URLs require "//"
    let rest = consume_slashes(rest);
    // Rest is: [user:pass@]host[:port][/path][?query]
    validate_authority_and_rest(rest, true)
}

fn validate_non_special(rest: &[u8]) -> bool {
    if rest.starts_with(b"//") {
        validate_authority_and_rest(&rest[2..], false)
    } else {
        true // opaque path — always valid structurally
    }
}

/// Validate a `file:` URL.
///
/// The host is optional for file: (e.g., `file:///path` has an empty host).
/// When an authority is present validate its host/port with the same rules
/// used for non-special URLs (empty host is allowed).
fn validate_file_url(rest: &[u8]) -> bool {
    if rest.starts_with(b"//") || rest.starts_with(b"\\\\") {
        let rest = consume_slashes(rest);
        // Empty host is allowed for file: (e.g., file:///path)
        validate_authority_and_rest(rest, false)
    } else {
        true // path-only file URL, no authority to validate
    }
}

fn consume_slashes(b: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < b.len() && (b[i] == b'/' || b[i] == b'\\') { i += 1; }
    &b[i..]
}

// ---------------------------------------------------------------------------
// Authority + path/query validation
// ---------------------------------------------------------------------------

fn validate_authority_and_rest(rest: &[u8], is_special: bool) -> bool {
    // Find end of authority: first '/', '\\' (special), '?' or end
    let auth_end = rest.iter().position(|&c| c == b'/' || (is_special && c == b'\\') || c == b'?')
        .unwrap_or(rest.len());
    let authority = &rest[..auth_end];

    // Strip credentials if '@' is present
    let host_port = if let Some(at) = authority.iter().rposition(|&c| c == b'@') {
        &authority[at + 1..]
    } else {
        authority
    };

    validate_host_and_port(host_port, is_special)
    // path and query always pass structural validation
}

// ---------------------------------------------------------------------------
// Host + port validation
// ---------------------------------------------------------------------------

fn validate_host_and_port(host_port: &[u8], is_special: bool) -> bool {
    if host_port.is_empty() {
        return !is_special; // special URLs must have a host
    }

    // IPv6: [...]
    if host_port.starts_with(b"[") {
        let close = match host_port.iter().position(|&c| c == b']') {
            Some(p) => p,
            None => return false,
        };
        let ipv6 = &host_port[1..close];
        if !validate_ipv6_fast(ipv6) {
            return false;
        }
        // After ']': either end of input, or ':' followed by an optional port.
        let after = &host_port[close + 1..];
        if after.is_empty() {
            return true;
        }
        if after[0] != b':' {
            return false; // trailing garbage, e.g. "[::1]garbage"
        }
        return validate_port(&after[1..]);
    }

    // Split host from port at ':'
    // But be careful: host might be just empty for non-special
    let (host_bytes, port_bytes) = split_host_port(host_port);

    if !port_bytes.is_empty() && !validate_port(port_bytes) {
        return false;
    }

    validate_host(host_bytes, is_special)
}

fn split_host_port(b: &[u8]) -> (&[u8], &[u8]) {
    match b.iter().rposition(|&c| c == b':') {
        None => (b, b""),
        Some(colon) => (&b[..colon], &b[colon + 1..]),
    }
}

fn validate_port(port: &[u8]) -> bool {
    if port.is_empty() { return true; }
    if !port.iter().all(|&c| is_ascii_digit(c)) { return false; }
    // Range check 0-65535
    if port.len() > 5 { return false; }
    let mut n: u32 = 0;
    for &c in port { n = n * 10 + (c - b'0') as u32; }
    n <= 65535
}

// ---------------------------------------------------------------------------
// Host validation — zero-alloc ASCII fast path
// ---------------------------------------------------------------------------

fn validate_host(host: &[u8], is_special: bool) -> bool {
    if host.is_empty() { return !is_special; }

    if !is_special {
        // Opaque host: just check no forbidden host code points
        return !host.iter().any(|&c| crate::unicode::is_forbidden_host_code_point(c));
    }

    // Fast IPv4 check (pure decimal, e.g. "192.168.1.1")
    if let Ok(s) = core::str::from_utf8(host) {
        let fast = try_parse_ipv4_fast(s);
        if fast != u64::MAX { return true; }
    }

    let status = contains_forbidden_domain_code_point_or_upper(host);

    // Fast path: pure lowercase ASCII, no forbidden chars, no xn-- labels
    if status == 0
        && let Ok(s) = core::str::from_utf8(host)
            && !contains_xn_prefix_pub(s) {
                // Quick domain character check
                let ok = host.iter().all(|&c| {
                    c.is_ascii_alphanumeric() || c == b'-' || c == b'.'
                });
                if ok { return true; }
                // Even with odd chars, if no forbidden domain code points → valid
                return !contains_forbidden_domain_code_point(host);
            }

    // Needs IDNA (non-ASCII, uppercase, or xn-- labels) → fall back to full parse.
    // This path is rare for real-world URLs and the allocation cost is acceptable.
    if let Ok(s) = core::str::from_utf8(host) {
        crate::unicode::to_ascii(s, s.find('%')).is_some()
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// IPv6 structural validator (non-allocating, full structural check)
// ---------------------------------------------------------------------------

/// Validates the content between `[` and `]` of an IPv6 host.
///
/// Enforces the RFC 3986 / WHATWG rules:
/// - Exactly 8 hextets of 1-4 hex digits separated by `:`, OR
/// - Fewer hextets with exactly one `::` compression, OR
/// - An embedded IPv4 address in the last two hextet positions.
/// - An optional zone ID after `%` (unreserved ASCII chars only).
fn validate_ipv6_fast(inner: &[u8]) -> bool {
    if inner.is_empty() { return false; }

    // Split off optional zone ID (e.g. "fe80::1%eth0").
    let addr = match inner.iter().position(|&c| c == b'%') {
        Some(idx) => {
            if idx == 0 { return false; } // no address before '%'
            let zone = &inner[idx + 1..];
            if zone.is_empty() { return false; }
            if !zone.iter().all(|&c| {
                c.is_ascii_alphanumeric() || c == b'.' || c == b'-' || c == b'_'
            }) {
                return false;
            }
            &inner[..idx]
        }
        None => inner,
    };

    if addr.is_empty() { return false; }

    let n = addr.len();
    let mut i = 0usize;
    let mut hextets: u8 = 0;
    let mut has_double_colon = false;

    // Handle leading "::"
    if n >= 2 && addr[0] == b':' && addr[1] == b':' {
        has_double_colon = true;
        i = 2;
        if i == n { return true; } // bare "::"
    } else if addr[0] == b':' {
        return false; // leading single ':'
    }

    while i < n {
        let seg_start = i;
        let mut has_dot = false;

        // Consume one segment (hex digits and/or dots for embedded IPv4)
        while i < n && addr[i] != b':' {
            let c = addr[i];
            if c == b'.' {
                has_dot = true;
            } else if !c.is_ascii_hexdigit() {
                return false;
            }
            i += 1;
        }

        let seg = &addr[seg_start..i];
        if seg.is_empty() { return false; }

        if has_dot {
            // Embedded IPv4 — must appear at the very end and counts as 2 hextets.
            if !validate_embedded_ipv4(seg) { return false; }
            hextets = hextets.saturating_add(2);
            if hextets > 8 { return false; }
            if i < n { return false; } // nothing may follow the IPv4 part
            break;
        } else {
            if seg.len() > 4 { return false; } // hextet must be 1-4 hex digits
            hextets = hextets.saturating_add(1);
            if hextets > 8 { return false; }
        }

        if i < n {
            // Current byte is ':', decide between ':' and '::'
            if i + 1 < n && addr[i + 1] == b':' {
                if has_double_colon { return false; } // only one '::' allowed
                has_double_colon = true;
                i += 2;
                if i == n { break; } // trailing "::" is valid
            } else {
                i += 1;
                if i == n { return false; } // trailing single ':' is invalid
            }
        }
    }

    if has_double_colon {
        hextets < 8 // '::' must represent at least one omitted zero hextet
    } else {
        hextets == 8 // without compression, exactly 8 hextets are required
    }
}

/// Validate an embedded IPv4 address inside an IPv6 literal (e.g. `::ffff:192.0.2.1`).
fn validate_embedded_ipv4(b: &[u8]) -> bool {
    match core::str::from_utf8(b) {
        Ok(s) => try_parse_ipv4_fast(s) != u64::MAX,
        Err(_) => false,
    }
}
