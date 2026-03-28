// Pure-Rust IDNA implementation — ported from Ada's ada_idna.cpp.
// UTS #46 non-transitional processing.

#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

#[cfg(feature = "std")]
extern crate std;
#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(feature = "std")]
use std::{cmp::Ordering, string::String, vec::Vec};
#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};
#[cfg(not(feature = "std"))]
use core::cmp::Ordering;

use crate::idna_norm_tables::{
    CCC_BLOCK, CCC_INDEX, COMP_BLOCK, COMP_DATA, COMP_INDEX, DECOMP_BLOCK, DECOMP_DATA,
    DECOMP_INDEX,
};
use crate::idna_tables::{MAPPINGS, TABLE};

// ============================================================
// IDNA Mapping  (UTS #46 §5)
// ============================================================

fn find_range_index(key: u32) -> usize {
    let mut low: usize = 0;
    let mut high: usize = TABLE.len() - 1;
    while low <= high {
        let mid = (low + high) >> 1;
        let mv = TABLE[mid][0];
        if mv < key {
            low = mid + 1;
        } else if mv > key {
            if mid == 0 {
                return 0;
            }
            high = mid - 1;
        } else {
            return mid;
        }
    }
    if low == 0 { 0 } else { low - 1 }
}

/// Apply UTS #46 IDNA mapping to a sequence of code points.
/// Returns `None` if any disallowed code point is encountered.
fn idna_map(input: &[u32]) -> Option<Vec<u32>> {
    let mut out = Vec::with_capacity(input.len());
    for &cp in input {
        let idx = find_range_index(cp);
        let descriptor = TABLE[idx][1];
        match descriptor & 0xFF {
            0 => {}        // ignored
            1 => out.push(cp), // valid
            2 => return None,  // disallowed
            _ => {
                let char_count = (descriptor >> 24) as usize;
                let char_idx   = ((descriptor >> 8) & 0xFFFF) as usize;
                for m in MAPPINGS.iter().skip(char_idx).take(char_count) {
                    out.push(*m);
                }
            }
        }
    }
    Some(out)
}

// ============================================================
// Hangul constants
// ============================================================
const S_BASE:  u32 = 0xAC00;
const L_BASE:  u32 = 0x1100;
const V_BASE:  u32 = 0x1161;
const T_BASE:  u32 = 0x11A7;
const V_COUNT: u32 = 21;
const T_COUNT: u32 = 28;
const N_COUNT: u32 = V_COUNT * T_COUNT;
const S_COUNT: u32 = 19 * N_COUNT;

// ============================================================
// Canonical Combining Class
// ============================================================

fn ccc(cp: u32) -> u8 {
    if cp >= 0x110000 { return 0; }
    let hi = (cp >> 8) as usize;
    let block = CCC_INDEX[hi] as usize;
    CCC_BLOCK[block * 256 + (cp & 0xFF) as usize]
}

// ============================================================
// Canonical Decomposition (NFD subset used by NFC)
// ============================================================

fn decomp_range(cp: u32) -> (usize, usize, bool) {
    if cp >= 0x110000 { return (0, 0, false); }
    let hi = (cp >> 8) as usize;
    let block = DECOMP_INDEX[hi] as usize;
    let lo = (cp & 0xFF) as usize;
    let bi = block * 257 + lo;
    let e0 = DECOMP_BLOCK[bi]     as usize;
    let e1 = DECOMP_BLOCK[bi + 1] as usize;
    let compat = (e0 & 1) != 0;
    ((e0 >> 2), (e1 >> 2), compat)
}

fn decompose_into(cp: u32, out: &mut Vec<u32>) {
    // Hangul syllable decomposition
    if (S_BASE..S_BASE + S_COUNT).contains(&cp) {
        let si = cp - S_BASE;
        out.push(L_BASE + si / N_COUNT);
        out.push(V_BASE + (si % N_COUNT) / T_COUNT);
        let t = si % T_COUNT;
        if t != 0 { out.push(T_BASE + t); }
        return;
    }
    if cp >= 0x110000 { out.push(cp); return; }
    let (start, end, compat) = decomp_range(cp);
    if start == end || compat {
        out.push(cp); // no canonical decomposition
    } else {
        // Non-recursive: the table stores pre-fully-decomposed sequences
        for &cp in &DECOMP_DATA[start..end] {
            out.push(cp);
        }
    }
}

