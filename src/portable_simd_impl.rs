//! `std::simd` (portable SIMD) implementations of every hot scanning path.
//!
//! Enabled by `--features nightly-simd` on a nightly toolchain.
//! Unlike the `simd` feature (which uses platform-specific `std::arch`
//! intrinsics with explicit SSE3/NEON code paths), this module uses
//! `std::simd` which LLVM lowers to the best available ISA automatically —
//! SSE2/AVX2 on x86-64, NEON on AArch64, SVE on AArch64 with SVE, etc.
//!
//! All public functions are drop-in replacements for the scalar versions in
//! `unicode.rs`, `helpers.rs`, and `checkers.rs`.

use std::simd::Select;
use std::simd::cmp::{SimdPartialEq, SimdPartialOrd};
use std::simd::prelude::*;

// SWAR helper shared by several functions for short inputs
#[inline(always)]
fn swar_to_lower_ascii(buf: &mut [u8]) -> bool {
    const M80: u64 = 0x8080_8080_8080_8080;
    const AP: u64 = 0x3f3f_3f3f_3f3f_3f3f; // broadcast(128 - b'A')
    const ZP: u64 = 0x2525_2525_2525_2525; // broadcast(128 - b'Z' - 1)
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
        unsafe { core::ptr::copy_nonoverlapping(ptr.add(i), &mut w as *mut u64 as *mut u8, rem) };
        non_ascii |= w & M80;
        w ^= (((w.wrapping_add(AP)) ^ (w.wrapping_add(ZP))) & M80) >> 2;
        unsafe { core::ptr::copy_nonoverlapping(&w as *const u64 as *const u8, ptr.add(i), rem) };
    }
    non_ascii == 0
}

// ============================================================
// to_lower_ascii
// ============================================================

/// ASCII-lowercase `buf` in-place. Returns `true` iff every byte is ASCII.
///
/// Uses 32-lane `u8x32`; LLVM maps this to:
///   • AVX2 on x86-64 with AVX2 — 32 bytes/cycle
///   • SSE2 on x86-64 baseline — two 16-byte passes
///   • NEON on AArch64 — 32 bytes/cycle
#[inline]
pub fn to_lower_ascii(buf: &mut [u8]) -> bool {
    // Short inputs (e.g. scheme names "https" = 5 bytes) don't benefit from
    // 16-lane SIMD — use SWAR (8 bytes/iter) which has lower setup overhead.
    if buf.len() < 16 {
        return swar_to_lower_ascii(buf);
    }
    const N: usize = 16;
    let upper_a = Simd::<u8, N>::splat(b'A');
    let upper_z = Simd::<u8, N>::splat(b'Z');
    // 0x20 = bit5 (inline)
    let hi_bit = Simd::<u8, N>::splat(0x80); // non-ASCII flag

    let mut non_ascii = Simd::<u8, N>::splat(0);
    let n = buf.len();
    let mut i = 0;

    while i + N <= n {
        // SAFETY: bounds checked above
        let chunk = Simd::<u8, N>::from_slice(&buf[i..i + N]);
        non_ascii |= chunk & hi_bit;
        let is_upper = chunk.simd_ge(upper_a) & chunk.simd_le(upper_z);
        // Select: where is_upper → chunk | 0x20, else → chunk unchanged
        // This compiles to a single VPBLENDVB/PBLENDVB or VORN instruction.
        let lowered = is_upper.select(chunk | Simd::<u8, N>::splat(0x20), chunk);
        lowered.copy_to_slice(&mut buf[i..i + N]);
        i += N;
    }

    // Scalar tail
    let mut tail_non_ascii = false;
    while i < n {
        let b = buf[i];
        if b.is_ascii_uppercase() {
            buf[i] = b | 0x20;
        } else if b >= 0x80 {
            tail_non_ascii = true;
        }
        i += 1;
    }
    non_ascii == Simd::splat(0) && !tail_non_ascii
}

// ============================================================
// has_tabs_or_newline
// ============================================================

