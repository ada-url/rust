//! Zero-allocation URL validator for [`Url::can_parse`].
//!
//! **Guaranteed zero heap allocations** for any input, including:
//! - URLs with tabs/newlines (processed via inline filtering iterator)
//! - URLs with IDNA hosts (validated using stack-allocated `[u32; 64]` label
//!   buffers — DNS labels are capped at 63 chars, so 64 u32s is always enough)
//!
//! Falls back to `false` only when a URL is structurally invalid or contains an
//! actually-invalid IDNA label; valid IDN domains are accepted correctly.

#[cfg(not(feature = "std"))]
extern crate alloc;

use crate::checkers::{is_alpha, try_parse_ipv4_fast};
use crate::scheme::{SchemeType, get_scheme_type};
use crate::unicode::{
    contains_forbidden_domain_code_point_or_upper, contains_xn_prefix_pub, is_alnum_plus,
    is_ascii_digit, is_c0_control_or_space,
};

// ============================================================
// Entry point
// ============================================================

/// Validate an absolute URL with no base URL — **zero heap allocations**.
pub fn can_parse_no_base(user_input: &str) -> bool {
    let b = user_input.as_bytes();
    // C0 whitespace trim — borrows a subslice, no allocation
    let start = b
        .iter()
        .position(|&c| !is_c0_control_or_space(c))
        .unwrap_or(b.len());
    let end = b
        .iter()
        .rposition(|&c| !is_c0_control_or_space(c))
        .map(|i| i + 1)
        .unwrap_or(0);
    if start >= end {
        return false;
    }
    validate_absolute_raw(&b[start..end])
}

// ============================================================
// Inline-filtering byte source
// ============================================================

/// An iterator over `b` that transparently skips `\t`, `\n`, `\r`.
/// No allocation — works entirely on the original slice.
struct SkipSpecial<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> SkipSpecial<'a> {
    #[inline]
    fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }

    /// Advance past the current position (already consumed one byte).
    #[inline]
    fn advance_past(&mut self, byte_pos_in_b: usize) {
        self.pos = byte_pos_in_b + 1;
    }

    /// Position of the next non-special byte, or `b.len()` if exhausted.
    #[inline]
    fn peek_pos(&self) -> usize {
        let mut p = self.pos;
        while p < self.b.len() && matches!(self.b[p], b'\t' | b'\n' | b'\r') {
            p += 1;
        }
        p
    }

    #[inline]
    fn peek(&self) -> Option<u8> {
        let p = self.peek_pos();
        if p < self.b.len() {
            Some(self.b[p])
        } else {
            None
        }
    }
}

// ============================================================
// Main validator (works on raw bytes, handles tabs/newlines inline)
// ============================================================

fn validate_absolute_raw(b: &[u8]) -> bool {
    if b.is_empty() {
        return false;
    }

    let mut src = SkipSpecial::new(b);

    // Scheme: must start with alpha, then alnum/+/-/., terminated by ':'
    let first = src.peek().unwrap_or(0);
    if !is_alpha(first) {
        return false;
    }

    #[allow(unused_assignments)]
    let mut scheme_end_in_b = 0usize;
    loop {
        let p = src.peek_pos();
        if p >= b.len() {
            return false;
        }
        let c = b[p];
        if c == b':' {
            scheme_end_in_b = p;
            src.advance_past(p);
            break;
        }
        if !is_alnum_plus(c) {
            return false;
        }
        src.advance_past(p);
    }

    // Lowercase scheme into a small stack buffer for special-scheme classification.
    // All special schemes (http, https, ftp, ws, wss, file) are ≤5 significant
    // chars. Schemes longer than that are definitively non-special — skip the copy.
    let raw_scheme = &b[..scheme_end_in_b];
    let scheme_type = {
        let significant_len = raw_scheme
            .iter()
            .filter(|&&c| !matches!(c, b'\t' | b'\n' | b'\r'))
            .count();
        if significant_len <= 5 {
            let mut scheme_lower = [0u8; 5];
            let mut scheme_len = 0usize;
            for &c in raw_scheme
                .iter()
                .filter(|&&c| !matches!(c, b'\t' | b'\n' | b'\r'))
            {
                scheme_lower[scheme_len] = c | 0x20;
                scheme_len += 1;
            }
            let scheme_str = unsafe { core::str::from_utf8_unchecked(&scheme_lower[..scheme_len]) };
            get_scheme_type(scheme_str)
        } else {
            SchemeType::NotSpecial
        }
    };

    // Rest = everything after ':'
    let rest_start = src.peek_pos();

    // Find '#' in remaining bytes (fragment is always valid, stop there)
    let rest_end = b[rest_start..]
        .iter()
        .position(|&c| c == b'#')
        .map(|p| rest_start + p)
        .unwrap_or(b.len());

    let rest = &b[rest_start..rest_end];

    match scheme_type {
        SchemeType::File => validate_file_url_raw(rest),
        s if s.is_special() => validate_special_authority_raw(rest),
        _ => validate_non_special_raw(rest),
    }
}