fn canonical_decompose(input: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(input.len() + 8);
    for &cp in input {
        decompose_into(cp, &mut out);
    }
    out
}

fn sort_combining(buf: &mut [u32]) {
    // Insertion sort on canonical combining class (stable)
    for i in 1..buf.len() {
        let cc = ccc(buf[i]);
        if cc == 0 { continue; }
        let cur = buf[i];
        let mut j = i;
        while j > 0 && ccc(buf[j - 1]) > cc {
            buf[j] = buf[j - 1];
            j -= 1;
        }
        buf[j] = cur;
    }
}

// ============================================================
// Canonical Composition
// ============================================================

fn comp_range(cp: u32) -> (usize, usize) {
    if cp >= 0x110000 { return (0, 0); }
    let hi = (cp >> 8) as usize;
    let block = COMP_INDEX[hi] as usize;
    let lo = (cp & 0xFF) as usize;
    let bi = block * 257 + lo;
    (COMP_BLOCK[bi] as usize, COMP_BLOCK[bi + 1] as usize)
}

#[allow(dead_code)]
fn find_composition(starter: u32, combiner: u32) -> Option<u32> {
    // Hangul composition
    if (L_BASE..L_BASE + 19).contains(&starter) && (V_BASE..V_BASE + V_COUNT).contains(&combiner) {
        return Some(S_BASE + (starter - L_BASE) * N_COUNT + (combiner - V_BASE) * T_COUNT);
    }
    if (S_BASE..S_BASE + S_COUNT).contains(&starter)
        && (starter - S_BASE).is_multiple_of(T_COUNT)
        && combiner > T_BASE && combiner < T_BASE + T_COUNT
    {
        return Some(starter + (combiner - T_BASE));
    }
    let (start, end) = comp_range(starter);
    if start == end { return None; }
    // COMP_DATA pairs: [combiner_cp, composed_cp, combiner_cp, composed_cp, ...]
    // Binary search for combiner in even positions
    let mut lo = start;
    let mut hi = end;
    while lo + 2 <= hi {
        let mid = lo + (((hi - lo) >> 1) & !1usize); // keep even alignment
        match COMP_DATA[mid].cmp(&combiner) {
            Ordering::Equal   => return Some(COMP_DATA[mid + 1]),
            Ordering::Less    => lo = mid + 2,
            Ordering::Greater => { if mid == 0 { return None; } hi = mid; }
        }
    }
    // Check remaining pair
    if lo + 1 < COMP_DATA.len() && COMP_DATA[lo] == combiner {
        return Some(COMP_DATA[lo + 1]);
    }
    None
}