/// Returns `true` if `s` contains `\t`, `\n`, or `\r`.
#[inline]
pub fn has_tabs_or_newline(s: &[u8]) -> bool {
    const N: usize = 32;
    let tab = Simd::<u8, N>::splat(b'\t');
    let lf = Simd::<u8, N>::splat(b'\n');
    let cr = Simd::<u8, N>::splat(b'\r');

    let mut found = false;
    let (chunks, rem) = s.as_chunks::<N>();
    'outer: for chunk in chunks {
        let v = Simd::from_array(*chunk);
        let hit = v.simd_eq(tab) | v.simd_eq(lf) | v.simd_eq(cr);
        if hit.any() {
            found = true;
            break 'outer;
        }
    }
    found || rem.iter().any(|&c| matches!(c, b'\t' | b'\n' | b'\r'))
}

// ============================================================
// percent_encode_index — generic (bit-table)
// ============================================================

/// Find the first byte in `input` that needs percent-encoding per `charset`.
///
/// **Two-pass strategy** (portable, no gather instruction needed):
/// 1. SIMD pass: reject bytes that are *definitely safe* (printable ASCII
///    outside `\x20` and `\x7F` boundaries) using range comparisons.
///    Any byte in 0x21–0x7E that needs encoding is caught in pass 2.
/// 2. Scalar pass: for the bytes the SIMD couldn't confirm safe, use
///    the 32-byte bit-table.
///
/// For the common character sets where only a handful of printable ASCII
/// bytes are flagged, pass 2 rarely triggers.
#[inline]
pub fn percent_encode_index(input: &str, charset: &[u8; 32]) -> usize {
    let b = input.as_bytes();
    const N: usize = 16;

    // Splats for range checks
    let lo_safe = Simd::<u8, N>::splat(0x21); // < 0x21 → always encode
    let hi_safe = Simd::<u8, N>::splat(0x7E); // > 0x7E → always encode

    // Precompute the specific printable bytes that need encoding in this set
    // (those in 0x21..=0x7E where bit_at is true)
    let mut printable_need_enc = [false; 94]; // 0x21..=0x7E
    for c in 0x21u8..=0x7Eu8 {
        if crate::character_sets::bit_at(charset, c) {
            printable_need_enc[(c - 0x21) as usize] = true;
        }
    }

    let n = b.len();
    let mut i = 0;

    while i + N <= n {
        let chunk = Simd::<u8, N>::from_slice(&b[i..i + N]);

        // Range check: flag bytes outside 0x21–0x7E (always need encoding)
        let outside = chunk.simd_lt(lo_safe) | chunk.simd_gt(hi_safe);
        let outside_mask = outside.to_bitmask();
        if outside_mask != 0 {
            return i + outside_mask.trailing_zeros() as usize;
        }

        // All 16 bytes are in 0x21–0x7E; check the bit table for each
        // using a simple scalar loop (these are typically a no-op for clean inputs)
        let arr = chunk.to_array();
        for (k, &byte) in arr.iter().enumerate() {
            if printable_need_enc[(byte - 0x21) as usize] {
                return i + k;
            }
        }
        i += N;
    }

    // Scalar tail
    while i < n {
        if crate::character_sets::bit_at(charset, b[i]) {
            return i;
        }
        i += 1;
    }
    n
}

// ============================================================
// percent_encode_index — specialised for hot character sets
// ============================================================

/// SIMD `percent_encode_index` for `SPECIAL_QUERY_PERCENT_ENCODE`.
///
/// Encoding set: C0 (0x00–0x20), `"` 0x22, `#` 0x23, `'` 0x27,
///               `<` 0x3C, `>` 0x3E, DEL 0x7F, 0x80–0xFF.
#[inline]
pub fn percent_encode_index_special_query(input: &str) -> usize {
    let b = input.as_bytes();
    const N: usize = 16;

    let c0_end = Simd::<u8, N>::splat(0x21); // < 0x21
    let hi_start = Simd::<u8, N>::splat(0x7F); // >= 0x7F
    let eq22 = Simd::<u8, N>::splat(0x22);
    let eq23 = Simd::<u8, N>::splat(0x23);
    let eq27 = Simd::<u8, N>::splat(0x27);
    let eq3c = Simd::<u8, N>::splat(0x3C);
    let eq3e = Simd::<u8, N>::splat(0x3E);

    let n = b.len();
    let mut i = 0;

    while i + N <= n {
        let v = Simd::<u8, N>::from_slice(&b[i..i + N]);
        let needs = v.simd_lt(c0_end)
            | v.simd_ge(hi_start)
            | v.simd_eq(eq22)
            | v.simd_eq(eq23)
            | v.simd_eq(eq27)
            | v.simd_eq(eq3c)
            | v.simd_eq(eq3e);
        let mask = needs.to_bitmask();
        if mask != 0 {
            return i + mask.trailing_zeros() as usize;
        }
        i += N;
    }
    // Scalar tail
    while i < n {
        if crate::character_sets::bit_at(&crate::character_sets::SPECIAL_QUERY_PERCENT_ENCODE, b[i])
        {
            return i;
        }
        i += 1;
    }
    n
}