// ============================================================
// Authority validation
// ============================================================

fn validate_special_authority_raw(rest: &[u8]) -> bool {
    let rest = consume_slashes_raw(rest);
    validate_authority_and_rest_raw(rest, true)
}

fn validate_non_special_raw(rest: &[u8]) -> bool {
    // Skip tabs/newlines when checking for "//"
    let mut i = 0;
    while i < rest.len() && matches!(rest[i], b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    if i + 1 < rest.len() && rest[i] == b'/' && rest[i + 1] == b'/' {
        validate_authority_and_rest_raw(&rest[i + 2..], false)
    } else {
        true // opaque path
    }
}

/// Validate a `file:` URL.
///
/// The host is optional for `file:` (e.g. `file:///path` has an empty host).
/// When an authority is present, validate its host/port with the same rules
/// used for non-special URLs (empty host is allowed).
fn validate_file_url_raw(rest: &[u8]) -> bool {
    // Check for "//" or "\\" authority indicator (skip leading tabs/newlines)
    let mut i = 0;
    while i < rest.len() && matches!(rest[i], b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    let has_authority = (i + 1 < rest.len() && rest[i] == b'/' && rest[i + 1] == b'/')
        || (i + 1 < rest.len() && rest[i] == b'\\' && rest[i + 1] == b'\\');
    if has_authority {
        let rest = consume_slashes_raw(rest);
        // Empty host is allowed for file: (e.g. file:///path)
        validate_authority_and_rest_raw(rest, false)
    } else {
        true // path-only file URL, no authority to validate
    }
}

#[inline]
fn consume_slashes_raw(b: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' | b'\\' => i += 1,
            b'\t' | b'\n' | b'\r' => i += 1,
            _ => break,
        }
    }
    &b[i..]
}

fn validate_authority_and_rest_raw(rest: &[u8], is_special: bool) -> bool {
    let auth_end = rest
        .iter()
        .position(|&c| c == b'/' || (is_special && c == b'\\') || c == b'?')
        .unwrap_or(rest.len());

    let authority = &rest[..auth_end];

    // Strip credentials: find last '@' (skipping tabs/newlines)
    let host_port = if let Some(at) = authority.iter().rposition(|&c| c == b'@') {
        &authority[at + 1..]
    } else {
        authority
    };

    validate_host_and_port_raw(host_port, is_special)
}

// ============================================================
// Host + port validation
// ============================================================

