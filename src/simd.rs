//! SIMD and SWAR (SIMD Within A Register) optimisations for hot scanning paths.
//!
//! Enabled by the `simd` cargo feature.
//!
//! Dispatch order (best → worst):
//!   1. x86_64 SSSE3  – 16 bytes/iter, runtime-detected
//!   2. aarch64 NEON  – 16 bytes/iter, compile-time
//!   3. SWAR (scalar) – 8 bytes/iter using `u64` arithmetic, all platforms
//!
//! The PSHUFB / VTBL character-classification technique (same as Ada C++):
//!   split byte into low nibble (bits 0-3) and high nibble (bits 4-7),
//!   look each up in a 16-byte table, AND results; non-zero → byte is in set.
// ============================================================
// SWAR helpers (always available, 100% safe)
// ============================================================

#[inline(always)]
pub(crate) const fn broadcast(v: u8) -> u64 {
    (v as u64).wrapping_mul(0x0101_0101_0101_0101)
}

#[inline(always)]
pub(crate) const fn has_zero_byte(w: u64) -> u64 {
    w.wrapping_sub(0x0101_0101_0101_0101) & !w & 0x8080_8080_8080_8080
}

/// Load 8 bytes unaligned from `ptr` as a little-endian u64.
#[inline(always)]
unsafe fn load8(ptr: *const u8) -> u64 {
    unsafe {
        let mut w = 0u64;
        core::ptr::copy_nonoverlapping(ptr, &mut w as *mut u64 as *mut u8, 8);
        w
    }
}

// ============================================================
// to_lower_ascii  (SWAR — Ada's `ascii_map` formula, 8 bytes/iter)
// ============================================================