/// SIMD `percent_encode_index` for `QUERY_PERCENT_ENCODE`.
///
/// Like special-query but without `'` (0x27).
#[inline]
pub fn percent_encode_index_query(input: &str) -> usize {
    let b = input.as_bytes();
    const N: usize = 16;

    let c0_end = Simd::<u8, N>::splat(0x21);
    let hi_start = Simd::<u8, N>::splat(0x7F);
    let eq22 = Simd::<u8, N>::splat(0x22);
    let eq23 = Simd::<u8, N>::splat(0x23);
    let eq3c = Simd::<u8, N>::splat(0x3C);
    let eq3e = Simd::<u8, N>::splat(0x3E);

    let n = b.len();
    let mut i = 0;

    while i + N <= n {
        let v = Simd::<u8, N>::from_slice(&b[i..i + N]);
        let needs = v.simd_lt(c0_end)
            | v.simd_ge(hi_start)
            | v.simd_eq(eq22)
            | v.simd_eq(eq23)
            | v.simd_eq(eq3c)
            | v.simd_eq(eq3e);
        let mask = needs.to_bitmask();
        if mask != 0 {
            return i + mask.trailing_zeros() as usize;
        }
        i += N;
    }
    while i < n {
        if crate::character_sets::bit_at(&crate::character_sets::QUERY_PERCENT_ENCODE, b[i]) {
            return i;
        }
        i += 1;
    }
    n
}

/// SIMD `percent_encode_index` for `PATH_PERCENT_ENCODE`.
///
/// Encodes: C0, space, `"`, `#`, `<`, `>`, `?`, `^`, `` ` ``, `{`, `|`, `}`, DEL, 0x80+.
#[inline]
pub fn percent_encode_index_path(input: &str) -> usize {
    let b = input.as_bytes();
    const N: usize = 16;

    let c0_end = Simd::<u8, N>::splat(0x21);
    let hi_start = Simd::<u8, N>::splat(0x7F);
    // Specific chars in 0x21–0x7E
    let eq22 = Simd::<u8, N>::splat(0x22); // "
    let eq23 = Simd::<u8, N>::splat(0x23); // #
    let eq3c = Simd::<u8, N>::splat(0x3C); // <
    let eq3e = Simd::<u8, N>::splat(0x3E); // >
    let eq3f = Simd::<u8, N>::splat(0x3F); // ?
    let eq5e = Simd::<u8, N>::splat(0x5E); // ^
    let eq60 = Simd::<u8, N>::splat(0x60); // `
    let eq7b = Simd::<u8, N>::splat(0x7B); // {
    let eq7c = Simd::<u8, N>::splat(0x7C); // |
    let eq7d = Simd::<u8, N>::splat(0x7D); // }

    let n = b.len();
    let mut i = 0;

    while i + N <= n {
        let v = Simd::<u8, N>::from_slice(&b[i..i + N]);
        let needs = v.simd_lt(c0_end)
            | v.simd_ge(hi_start)
            | v.simd_eq(eq22)
            | v.simd_eq(eq23)
            | v.simd_eq(eq3c)
            | v.simd_eq(eq3e)
            | v.simd_eq(eq3f)
            | v.simd_eq(eq5e)
            | v.simd_eq(eq60)
            | v.simd_eq(eq7b)
            | v.simd_eq(eq7c)
            | v.simd_eq(eq7d);
        let mask = needs.to_bitmask();
        if mask != 0 {
            return i + mask.trailing_zeros() as usize;
        }
        i += N;
    }
    while i < n {
        if crate::character_sets::bit_at(&crate::character_sets::PATH_PERCENT_ENCODE, b[i]) {
            return i;
        }
        i += 1;
    }
    n
}