fn compose(buf: &mut Vec<u32>) {
    if buf.len() < 2 { return; }
    let mut composition_count = 0usize;
    let mut input_count = 0usize;

    while input_count < buf.len() {
        // Try Hangul LV/LVT composition
        if buf[input_count] >= L_BASE && buf[input_count] < L_BASE + 19 {
            if input_count + 1 < buf.len()
                && buf[input_count + 1] >= V_BASE
                && buf[input_count + 1] < V_BASE + V_COUNT
            {
                buf[composition_count] = S_BASE
                    + (buf[input_count] - L_BASE) * N_COUNT
                    + (buf[input_count + 1] - V_BASE) * T_COUNT;
                input_count += 2;
                if input_count < buf.len()
                    && buf[input_count] > T_BASE
                    && buf[input_count] < T_BASE + T_COUNT
                {
                    buf[composition_count] += buf[input_count] - T_BASE;
                    input_count += 1;
                }
                composition_count += 1;
                continue;
            }
        } else if buf[input_count] >= S_BASE && buf[input_count] < S_BASE + S_COUNT
            && (buf[input_count] - S_BASE).is_multiple_of(T_COUNT)
                && input_count + 1 < buf.len()
                && buf[input_count + 1] > T_BASE
                && buf[input_count + 1] < T_BASE + T_COUNT
            {
                buf[composition_count] = buf[input_count] + buf[input_count + 1] - T_BASE;
                input_count += 2;
                composition_count += 1;
                continue;
            }

        if buf[input_count] < 0x110000 {
            let (start, end) = comp_range(buf[input_count]);
            let initial = composition_count;
            buf[composition_count] = buf[input_count];
            input_count += 1;

            if start != end {
                let mut prev_ccc: i32 = -1;
                while input_count < buf.len() {
                    let cc = ccc(buf[input_count]) as i32;
                    if prev_ccc < cc {
                        // binary search for buf[input_count] in COMP_DATA[start..end]
                        let mut lo = start;
                        let mut hi = end;
                        let mut found = false;
                        while lo + 2 <= hi {
                            let mid = lo + (((hi - lo) >> 1) & !1usize);
                            match COMP_DATA[mid].cmp(&buf[input_count]) {
                                Ordering::Equal => {
                                    buf[initial] = COMP_DATA[mid + 1];
                                    found = true;
                                    break;
                                }
                                Ordering::Less => lo = mid + 2,
                                Ordering::Greater => {
                                    if mid == 0 { break; }
                                    hi = mid;
                                }
                            }
                        }
                        if !found && lo + 1 < COMP_DATA.len() && COMP_DATA[lo] == buf[input_count] {
                            buf[initial] = COMP_DATA[lo + 1];
                            found = true;
                        }
                        if found {
                            input_count += 1;
                            // update comp range for new starter
                            // (update start/end via the composed char)
                            // NOTE: we do NOT update start/end here because the 
                            // Ada code continues with the same composition pointer.
                            // This is handled by the outer loop advancing composition_count.
                            // For simplicity we just continue with possible re-composition
                            // handled by the next loop iteration.
                            continue;
                        }
                    }
                    if cc == 0 { break; }
                    prev_ccc = cc;
                    composition_count += 1;
                    buf[composition_count] = buf[input_count];
                    input_count += 1;
                }
            }
        } else {
            buf[composition_count] = buf[input_count];
            input_count += 1;
        }
        composition_count += 1;
    }
    buf.truncate(composition_count);
}

// ============================================================
// NFC
// ============================================================

pub fn nfc(input: &[u32]) -> Vec<u32> {
    let mut buf = canonical_decompose(input);
    sort_combining(&mut buf);
    compose(&mut buf);
    buf
}

// ============================================================
// Punycode  (RFC 3492) — ported from ada_idna.cpp
// ============================================================

const PY_BASE:         i32 = 36;
const PY_TMIN:         i32 =  1;
const PY_TMAX:         i32 = 26;
const PY_SKEW:         i32 = 38;
const PY_DAMP:         i32 = 700;
const PY_INITIAL_BIAS: i32 = 72;
const PY_INITIAL_N:    u32 = 128;

fn py_digit(c: u8) -> i32 {
    match c {
        b'a'..=b'z' => (c - b'a') as i32,
        b'0'..=b'9' => (c - b'0') as i32 + 26,
        _ => -1,
    }
}

fn py_char(d: i32) -> char {
    if d < 26 { (d as u8 + b'a') as char } else { (d as u8 + b'0' - 26) as char }
}

fn py_adapt(mut d: i32, n: i32, first: bool) -> i32 {
    d = if first { d / PY_DAMP } else { d / 2 };
    d += d / n;
    let mut k = 0;
    while d > (PY_BASE - PY_TMIN) * PY_TMAX / 2 { d /= PY_BASE - PY_TMIN; k += PY_BASE; }
    k + (PY_BASE - PY_TMIN + 1) * d / (d + PY_SKEW)
}