/// ASCII-lowercase `buf` in-place. Returns `true` iff every byte is ASCII.
///
/// Uses Ada C++'s SWAR `ascii_map` formula:
/// `word ^= (((word + AP) ^ (word + ZP)) & 0x80…) >> 2`
#[inline]
pub fn to_lower_ascii(buf: &mut [u8]) -> bool {
    const M80: u64 = 0x8080_8080_8080_8080;
    const AP: u64 = broadcast(128 - b'A'); // 63 repeated
    const ZP: u64 = broadcast(128 - b'Z' - 1); // 37 repeated

    let n = buf.len();
    let ptr = buf.as_mut_ptr();
    let mut non_ascii: u64 = 0;
    let mut i = 0usize;

    while i + 8 <= n {
        let mut w = unsafe { load8(ptr.add(i)) };
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
// 256-byte lookup tables (Ada style)
// ============================================================

#[allow(dead_code)]
pub static FORBIDDEN_HOST_TABLE: [bool; 256] = {
    let mut t = [false; 256];
    let bytes: &[u8] = &[
        0, 9, 10, 13, 32, 35, 47, 58, 60, 62, 63, 64, 91, 92, 93, 94, 124,
    ];
    let mut i = 0;
    while i < bytes.len() {
        t[bytes[i] as usize] = true;
        i += 1;
    }
    t
};

#[allow(dead_code)]
pub static FORBIDDEN_DOMAIN_TABLE: [bool; 256] = {
    let mut t = [false; 256];
    let bytes: &[u8] = &[
        0, 9, 10, 13, 32, 35, 47, 58, 60, 62, 63, 64, 91, 92, 93, 94, 124, 37,
    ];
    let mut i = 0;
    while i < bytes.len() {
        t[bytes[i] as usize] = true;
        i += 1;
    }
    let mut c = 0usize;
    while c <= 32 {
        t[c] = true;
        c += 1;
    }
    let mut c = 127usize;
    while c < 256 {
        t[c] = true;
        c += 1;
    }
    t
};

// ============================================================
// PSHUFB nibble-classification tables
// ============================================================

/// low/high nibble masks for: `@`(40), `/`(2F), `\`(5C), `?`(3F)
#[allow(dead_code)]
const AUTHORITY_DELIM_SPECIAL_LOW: [u8; 16] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x0A,
];
#[allow(dead_code)]
const AUTHORITY_DELIM_SPECIAL_HIGH: [u8; 16] = [
    0x00, 0x00, 0x02, 0x08, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// low/high nibble masks for: `@`(40), `/`(2F), `?`(3F)
#[allow(dead_code)]
const AUTHORITY_DELIM_LOW: [u8; 16] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0A,
];
#[allow(dead_code)]
const AUTHORITY_DELIM_HIGH: [u8; 16] = [
    0x00, 0x00, 0x02, 0x08, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// low/high nibble masks for: `:`(3A), `/`(2F), `\`(5C), `?`(3F), `[`(5B)
/// (Ada C++ tables for find_next_host_delimiter_special)
#[allow(dead_code)]
const HOST_DELIM_SPECIAL_LOW: [u8; 16] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x04, 0x04, 0x00, 0x00, 0x03,
];
#[allow(dead_code)]
const HOST_DELIM_SPECIAL_HIGH: [u8; 16] = [
    0x00, 0x00, 0x02, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// low/high nibble masks for: `:`(3A), `/`(2F), `?`(3F), `[`(5B)
/// (Ada C++ tables for find_next_host_delimiter)
#[allow(dead_code)]
const HOST_DELIM_LOW: [u8; 16] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x04, 0x00, 0x00, 0x00, 0x03,
];
#[allow(dead_code)]
const HOST_DELIM_HIGH: [u8; 16] = [
    0x00, 0x00, 0x02, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// ============================================================
// SWAR fallbacks for delimiter search (8 bytes/iter)
// ============================================================

#[inline]
fn swar_find4(b: &[u8], d0: u8, d1: u8, d2: u8, d3: u8) -> usize {
    let m0 = broadcast(d0);
    let m1 = broadcast(d1);
    let m2 = broadcast(d2);
    let m3 = broadcast(d3);
    let n = b.len();
    let mut i = 0;
    while i + 8 <= n {
        let w = unsafe { load8(b.as_ptr().add(i)) };
        if has_zero_byte(w ^ m0)
            | has_zero_byte(w ^ m1)
            | has_zero_byte(w ^ m2)
            | has_zero_byte(w ^ m3)
            != 0
        {
            for j in 0..8 {
                if b[i + j] == d0 || b[i + j] == d1 || b[i + j] == d2 || b[i + j] == d3 {
                    return i + j;
                }
            }
        }
        i += 8;
    }
    for (k, &c) in b[i..].iter().enumerate() {
        if c == d0 || c == d1 || c == d2 || c == d3 {
            return i + k;
        }
    }
    n
}

#[inline]
fn swar_find3(b: &[u8], d0: u8, d1: u8, d2: u8) -> usize {
    let m0 = broadcast(d0);
    let m1 = broadcast(d1);
    let m2 = broadcast(d2);
    let n = b.len();
    let mut i = 0;
    while i + 8 <= n {
        let w = unsafe { load8(b.as_ptr().add(i)) };
        if has_zero_byte(w ^ m0) | has_zero_byte(w ^ m1) | has_zero_byte(w ^ m2) != 0 {
            for j in 0..8 {
                if b[i + j] == d0 || b[i + j] == d1 || b[i + j] == d2 {
                    return i + j;
                }
            }
        }
        i += 8;
    }
    for (k, &c) in b[i..].iter().enumerate() {
        if c == d0 || c == d1 || c == d2 {
            return i + k;
        }
    }
    n
}

#[inline]
fn swar_find5(b: &[u8], d0: u8, d1: u8, d2: u8, d3: u8, d4: u8) -> usize {
    let m0 = broadcast(d0);
    let m1 = broadcast(d1);
    let m2 = broadcast(d2);
    let m3 = broadcast(d3);
    let m4 = broadcast(d4);
    let n = b.len();
    let mut i = 0;
    while i + 8 <= n {
        let w = unsafe { load8(b.as_ptr().add(i)) };
        let h = has_zero_byte(w ^ m0)
            | has_zero_byte(w ^ m1)
            | has_zero_byte(w ^ m2)
            | has_zero_byte(w ^ m3)
            | has_zero_byte(w ^ m4);
        if h != 0 {
            for j in 0..8 {
                if b[i + j] == d0
                    || b[i + j] == d1
                    || b[i + j] == d2
                    || b[i + j] == d3
                    || b[i + j] == d4
                {
                    return i + j;
                }
            }
        }
        i += 8;
    }
    for (k, &c) in b[i..].iter().enumerate() {
        if c == d0 || c == d1 || c == d2 || c == d3 || c == d4 {
            return i + k;
        }
    }
    n
}

#[inline(always)]
#[allow(dead_code)]
fn swar_has_tabs_or_newline(b: &[u8]) -> bool {
    let cr = broadcast(b'\r');
    let lf = broadcast(b'\n');
    let ht = broadcast(b'\t');
    let n = b.len();
    let mut running: u64 = 0;
    let mut i = 0;
    while i + 8 <= n {
        let w = unsafe { load8(b.as_ptr().add(i)) };
        running |= has_zero_byte(w ^ cr) | has_zero_byte(w ^ lf) | has_zero_byte(w ^ ht);
        i += 8;
    }
    if running != 0 {
        return true;
    }
    if i < n {
        let mut w: u64 = 0;
        unsafe {
            core::ptr::copy_nonoverlapping(b.as_ptr().add(i), &mut w as *mut u64 as *mut u8, n - i)
        };
        running |= has_zero_byte(w ^ cr) | has_zero_byte(w ^ lf) | has_zero_byte(w ^ ht);
    }
    running != 0
}

// SWAR nibble classify: find first byte matching the low/high table pair
#[inline]
#[allow(dead_code)]
fn swar_classify(b: &[u8], low: &[u8; 16], high: &[u8; 16]) -> usize {
    for (i, &c) in b.iter().enumerate() {
        if low[(c & 0xF) as usize] & high[(c >> 4) as usize] != 0 {
            return i;
        }
    }
    b.len()
}


// ============================================================
// Cached CPU-feature detection
// ============================================================

/// Cache the SSSE3 detection result so we pay the atomic-load overhead
/// only once per call-site, not per-call.  Returns `false` on non-x86_64
/// or when SSSE3 is unavailable.
///
/// The threshold of 16 bytes is also enforced here: SSSE3 processes data in
/// 16-byte lanes; for shorter inputs it never beats SWAR, so we skip the
/// detection check entirely.
#[cfg(all(feature = "std", target_arch = "x86_64"))]
#[inline(always)]
fn ssse3_available_for(len: usize) -> bool {
    if len < 16 {
        return false; // SSSE3 has no benefit for < 16 bytes
    }
    // Rust's is_x86_feature_detected! caches the CPUID result in a global
    // AtomicUsize, so this is just a relaxed load + bit test after the first
    // call (~3 cycles vs ~100+ for CPUID).
    is_x86_feature_detected!("ssse3")
}

// ============================================================
// Public API — dispatch to best available implementation
// ============================================================

#[inline]
pub fn has_tabs_or_newline(b: &[u8]) -> bool {
    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    if ssse3_available_for(b.len()) {
        return unsafe { x86::has_tabs_or_newline_ssse3(b) };
    }
    #[cfg(target_arch = "aarch64")]
    { return unsafe { neon::has_tabs_or_newline_neon(b) }; }
    swar_has_tabs_or_newline(b)
}

#[inline]
pub fn find_authority_delimiter_special(view: &str) -> usize {
    let b = view.as_bytes();
    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    if ssse3_available_for(b.len()) {
        return unsafe { x86::find_delim_ssse3(b, &AUTHORITY_DELIM_SPECIAL_LOW, &AUTHORITY_DELIM_SPECIAL_HIGH) };
    }
    #[cfg(target_arch = "aarch64")]
    { return unsafe { neon::find_delim_neon(b, &AUTHORITY_DELIM_SPECIAL_LOW, &AUTHORITY_DELIM_SPECIAL_HIGH) }; }
    swar_find4(b, b'@', b'/', b'\\', b'?')
}

#[inline]
pub fn find_authority_delimiter(view: &str) -> usize {
    let b = view.as_bytes();
    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    if ssse3_available_for(b.len()) {
        return unsafe { x86::find_delim_ssse3(b, &AUTHORITY_DELIM_LOW, &AUTHORITY_DELIM_HIGH) };
    }
    #[cfg(target_arch = "aarch64")]
    { return unsafe { neon::find_delim_neon(b, &AUTHORITY_DELIM_LOW, &AUTHORITY_DELIM_HIGH) }; }
    swar_find3(b, b'@', b'/', b'?')
}

#[inline]
pub fn find_next_host_delimiter_special(view: &str, from: usize) -> usize {
    let b = &view.as_bytes()[from..];
    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    if ssse3_available_for(b.len()) {
        return from + unsafe { x86::find_delim_ssse3(b, &HOST_DELIM_SPECIAL_LOW, &HOST_DELIM_SPECIAL_HIGH) };
    }
    #[cfg(target_arch = "aarch64")]
    { return from + unsafe { neon::find_delim_neon(b, &HOST_DELIM_SPECIAL_LOW, &HOST_DELIM_SPECIAL_HIGH) }; }
    from + swar_find5(b, b':', b'/', b'\\', b'?', b'[')
}

#[inline]
pub fn find_next_host_delimiter(view: &str, from: usize) -> usize {
    let b = &view.as_bytes()[from..];
    #[cfg(all(feature = "std", target_arch = "x86_64"))]
    if ssse3_available_for(b.len()) {
        return from + unsafe { x86::find_delim_ssse3(b, &HOST_DELIM_LOW, &HOST_DELIM_HIGH) };
    }
    #[cfg(target_arch = "aarch64")]
    { return from + unsafe { neon::find_delim_neon(b, &HOST_DELIM_LOW, &HOST_DELIM_HIGH) }; }
    from + swar_find4(b, b':', b'/', b'?', b'[')
}

// ============================================================
// x86_64 SSSE3 implementations (require runtime detection above)
// ============================================================

#[cfg(all(feature = "std", target_arch = "x86_64"))]
mod x86 {
    /// PSHUFB-based delimiter finding — exact port of Ada C++.
    ///
    /// # Safety
    /// Caller must ensure SSSE3 is available (use `is_x86_feature_detected!`).
    #[target_feature(enable = "ssse3")]
    pub(super) unsafe fn find_delim_ssse3(b: &[u8], low: &[u8; 16], high: &[u8; 16]) -> usize {
        unsafe {
            use core::arch::x86_64::*;
            let n = b.len();
            let ptr = b.as_ptr();
            let low_m = _mm_loadu_si128(low.as_ptr() as *const __m128i);
            let high_m = _mm_loadu_si128(high.as_ptr() as *const __m128i);
            let fmask = _mm_set1_epi8(0x0f);
            let zero = _mm_setzero_si128();
            let mut i = 0usize;

            while i + 16 <= n {
                let w = _mm_loadu_si128(ptr.add(i) as *const __m128i);
                let lo = _mm_and_si128(w, fmask);
                let hi = _mm_and_si128(_mm_srli_epi16(w, 4), fmask);
                let cl = _mm_and_si128(_mm_shuffle_epi8(low_m, lo), _mm_shuffle_epi8(high_m, hi));
                let mask = !_mm_movemask_epi8(_mm_cmpeq_epi8(cl, zero)) & 0xFFFF;
                if mask != 0 {
                    return i + (mask as u32).trailing_zeros() as usize;
                }
                i += 16;
            }
            // Overlapping tail — only when we have 16+ bytes total
            if n >= 16 && i < n {
                let tail = n - 16;
                let w = _mm_loadu_si128(ptr.add(tail) as *const __m128i);
                let lo = _mm_and_si128(w, fmask);
                let hi = _mm_and_si128(_mm_srli_epi16(w, 4), fmask);
                let cl = _mm_and_si128(_mm_shuffle_epi8(low_m, lo), _mm_shuffle_epi8(high_m, hi));
                let mask = !_mm_movemask_epi8(_mm_cmpeq_epi8(cl, zero)) & 0xFFFF;
                if mask != 0 {
                    let pos = tail + (mask as u32).trailing_zeros() as usize;
                    if pos >= i {
                        return pos;
                    }
                }
            }
            // Scalar remainder (short tail or short input)
            for (k, &c) in b[i..].iter().enumerate() {
                if low[(c & 0xF) as usize] & high[(c >> 4) as usize] != 0 {
                    return i + k;
                }
            }
            n
        }
    }

    /// SSSE3 `has_tabs_or_newline` — Ada C++ PSHUFB table approach.
    ///
    /// # Safety
    /// Caller must ensure SSSE3 is available.
    #[target_feature(enable = "ssse3")]
    pub(super) unsafe fn has_tabs_or_newline_ssse3(b: &[u8]) -> bool {
        unsafe {
            use core::arch::x86_64::*;
            // Table: pos 9→9 (HT), pos 10→10 (LF), pos 13→13 (CR), else→1
            let rnt = _mm_setr_epi8(1, 0, 0, 0, 0, 0, 0, 0, 0, 9, 10, 0, 0, 13, 0, 0);
            let n = b.len();
            let ptr = b.as_ptr();
            let mut running = _mm_setzero_si128();
            let mut i = 0usize;
            while i + 16 <= n {
                let w = _mm_loadu_si128(ptr.add(i) as *const __m128i);
                running = _mm_or_si128(running, _mm_cmpeq_epi8(_mm_shuffle_epi8(rnt, w), w));
                i += 16;
            }
            if _mm_movemask_epi8(running) != 0 {
                return true;
            }
            b[i..]
                .iter()
                .any(|&c| c == b'\t' || c == b'\n' || c == b'\r')
        }
    }
}

// ============================================================
// aarch64 NEON implementations
// ============================================================

#[cfg(all(feature = "std", target_arch = "aarch64"))]
mod neon {
    /// VTBL-based delimiter finding.
    ///
    /// # Safety
    /// Caller must ensure NEON is available (it is on all AArch64 targets).
    #[target_feature(enable = "neon")]
    pub(super) unsafe fn find_delim_neon(b: &[u8], low: &[u8; 16], high: &[u8; 16]) -> usize {
        use core::arch::aarch64::*;
        let n = b.len();
        let ptr = b.as_ptr();
        let low_m = vld1q_u8(low.as_ptr());
        let high_m = vld1q_u8(high.as_ptr());
        let fmask = vdupq_n_u8(0x0f);
        let mut i = 0usize;

        while i + 16 <= n {
            let w = vld1q_u8(ptr.add(i));
            let lo = vandq_u8(w, fmask);
            let hi = vshrq_n_u8(w, 4);
            let cl = vandq_u8(vqtbl1q_u8(low_m, lo), vqtbl1q_u8(high_m, hi));
            if vmaxvq_u8(cl) != 0 {
                let tmp: [u8; 16] = core::mem::transmute(cl);
                for (k, &v) in tmp.iter().enumerate() {
                    if v != 0 {
                        return i + k;
                    }
                }
            }
            i += 16;
        }
        if i < n {
            let tail = n - 16;
            let w = vld1q_u8(ptr.add(tail));
            let lo = vandq_u8(w, fmask);
            let hi = vshrq_n_u8(w, 4);
            let cl = vandq_u8(vqtbl1q_u8(low_m, lo), vqtbl1q_u8(high_m, hi));
            if vmaxvq_u8(cl) != 0 {
                let tmp: [u8; 16] = core::mem::transmute(cl);
                for (k, &v) in tmp.iter().enumerate() {
                    if v != 0 && tail + k >= i {
                        return tail + k;
                    }
                }
            }
        }
        n
    }

    #[target_feature(enable = "neon")]
    pub(super) unsafe fn has_tabs_or_newline_neon(b: &[u8]) -> bool {
        use core::arch::aarch64::*;
        let tbl: [u8; 16] = [1, 0, 0, 0, 0, 0, 0, 0, 0, 9, 10, 0, 0, 13, 0, 0];
        let rnt = vld1q_u8(tbl.as_ptr());
        let n = b.len();
        let ptr = b.as_ptr();
        let mut running = vdupq_n_u8(0);
        let mut i = 0usize;
        while i + 16 <= n {
            let w = vld1q_u8(ptr.add(i));
            running = vorrq_u8(running, vceqq_u8(vqtbl1q_u8(rnt, w), w));
            i += 16;
        }
        if vmaxvq_u8(running) != 0 {
            return true;
        }
        b[i..]
            .iter()
            .any(|&c| c == b'\t' || c == b'\n' || c == b'\r')
    }
}