/// SIMD `percent_encode_index` for `FRAGMENT_PERCENT_ENCODE`.
#[inline]
pub fn percent_encode_index_fragment(input: &str) -> usize {
    let b = input.as_bytes();
    const N: usize = 16;

    let c0_end = Simd::<u8, N>::splat(0x21);
    let hi_start = Simd::<u8, N>::splat(0x7F);
    let eq22 = Simd::<u8, N>::splat(0x22);
    let eq3c = Simd::<u8, N>::splat(0x3C);
    let eq3e = Simd::<u8, N>::splat(0x3E);
    let eq60 = Simd::<u8, N>::splat(0x60);

    let n = b.len();
    let mut i = 0;

    while i + N <= n {
        let v = Simd::<u8, N>::from_slice(&b[i..i + N]);
        let needs = v.simd_lt(c0_end)
            | v.simd_ge(hi_start)
            | v.simd_eq(eq22)
            | v.simd_eq(eq3c)
            | v.simd_eq(eq3e)
            | v.simd_eq(eq60);
        let mask = needs.to_bitmask();
        if mask != 0 {
            return i + mask.trailing_zeros() as usize;
        }
        i += N;
    }
    while i < n {
        if crate::character_sets::bit_at(&crate::character_sets::FRAGMENT_PERCENT_ENCODE, b[i]) {
            return i;
        }
        i += 1;
    }
    n
}

// ============================================================
// path_signature
// ============================================================

/// Compute the path-signature byte using 32-lane SIMD.
///
/// Bit 0 (0x01) — needs percent-encoding  
/// Bit 1 (0x02) — backslash  
/// Bit 2 (0x04) — dot  
/// Bit 3 (0x08) — percent sign
#[inline]
pub fn path_signature(input: &str) -> u8 {
    let b = input.as_bytes();
    // For short paths (common: /foo, /path) fall back to the fast scalar loop
    if b.len() < 16 {
        let mut acc = 0u8;
        for &byte in b {
            acc |= crate::checkers::PATH_SIG_TABLE[byte as usize];
        }
        return acc;
    }
    const N: usize = 16;

    // Range sentinels
    let c0_end = Simd::<u8, N>::splat(0x21); // < 0x21 → encoding needed
    let hi_start = Simd::<u8, N>::splat(0x7F); // >= 0x7F → encoding needed
    // Specific bytes that need encoding in 0x21–0x7E
    let eq22 = Simd::<u8, N>::splat(0x22); // "
    let eq23 = Simd::<u8, N>::splat(0x23); // #
    let eq3c = Simd::<u8, N>::splat(0x3C); // <
    let eq3e = Simd::<u8, N>::splat(0x3E); // >
    let eq3f = Simd::<u8, N>::splat(0x3F); // ?
    let eq5e = Simd::<u8, N>::splat(0x5E); // ^
    let eq60 = Simd::<u8, N>::splat(0x60); // `
    let eq7b = Simd::<u8, N>::splat(0x7B); // {
    let eq7c = Simd::<u8, N>::splat(0x7C); // |
    let eq7d = Simd::<u8, N>::splat(0x7D); // }
    // Special-flag bytes
    let eq5c = Simd::<u8, N>::splat(0x5C); // backslash
    let eq2e = Simd::<u8, N>::splat(0x2E); // dot
    let eq25 = Simd::<u8, N>::splat(0x25); // percent

    let mut acc: u8 = 0;
    let n = b.len();
    let mut i = 0;

    while i + N <= n {
        let v = Simd::<u8, N>::from_slice(&b[i..i + N]);

        // bit 1 (0x02): backslash
        if (v.simd_eq(eq5c)).any() {
            acc |= 0x02;
        }
        // bit 2 (0x04): dot
        if (v.simd_eq(eq2e)).any() {
            acc |= 0x04;
        }
        // bit 3 (0x08): percent
        if (v.simd_eq(eq25)).any() {
            acc |= 0x08;
        }
        // bit 0 (0x01): needs encoding
        let needs_enc = v.simd_lt(c0_end)
            | v.simd_ge(hi_start)
            | v.simd_eq(eq22)
            | v.simd_eq(eq23)
            | v.simd_eq(eq3c)
            | v.simd_eq(eq3e)
            | v.simd_eq(eq3f)
            | v.simd_eq(eq5e)
            | v.simd_eq(eq60)
            | v.simd_eq(eq7b)
            | v.simd_eq(eq7c)
            | v.simd_eq(eq7d);
        if needs_enc.any() {
            acc |= 0x01;
        }

        if acc == 0x0F {
            return acc;
        } // all bits set, stop early
        i += N;
    }

    // Scalar tail
    while i < n {
        acc |= crate::checkers::PATH_SIG_TABLE[b[i] as usize];
        i += 1;
    }
    acc
}