fn validate_host_and_port_raw(host_port: &[u8], is_special: bool) -> bool {
    // Filter out tabs/newlines first (stack — no heap).
    // Since host_port is typically very short (≤ 253 bytes), a 256-byte stack
    // buffer covers virtually all realistic inputs.
    let mut buf = [0u8; 256];
    let mut len = 0usize;
    for &c in host_port {
        if matches!(c, b'\t' | b'\n' | b'\r') {
            continue;
        }
        if len >= 256 {
            return false; // pathologically long host — invalid
        }
        buf[len] = c;
        len += 1;
    }
    let hp = &buf[..len];

    if hp.is_empty() {
        return !is_special;
    }

    // IPv6: [...]
    if hp.starts_with(b"[") {
        let close = hp.iter().position(|&c| c == b']').unwrap_or(0);
        if close == 0 {
            return false;
        }
        if !validate_ipv6_fast(&hp[1..close]) {
            return false;
        }
        // After ']': either end of input, or ':' followed by an optional port.
        let after = &hp[close + 1..];
        if after.is_empty() {
            return true;
        }
        if after[0] != b':' {
            return false; // trailing garbage, e.g. "[::1]garbage"
        }
        return validate_port(&after[1..]);
    }

    // Split host from port
    let (host_bytes, port_bytes) = match hp.iter().rposition(|&c| c == b':') {
        None => (hp, &b""[..]),
        Some(p) => (&hp[..p], &hp[p + 1..]),
    };

    if !port_bytes.is_empty() && !validate_port(port_bytes) {
        return false;
    }
    validate_host_zero_alloc(host_bytes, is_special)
}

#[inline]
fn validate_port(port: &[u8]) -> bool {
    if port.is_empty() {
        return true;
    }
    if port.len() > 5 {
        return false;
    }
    if !port.iter().all(|&c| is_ascii_digit(c)) {
        return false;
    }
    let n: u32 = port.iter().fold(0u32, |a, &c| a * 10 + (c - b'0') as u32);
    n <= 65535
}

// ============================================================
// Host validation — zero-allocation including IDNA
// ============================================================

fn validate_host_zero_alloc(host: &[u8], is_special: bool) -> bool {
    if host.is_empty() {
        return !is_special;
    }

    if !is_special {
        return !host
            .iter()
            .any(|&c| crate::unicode::is_forbidden_host_code_point(c));
    }

    // Pure-decimal IPv4 fast path (e.g. "192.168.1.1")
    if let Ok(s) = core::str::from_utf8(host)
        && try_parse_ipv4_fast(s) != u64::MAX
    {
        return true;
    }

    let status = contains_forbidden_domain_code_point_or_upper(host);

    // Fast path: pure lowercase ASCII, no forbidden chars, no xn-- labels.
    if status == 0
        && let Ok(s) = core::str::from_utf8(host)
        && !contains_xn_prefix_pub(s)
    {
        return !crate::unicode::contains_forbidden_domain_code_point(host);
    }

    // Non-ASCII or xn-- labels — validate each label in-place.
    // Uses a stack-allocated [u32; 64] scratch buffer per label (DNS max = 63).
    // **Zero heap allocations.**
    if let Ok(s) = core::str::from_utf8(host) {
        validate_idna_labels_no_alloc(s)
    } else {
        false
    }
}

// ============================================================
// Zero-allocation IDNA label validator
// ============================================================

/// Validate each dot-separated label of `host` for IDNA correctness.
///
/// Uses only stack-allocated scratch buffers — **no heap allocation**.
fn validate_idna_labels_no_alloc(host: &str) -> bool {
    for raw_label in host.split('.') {
        if raw_label.is_empty() {
            continue;
        } // trailing dot is OK

        if let Some(puny_part) = raw_label.strip_prefix("xn--") {
            // ACE label: punycode-decode into a stack buffer, then validate.
            if !validate_xn_label_no_alloc(puny_part) {
                return false;
            }
        } else {
            // Plain label: decode UTF-8 to code points (stack), apply IDNA
            // mapping + validate, using a [u32; 64] stack buffer.
            if !validate_plain_label_no_alloc(raw_label) {
                return false;
            }
        }
    }
    true
}