/// Decode punycode (the part after "xn--") into `out` (appends to existing).
pub fn punycode_decode(input: &str, out: &mut Vec<u32>) -> bool {
    // Per Ada: reject "xn--" prefix — caller strips it already
    if input.starts_with("xn--") { return false; }
    let bytes = input.as_bytes();
    let mut written: i32 = 0;
    let mut n: u32 = PY_INITIAL_N;
    let mut i: i32 = 0;
    let mut bias = PY_INITIAL_BIAS;

    // ASCII prefix before the last '-'
    let delim = bytes.iter().rposition(|&b| b == b'-');
    let delta_start = if let Some(pos) = delim {
        for &b in &bytes[..pos] {
            if b >= 0x80 { return false; }
            out.push(b as u32);
            written += 1;
        }
        pos + 1
    } else { 0 };

    let mut cursor = delta_start;
    while cursor < bytes.len() {
        let old_i = i;
        let mut w: i32 = 1;
        let mut k = PY_BASE;
        loop {
            if cursor >= bytes.len() { return false; }
            let digit = py_digit(bytes[cursor]); cursor += 1;
            if digit < 0 { return false; }
            if digit > (0x7fff_ffff - i) / w { return false; }
            i += digit * w;
            let t = if k <= bias { PY_TMIN } else if k >= bias + PY_TMAX { PY_TMAX } else { k - bias };
            if digit < t { break; }
            if w > 0x7fff_ffff / (PY_BASE - t) { return false; }
            w *= PY_BASE - t;
            k += PY_BASE;
        }
        bias = py_adapt(i - old_i, written + 1, old_i == 0);
        if i / (written + 1) > 0x7fff_ffff - n as i32 { return false; }
        n = n.wrapping_add((i / (written + 1)) as u32);
        i %= written + 1;
        if n < 0x80 { return false; }
        out.insert(i as usize, n);
        written += 1;
        i += 1;
    }
    true
}

/// Encode code points to punycode. Returns `None` on overflow/invalid input.
pub fn punycode_encode(input: &[u32]) -> Option<String> {
    let mut out = String::with_capacity(input.len() + 4);
    let mut n:    u32 = PY_INITIAL_N;
    let mut d:    i32 = 0;
    let mut bias: i32 = PY_INITIAL_BIAS;
    let mut h:  usize = 0;

    for &cp in input {
        if cp > 0x10FFFF || (0xD800..0xE000).contains(&cp) { return None; }
        if cp < 0x80 { h += 1; out.push(cp as u8 as char); }
    }
    let b = h;
    if b > 0 { out.push('-'); }

    while h < input.len() {
        let m = input.iter().filter(|&&cp| cp >= n).fold(0x10FFFF_u32, |acc, &cp| acc.min(cp));
        let dm = ((m - n) as i64).checked_mul((h as i64) + 1)?;
        if dm > 0x7fff_ffff - d as i64 { return None; }
        d += dm as i32;
        n = m;
        for &cp in input {
            if cp < n { d = d.checked_add(1)?; }
            if cp == n {
                let mut q = d;
                let mut k = PY_BASE;
                loop {
                    let t = if k <= bias { PY_TMIN } else if k >= bias + PY_TMAX { PY_TMAX } else { k - bias };
                    if q < t { break; }
                    out.push(py_char(t + (q - t) % (PY_BASE - t)));
                    q = (q - t) / (PY_BASE - t);
                    k += PY_BASE;
                }
                out.push(py_char(q));
                bias = py_adapt(d, (h as i32) + 1, h == b);
                d = 0;
                h += 1;
            }
        }
        d += 1;
        n += 1;
    }
    Some(out)
}

// ============================================================
// Label validation helpers
// ============================================================

fn is_combining_start(cp: u32) -> bool {
    ccc(cp) != 0
}

fn label_is_valid(label: &[u32]) -> bool {
    if label.is_empty() { return true; }
    // Must not start with a combining mark
    if is_combining_start(label[0]) { return false; }
    true
}

// ============================================================
// Process one label: map, normalize, punycode-encode if needed
// ============================================================

// ============================================================
// Virama code points (for Context-J ZWJ/ZWNJ rules)
// Source: Ada IDNA idna.cpp
// ============================================================
const VIRAMA: &[u32] = &[
    0x094D, 0x09CD, 0x0A4D, 0x0ACD, 0x0B4D, 0x0BCD, 0x0C4D, 0x0CCD,
    0x0D3B, 0x0D3C, 0x0D4D, 0x0DCA, 0x0E3A, 0x0EBA, 0x0F84, 0x1039,
    0x103A, 0x1714, 0x1734, 0x17D2, 0x1A60, 0x1B44, 0x1BAA, 0x1BAB,
    0x1BF2, 0x1BF3, 0x2D7F, 0xA806, 0xA82C, 0xA8C4, 0xA953, 0xA9C0,
    0xAAF6, 0xABED, 0x10A3F, 0x11046, 0x1107F, 0x110B9, 0x11133, 0x11134,
    0x111C0, 0x11235, 0x112EA, 0x1134D, 0x11442, 0x114C2, 0x115BF, 0x1163F,
    0x116B6, 0x1172B, 0x11839, 0x1193D, 0x1193E, 0x119E0, 0x11A34, 0x11A47,
    0x11A99, 0x11C3F, 0x11D44, 0x11D45, 0x11D97,
];