// ============================================================
// Authority + host delimiter finding
// ============================================================

/// Find the first byte in `view` that is `@`, `/`, `\`, or `?`.
#[inline]
pub fn find_authority_delimiter_special(view: &str) -> usize {
    let b = view.as_bytes();
    find_any_of_4(b, b'@', b'/', b'\\', b'?')
}

/// Find the first byte in `view` that is `@`, `/`, or `?`.
#[inline]
pub fn find_authority_delimiter(view: &str) -> usize {
    let b = view.as_bytes();
    find_any_of_3(b, b'@', b'/', b'?')
}

/// Find the first byte from `from` in `view` that is `:`, `/`, `\`, `?`, or `[`.
#[inline]
pub fn find_next_host_delimiter_special(view: &str, from: usize) -> usize {
    let b = &view.as_bytes()[from..];
    from + find_any_of_5(b, b':', b'/', b'\\', b'?', b'[')
}

/// Find the first byte from `from` in `view` that is `:`, `/`, `?`, or `[`.
#[inline]
pub fn find_next_host_delimiter(view: &str, from: usize) -> usize {
    let b = &view.as_bytes()[from..];
    from + find_any_of_4(b, b':', b'/', b'?', b'[')
}

// ============================================================
// Generic N-char delimiter finders
// ============================================================

#[inline]
fn find_any_of_3(b: &[u8], d0: u8, d1: u8, d2: u8) -> usize {
    const N: usize = 16;
    let s0 = Simd::<u8, N>::splat(d0);
    let s1 = Simd::<u8, N>::splat(d1);
    let s2 = Simd::<u8, N>::splat(d2);
    let n = b.len();
    let mut i = 0;
    while i + N <= n {
        let v = Simd::<u8, N>::from_slice(&b[i..i + N]);
        let hit = v.simd_eq(s0) | v.simd_eq(s1) | v.simd_eq(s2);
        let mask = hit.to_bitmask();
        if mask != 0 {
            return i + mask.trailing_zeros() as usize;
        }
        i += N;
    }
    for (k, &c) in b[i..].iter().enumerate() {
        if c == d0 || c == d1 || c == d2 {
            return i + k;
        }
    }
    n
}

#[inline]
fn find_any_of_4(b: &[u8], d0: u8, d1: u8, d2: u8, d3: u8) -> usize {
    const N: usize = 16;
    let s0 = Simd::<u8, N>::splat(d0);
    let s1 = Simd::<u8, N>::splat(d1);
    let s2 = Simd::<u8, N>::splat(d2);
    let s3 = Simd::<u8, N>::splat(d3);
    let n = b.len();
    let mut i = 0;
    while i + N <= n {
        let v = Simd::<u8, N>::from_slice(&b[i..i + N]);
        let hit = v.simd_eq(s0) | v.simd_eq(s1) | v.simd_eq(s2) | v.simd_eq(s3);
        let mask = hit.to_bitmask();
        if mask != 0 {
            return i + mask.trailing_zeros() as usize;
        }
        i += N;
    }
    for (k, &c) in b[i..].iter().enumerate() {
        if c == d0 || c == d1 || c == d2 || c == d3 {
            return i + k;
        }
    }
    n
}