/// Validate an xn-- (ACE/Punycode) label by decoding into a stack buffer.
fn validate_xn_label_no_alloc(puny_part: &str) -> bool {
    if puny_part.is_empty() {
        return false;
    }

    // Decode punycode into a stack buffer (max 63 code points per DNS label)
    let mut decoded = [0u32; 64];
    let mut len = 0usize;

    if !crate::idna_impl::punycode_decode_into(puny_part, &mut decoded, &mut len) {
        return false;
    }
    let label = &decoded[..len];
    if label.is_empty() {
        return false;
    }

    // All-ASCII decoded labels are invalid in punycode (shouldn't be encoded)
    if label.iter().all(|&cp| cp < 0x80) {
        return false;
    }

    // Check combining-mark at start
    if crate::idna_impl::ccc(label[0]) != 0 {
        return false;
    }

    // Validate each decoded code point via IDNA mapping table
    for &cp in label {
        let idx = crate::idna_impl::find_range_index(cp);
        let status = crate::idna_tables::TABLE[idx][1] & 0xFF;
        if status != 1 {
            return false;
        } // not valid for IDNA
    }

    // Context-J and Bidi checks
    crate::idna_impl::validate_context_and_bidi(label)
}

/// Validate a plain (non-ACE) label: apply IDNA mapping, check validity.
fn validate_plain_label_no_alloc(label: &str) -> bool {
    // Decode into a stack buffer of code points
    let mut cps = [0u32; 128]; // 128 covers any realistic domain label
    let mut len = 0usize;
    for ch in label.chars() {
        if len >= 128 {
            return false;
        }
        cps[len] = ch as u32;
        len += 1;
    }
    let slice = &cps[..len];

    // Apply IDNA mapping: check each code point's status
    let mut mapped = [0u32; 256]; // max after expansion
    let mut mapped_len = 0usize;

    for &cp in slice {
        let idx = crate::idna_impl::find_range_index(cp);
        let descriptor = crate::idna_tables::TABLE[idx][1];
        let code = descriptor & 0xFF;
        match code {
            0 => {} // ignored — skip
            1 => {
                // valid
                if mapped_len >= 256 {
                    return false;
                }
                mapped[mapped_len] = cp;
                mapped_len += 1;
            }
            2 => return false, // disallowed
            _ => {
                // mapped: expand via MAPPINGS table
                let char_count = (descriptor >> 24) as usize;
                let char_idx = ((descriptor >> 8) & 0xFFFF) as usize;
                for i in char_idx..char_idx + char_count {
                    if mapped_len >= 256 {
                        return false;
                    }
                    mapped[mapped_len] = crate::idna_tables::MAPPINGS[i];
                    mapped_len += 1;
                }
            }
        }
    }

    let mapped_slice = &mapped[..mapped_len];
    if mapped_slice.is_empty() {
        return true;
    }

    // NFC normalization check: we apply NFC into another stack buffer
    let mut normed = [0u32; 256];
    let normed_len = nfc_into_stack(mapped_slice, &mut normed);
    if normed_len > 256 {
        return false;
    }
    let norm = &normed[..normed_len];

    // First char must not be a combining mark
    if crate::idna_impl::ccc(norm[0]) != 0 {
        return false;
    }

    // Context-J and Bidi
    crate::idna_impl::validate_context_and_bidi(norm)
}

/// Run NFC normalization entirely on stack arrays — no heap.
/// Returns the output length, or >256 to signal overflow.
fn nfc_into_stack(input: &[u32], out: &mut [u32; 256]) -> usize {
    // Step 1: canonical decompose into a stack buffer
    let mut decomp = [0u32; 512];
    let mut dlen = 0usize;
    for &cp in input {
        let added = decompose_cp_stack(cp, &mut decomp[dlen..]);
        dlen += added;
        if dlen > 512 {
            return 257;
        } // overflow
    }
    // Step 2: sort combining marks (insertion sort, stable)
    sort_combining_stack(&mut decomp[..dlen]);
    // Step 3: canonical compose
    compose_stack(&decomp[..dlen], out)
}