fn is_virama(cp: u32) -> bool {
    VIRAMA.binary_search(&cp).is_ok()
}

/// Arabic and Hebrew script characters used as joining chars in Context-J rule.
fn is_arabic_hebrew(cp: u32) -> bool {
    matches!(cp,
        0x0600..=0x06FF  // Arabic
        | 0x0750..=0x077F  // Arabic Supplement
        | 0x08A0..=0x08FF  // Arabic Extended-A
        | 0xFB50..=0xFDFF  // Arabic Presentation Forms-A
        | 0xFE70..=0xFEFF  // Arabic Presentation Forms-B
        | 0x0590..=0x05FF  // Hebrew
        | 0xFB1D..=0xFB4F  // Hebrew Presentation Forms
        | 0x0900..=0x097F  // Devanagari
        | 0x0980..=0x09FF  // Bengali
        | 0x0A00..=0x0A7F  // Gurmukhi
        | 0x0A80..=0x0AFF  // Gujarati
        | 0x0B00..=0x0B7F  // Oriya
        | 0x0B80..=0x0BFF  // Tamil
        | 0x0C00..=0x0C7F  // Telugu
        | 0x0C80..=0x0CFF  // Kannada
        | 0x0D00..=0x0D7F  // Malayalam
        | 0x0D80..=0x0DFF  // Sinhala
        | 0x07C0..=0x07FF  // NKo
        | 0x1800..=0x18AF  // Mongolian
    )
}

/// Bidi character classes (simplified: L=left-to-right, R/AL=right-to-left)
fn bidi_class(cp: u32) -> u8 {
    // Common Arabic range (AL)
    if (0x0600..=0x08FF).contains(&cp) { return 2; } // RTL/Arabic
    // Hebrew range (R)
    if (0x0590..=0x05FF).contains(&cp) { return 2; } // RTL
    // Arabic Presentation Forms
    if (0xFB1D..=0xFDFF).contains(&cp) || (0xFE70..=0xFEFF).contains(&cp) { return 2; }
    // Most Latin/ASCII: L
    if cp < 0x0590 { return 1; }
    // Default: non-strongly directional
    0
}

/// Validate Context-J rules (U+200C, U+200D) and basic Bidi rules.
fn validate_context_and_bidi(label: &[u32]) -> bool {
    let mut has_rtl = false;
    let mut has_ltr = false;

    for (i, &cp) in label.iter().enumerate() {
        // Context-J: U+200D (ZERO WIDTH JOINER)
        if cp == 0x200D {
            // Valid only if immediately preceded by a virama
            if i == 0 || !is_virama(label[i - 1]) {
                return false;
            }
        }
        // Context-J: U+200C (ZERO WIDTH NON-JOINER)
        if cp == 0x200C {
            if i == 0 || i + 1 >= label.len() { return false; }
            // Case 1: preceded by virama
            if is_virama(label[i - 1]) { continue; }
            // Case 2: Indic joining context — must have an L-or-D character
            // before it AND an R-or-D character after it.
            // Simplified: any Arabic/Hebrew script character qualifies.
            let has_joining_before = label[..i].iter().any(|&c| is_arabic_hebrew(c));
            let has_joining_after  = label[i+1..].iter().any(|&c| is_arabic_hebrew(c));
            if !(has_joining_before && has_joining_after) {
                return false;
            }
        }
        let bc = bidi_class(cp);
        if bc == 1 { has_ltr = true; }
        if bc == 2 { has_rtl = true; }
    }

    // Simplified Bidi: a label must not mix strongly LTR and strongly RTL characters
    if has_ltr && has_rtl { return false; }
    true
}