#[inline]
fn find_any_of_5(b: &[u8], d0: u8, d1: u8, d2: u8, d3: u8, d4: u8) -> usize {
    const N: usize = 16;
    let s0 = Simd::<u8, N>::splat(d0);
    let s1 = Simd::<u8, N>::splat(d1);
    let s2 = Simd::<u8, N>::splat(d2);
    let s3 = Simd::<u8, N>::splat(d3);
    let s4 = Simd::<u8, N>::splat(d4);
    let n = b.len();
    let mut i = 0;
    while i + N <= n {
        let v = Simd::<u8, N>::from_slice(&b[i..i + N]);
        let hit = v.simd_eq(s0) | v.simd_eq(s1) | v.simd_eq(s2) | v.simd_eq(s3) | v.simd_eq(s4);
        let mask = hit.to_bitmask();
        if mask != 0 {
            return i + mask.trailing_zeros() as usize;
        }
        i += N;
    }
    for (k, &c) in b[i..].iter().enumerate() {
        if c == d0 || c == d1 || c == d2 || c == d3 || c == d4 {
            return i + k;
        }
    }
    n
}

// ============================================================
// Domain-status check
// ============================================================

/// Scan `s` for forbidden domain code points and/or ASCII uppercase letters.
/// Returns the same flag byte as [`contains_forbidden_domain_code_point_or_upper`]:
///   bit 0 → forbidden code point, bit 1 → uppercase letter.
///
/// [`contains_forbidden_domain_code_point_or_upper`]: crate::unicode::contains_forbidden_domain_code_point_or_upper
#[inline]
pub fn contains_forbidden_domain_code_point_or_upper(s: &[u8]) -> u8 {
    // Most hostname checks are < 32 bytes; use scalar for short inputs
    if s.len() < 16 {
        let mut acc = 0u8;
        let mut i = 0;
        while i + 4 <= s.len() {
            acc |= crate::unicode::DOMAIN_CHECK[s[i] as usize]
                | crate::unicode::DOMAIN_CHECK[s[i + 1] as usize]
                | crate::unicode::DOMAIN_CHECK[s[i + 2] as usize]
                | crate::unicode::DOMAIN_CHECK[s[i + 3] as usize];
            i += 4;
        }
        while i < s.len() {
            acc |= crate::unicode::DOMAIN_CHECK[s[i] as usize];
            i += 1;
        }
        return acc;
    }
    const N: usize = 16;

    // Forbidden: < 0x21, > 0x7E, or specific chars
    let lo = Simd::<u8, N>::splat(0x21);
    let hi = Simd::<u8, N>::splat(0x7E);
    // Specific forbidden chars in printable range
    // # / : < > ? @ [ \ ] ^ |  %
    let hash = Simd::<u8, N>::splat(b'#');
    let slash = Simd::<u8, N>::splat(b'/');
    let colon = Simd::<u8, N>::splat(b':');
    let lt = Simd::<u8, N>::splat(b'<');
    let gt = Simd::<u8, N>::splat(b'>');
    let qm = Simd::<u8, N>::splat(b'?');
    let at = Simd::<u8, N>::splat(b'@');
    let lbr = Simd::<u8, N>::splat(b'[');
    let bsl = Simd::<u8, N>::splat(b'\\');
    let rbr = Simd::<u8, N>::splat(b']');
    let caret = Simd::<u8, N>::splat(b'^');
    let pipe = Simd::<u8, N>::splat(b'|');
    let pct = Simd::<u8, N>::splat(b'%');
    // Uppercase: A–Z
    let ua = Simd::<u8, N>::splat(b'A');
    let uz = Simd::<u8, N>::splat(b'Z');

    let mut acc: u8 = 0;
    let n = s.len();
    let mut i = 0;

    while i + N <= n {
        let v = Simd::<u8, N>::from_slice(&s[i..i + N]);

        let forbidden = v.simd_lt(lo)
            | v.simd_gt(hi)
            | v.simd_eq(hash)
            | v.simd_eq(slash)
            | v.simd_eq(colon)
            | v.simd_eq(lt)
            | v.simd_eq(gt)
            | v.simd_eq(qm)
            | v.simd_eq(at)
            | v.simd_eq(lbr)
            | v.simd_eq(bsl)
            | v.simd_eq(rbr)
            | v.simd_eq(caret)
            | v.simd_eq(pipe)
            | v.simd_eq(pct);
        let upper = v.simd_ge(ua) & v.simd_le(uz);

        if forbidden.any() {
            acc |= 1;
        }
        if upper.any() {
            acc |= 2;
        }
        if acc == 3 {
            return acc;
        } // both bits set — stop early
        i += N;
    }

    // Scalar tail
    while i < n {
        acc |= crate::unicode::DOMAIN_CHECK[s[i] as usize];
        i += 1;
    }
    acc
}