fn decompose_cp_stack(cp: u32, out: &mut [u32]) -> usize {
    use crate::idna_norm_tables::{DECOMP_BLOCK, DECOMP_DATA, DECOMP_INDEX};
    const S_BASE: u32 = 0xAC00;
    const L_BASE: u32 = 0x1100;
    const V_BASE: u32 = 0x1161;
    const T_BASE: u32 = 0x11A7;
    const V_COUNT: u32 = 21;
    const T_COUNT: u32 = 28;
    const N_COUNT: u32 = V_COUNT * T_COUNT;
    const S_COUNT: u32 = 19 * N_COUNT;

    if (S_BASE..S_BASE + S_COUNT).contains(&cp) {
        let si = cp - S_BASE;
        if out.len() < 3 {
            return 0;
        }
        out[0] = L_BASE + si / N_COUNT;
        out[1] = V_BASE + (si % N_COUNT) / T_COUNT;
        let t = si % T_COUNT;
        if t != 0 {
            out[2] = T_BASE + t;
            return 3;
        }
        return 2;
    }
    if cp < 0x110000 {
        let hi = (cp >> 8) as usize;
        if hi < DECOMP_INDEX.len() {
            let block = DECOMP_INDEX[hi] as usize;
            let lo = (cp & 0xFF) as usize;
            let bi = block * 257 + lo;
            if bi + 1 < DECOMP_BLOCK.len() {
                let e0 = DECOMP_BLOCK[bi] as usize;
                let e1 = DECOMP_BLOCK[bi + 1] as usize;
                if (e0 & 1) == 0 {
                    let start = e0 >> 2;
                    let end = e1 >> 2;
                    if start < end {
                        let count = end - start;
                        if out.len() >= count {
                            out[..count].copy_from_slice(&DECOMP_DATA[start..end]);
                            return count;
                        }
                    }
                }
            }
        }
    }
    if !out.is_empty() {
        out[0] = cp;
        1
    } else {
        0
    }
}

fn sort_combining_stack(buf: &mut [u32]) {
    // Insertion sort (stable, in-place)
    for i in 1..buf.len() {
        let cc = crate::idna_impl::ccc(buf[i]);
        if cc == 0 {
            continue;
        }
        let cur = buf[i];
        let mut j = i;
        while j > 0 && crate::idna_impl::ccc(buf[j - 1]) > cc {
            buf[j] = buf[j - 1];
            j -= 1;
        }
        buf[j] = cur;
    }
}

fn compose_stack(input: &[u32], out: &mut [u32; 256]) -> usize {
    use crate::idna_norm_tables::{COMP_BLOCK, COMP_DATA, COMP_INDEX};
    const S_BASE: u32 = 0xAC00;
    const L_BASE: u32 = 0x1100;
    const V_BASE: u32 = 0x1161;
    const T_BASE: u32 = 0x11A7;
    const V_COUNT: u32 = 21;
    const T_COUNT: u32 = 28;
    const N_COUNT: u32 = V_COUNT * T_COUNT;

    let mut out_len = 0usize;
    let mut i = 0usize;
    while i < input.len() {
        if out_len >= 256 {
            return 257;
        }
        let mut starter = input[i];
        i += 1;

        // Hangul LV/LVT
        if (L_BASE..L_BASE + 19).contains(&starter) {
            if i < input.len() && input[i] >= V_BASE && input[i] < V_BASE + V_COUNT {
                starter = S_BASE + (starter - L_BASE) * N_COUNT + (input[i] - V_BASE) * T_COUNT;
                i += 1;
                if i < input.len() && input[i] > T_BASE && input[i] < T_BASE + T_COUNT {
                    starter += input[i] - T_BASE;
                    i += 1;
                }
            }
            out[out_len] = starter;
            out_len += 1;
            continue;
        } else if (S_BASE..S_BASE + V_COUNT * T_COUNT * 19).contains(&starter) {
            if i < input.len()
                && input[i] > T_BASE
                && input[i] < T_BASE + T_COUNT
                && (starter - S_BASE).is_multiple_of(T_COUNT)
            {
                starter += input[i] - T_BASE;
                i += 1;
            }
            out[out_len] = starter;
            out_len += 1;
            continue;
        }

        // General composition
        if starter < 0x110000 {
            let hi = (starter >> 8) as usize;
            if hi < COMP_INDEX.len() {
                let block = COMP_INDEX[hi] as usize;
                let lo = (starter & 0xFF) as usize;
                let bi = block * 257 + lo;
                let (cs, ce) = if bi + 1 < COMP_BLOCK.len() {
                    (COMP_BLOCK[bi] as usize, COMP_BLOCK[bi + 1] as usize)
                } else {
                    (0, 0)
                };

                if cs < ce {
                    let initial = out_len;
                    out[out_len] = starter;
                    out_len += 1;
                    let mut prev_ccc: i32 = -1;
                    while i < input.len() {
                        let cc = crate::idna_impl::ccc(input[i]) as i32;
                        if prev_ccc < cc {
                            // binary search for input[i] in COMP_DATA[cs..ce]
                            let mut lo2 = cs;
                            let mut hi2 = ce;
                            while lo2 + 2 <= hi2 {
                                let mid = lo2 + (((hi2 - lo2) >> 1) & !1usize);
                                match COMP_DATA[mid].cmp(&input[i]) {
                                    core::cmp::Ordering::Equal => {
                                        out[initial] = COMP_DATA[mid + 1];
                                        i += 1;
                                        break;
                                    }
                                    core::cmp::Ordering::Less => lo2 = mid + 2,
                                    core::cmp::Ordering::Greater => {
                                        if mid == 0 {
                                            break;
                                        }
                                        hi2 = mid;
                                    }
                                }
                            }
                        }
                        if cc == 0 {
                            break;
                        }
                        prev_ccc = cc;
                        if out_len >= 256 {
                            return 257;
                        }
                        out[out_len] = input[i];
                        out_len += 1;
                        i += 1;
                    }
                    continue;
                }
            }
        }

        out[out_len] = starter;
        out_len += 1;
    }
    out_len
}