fn process_label(label_str: &str) -> Option<String> {
    // Validate xn-- (ACE) labels: the punycode must decode correctly and
    // every decoded code point must be IDNA-valid.
    if let Some(puny_part) = label_str.strip_prefix("xn--") {
        if puny_part.is_empty() {
            return None; // "xn--" with no payload is invalid
        }
        let mut decoded: Vec<u32> = Vec::new();
        if !punycode_decode(puny_part, &mut decoded) {
            return None; // punycode decode failure
        }
        if decoded.iter().all(|&cp| cp < 0x80) {
            return None; // all-ASCII should not be punycode-encoded
        }
        if !label_is_valid(&decoded) {
            return None;
        }
        // Every decoded code point must have IDNA status = valid (1).
        // Ignored (0), disallowed (2), or mapped (≥3) → invalid label.
        for &cp in &decoded {
            let idx = find_range_index(cp);
            let status = TABLE[idx][1] & 0xFF;
            if status != 1 {
                return None;
            }
        }
        // Check Context-J rules and Bidi rules on the decoded label.
        if !validate_context_and_bidi(&decoded) {
            return None;
        }
        return Some(String::from(label_str)); // keep the xn-- form unchanged
    }

    // Fast path: pure lowercase ASCII label (no mapping needed)
    let bytes = label_str.as_bytes();
    if bytes.iter().all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') {
        if !label_str.is_empty() {
            return Some(String::from(label_str));
        }
        return Some(String::new());
    }

    // Decode to code points
    let codepoints: Vec<u32> = label_str.chars().map(|c| c as u32).collect();
    // Map
    let mapped = idna_map(&codepoints)?;
    // Normalize (NFC)
    let normalized = nfc(&mapped);
    if normalized.is_empty() { return Some(String::new()); }
    // Validate
    if !label_is_valid(&normalized) { return None; }
    // Validate Context-J and Bidi rules on the normalized label
    if !validate_context_and_bidi(&normalized) {
        return None;
    }

    // All ASCII?
    let all_ascii = normalized.iter().all(|&cp| cp < 0x80);
    if all_ascii {
        let s: String = normalized.iter().map(|&cp| cp as u8 as char).collect();
        return Some(s);
    }
    // Punycode encode
    let encoded = punycode_encode(&normalized)?;
    { let mut label = String::from("xn--"); label.push_str(&encoded); Some(label) }
}

// ============================================================
// Public API
// ============================================================

/// Convert a (potentially internationalized) domain name to its
/// ASCII-compatible encoding (ACE / Punycode).
/// Returns `None` on failure.
pub fn domain_to_ascii(input: &str) -> Option<String> {
    // Decode from UTF-8 to code points
    let codepoints: Vec<u32> = input.chars().map(|c| c as u32).collect();
    // Apply IDNA mapping to the whole input (handles case fold, etc.)
    let mapped = idna_map(&codepoints)?;
    // NFC the whole thing
    let normalized = nfc(&mapped);

    // Reconstruct as string, then split on dot separators
    let normalized_str: String = normalized.iter()
        .filter_map(|&cp| char::from_u32(cp))
        .collect();

    // Split on dots (U+002E, U+FF0E, U+3002, U+FF61 all map to '.' after IDNA mapping)
    let mut result = String::with_capacity(input.len());
    let mut first = true;
    for label_str in normalized_str.split('.') {
        if !first { result.push('.'); }
        first = false;
        let processed = process_label(label_str)?;
        result.push_str(&processed);
    }
    if result.is_empty() { return None; }

    // Reject if the final result contains any forbidden domain code points
    // (e.g. ':', space, control characters that slipped through IDNA mapping).
    for b in result.bytes() {
        if crate::unicode::is_forbidden_domain_code_point(b) {
            return None;
        }
    }
    Some(result)
}

#[cfg_attr(not(feature = "std"), allow(dead_code))]
/// Decode an ACE/Punycode domain back to Unicode.
pub fn domain_to_unicode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut first = true;
    for label in input.split('.') {
        if !first { result.push('.'); }
        first = false;
        if let Some(suffix) = label.strip_prefix("xn--") {
            let mut decoded: Vec<u32> = Vec::new();
            if punycode_decode(suffix, &mut decoded) {
                let s: String = decoded.iter()
                    .filter_map(|&cp| char::from_u32(cp))
                    .collect();
                result.push_str(&s);
            } else {
                result.push_str(label); // keep as-is on error
            }
        } else {
            result.push_str(label);
        }
    }
    result
}