// ============================================================
// IPv6 structural validator (non-allocating, full structural check)
// ============================================================

/// Validates the content between `[` and `]` of an IPv6 host.
///
/// Enforces the RFC 3986 / WHATWG rules:
/// - Exactly 8 hextets of 1-4 hex digits separated by `:`, OR
/// - Fewer hextets with exactly one `::` compression, OR
/// - An embedded IPv4 address in the last two hextet positions.
/// - An optional zone ID after `%` (unreserved ASCII chars only).
fn validate_ipv6_fast(inner: &[u8]) -> bool {
    if inner.is_empty() {
        return false;
    }

    // Split off optional zone ID (e.g. "fe80::1%eth0").
    let addr = match inner.iter().position(|&c| c == b'%') {
        Some(idx) => {
            if idx == 0 {
                return false; // no address before '%'
            }
            let zone = &inner[idx + 1..];
            if zone.is_empty() {
                return false;
            }
            if !zone
                .iter()
                .all(|&c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-' || c == b'_')
            {
                return false;
            }
            &inner[..idx]
        }
        None => inner,
    };

    if addr.is_empty() {
        return false;
    }

    let n = addr.len();
    let mut i = 0usize;
    let mut hextets: u8 = 0;
    let mut has_double_colon = false;

    // Handle leading "::"
    if n >= 2 && addr[0] == b':' && addr[1] == b':' {
        has_double_colon = true;
        i = 2;
        if i == n {
            return true; // bare "::"
        }
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
        if seg.is_empty() {
            return false;
        }

        if has_dot {
            // Embedded IPv4 — must appear at the very end and counts as 2 hextets.
            if !validate_embedded_ipv4(seg) {
                return false;
            }
            hextets = hextets.saturating_add(2);
            if hextets > 8 {
                return false;
            }
            if i < n {
                return false; // nothing may follow the IPv4 part
            }
            break;
        } else {
            if seg.len() > 4 {
                return false; // hextet must be 1-4 hex digits
            }
            hextets = hextets.saturating_add(1);
            if hextets > 8 {
                return false;
            }
        }

        if i < n {
            // Current byte is ':', decide between ':' and '::'
            if i + 1 < n && addr[i + 1] == b':' {
                if has_double_colon {
                    return false; // only one '::' allowed
                }
                has_double_colon = true;
                i += 2;
                if i == n {
                    break; // trailing "::" is valid
                }
            } else {
                i += 1;
                if i == n {
                    return false; // trailing single ':' is invalid
                }
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
