//! URL scheme type — perfect-hash detection ported from Ada C++.
//!
//! Ada uses a gperf-style minimal perfect hash:
//!   `hash = (2 * len + first_byte) & 7`
//!
//! This maps every special scheme to a unique slot in [0,7] and guarantees
//! at most **one integer comparison** per call (after the hash), with no
//! string comparison loop and no allocation.
//!
//! References:
//!  - Schmidt, "Gperf: A perfect hash function generator", More C++ Gems 2000
//!  - <https://github.com/ada-url/ada/issues/617>
//!  - `ada/include/ada/scheme-inl.h`

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(dead_code)]
pub enum SchemeType {
    Http = 0,       // hash("http")  = (2*4+104)&7 = 0
    NotSpecial = 1, // sentinel slot — never produced by the hash
    Https = 2,      // hash("https") = (2*5+104)&7 = 2
    Ws = 3,         // hash("ws")    = (2*2+119)&7 = 3
    Ftp = 4,        // hash("ftp")   = (2*3+102)&7 = 4
    Wss = 5,        // hash("wss")   = (2*3+119)&7 = 5
    File = 6,       // hash("file")  = (2*4+102)&7 = 6
}

impl SchemeType {
    #[inline]
    pub const fn is_special(self) -> bool {
        !matches!(self, SchemeType::NotSpecial)
    }

    #[inline]
    pub const fn default_port(self) -> u16 {
        // Matches Ada's `special_ports[]` indexed by the same hash value.
        const PORTS: [u16; 8] = [80, 0, 443, 80, 21, 443, 0, 0];
        PORTS[self as usize]
    }
}

// ---------------------------------------------------------------------------
// Precomputed keys — make_key packs bytes into a u64 (little-endian).
// Slot 1 and 7 are sentinels (unused by any special scheme).
// ---------------------------------------------------------------------------

/// Lengths of the scheme at each hash slot (0 = sentinel).
const SCHEME_LENGTHS: [u8; 8] = [4, 0, 5, 2, 3, 3, 4, 0];

/// `make_key(s)` packs up to 5 bytes little-endian into a u64.
/// We pre-compute these at compile time and store them here.
const SCHEME_KEYS: [u64; 8] = [
    make_key(b"http"),  // 0 → Http
    0,                  // 1 → sentinel
    make_key(b"https"), // 2 → Https
    make_key(b"ws"),    // 3 → Ws
    make_key(b"ftp"),   // 4 → Ftp
    make_key(b"wss"),   // 5 → Wss
    make_key(b"file"),  // 6 → File
    0,                  // 7 → sentinel
];

const fn make_key(s: &[u8]) -> u64 {
    let mut v: u64 = 0;
    let mut i = 0;
    while i < s.len() && i < 8 {
        v |= (s[i] as u64) << (i * 8);
        i += 1;
    }
    v
}

// ---------------------------------------------------------------------------
// Branchless load of ≤ 5 bytes into a u64 (little-endian, zero-padded).
//
// Technique: for each byte beyond position 0, compute an index that is
// either the real position (when n is large enough) or 0 (when n is too
// small), and mask the contribution to zero when n is too small.
// This avoids all branches and reads only valid memory.
//
// SAFETY: `p` must point to at least `n ≥ 1` readable bytes.
// ---------------------------------------------------------------------------
#[inline(always)]
unsafe fn branchless_load5(p: *const u8, n: usize) -> u64 {
    unsafe {
        let b0 = *p as u64;
        let b1 = (*p.add((n > 1) as usize) as u64) << 8 & (0u64.wrapping_sub((n > 1) as u64));
        let b2 = (*p.add((n > 2) as usize * 2) as u64) << 16 & (0u64.wrapping_sub((n > 2) as u64));
        let b3 = (*p.add((n > 3) as usize * 3) as u64) << 24 & (0u64.wrapping_sub((n > 3) as u64));
        let b4 = (*p.add((n > 4) as usize * 4) as u64) << 32 & (0u64.wrapping_sub((n > 4) as u64));
        b0 | b1 | b2 | b3 | b4
    }
}

// ---------------------------------------------------------------------------
// Perfect-hash scheme detection
// ---------------------------------------------------------------------------

/// Per-length mask that ORs exactly the `n` valid bytes with `0x20` (lowercasing
/// ASCII uppercase letters) while leaving the zero-padded bytes unchanged.
/// This avoids corrupting the comparison when the key has trailing zeros.
const LOWER_MASKS: [u64; 6] = [
    0,                     // n=0  (unused — caught by early return)
    0x0000_0000_0000_0020, // n=1
    0x0000_0000_0000_2020, // n=2
    0x0000_0000_0020_2020, // n=3
    0x0000_0000_2020_2020, // n=4
    0x0000_0020_2020_2020, // n=5
];

/// Classify a scheme string (without `:`) in O(1) via Ada's perfect hash.
///
/// 1. Compute `hash = (2 * len + first_byte) & 7`.
/// 2. Verify `len == SCHEME_LENGTHS[hash]` (one integer compare).
/// 3. Load up to 5 bytes branchlessly, OR a length-gated 0x20-mask to
///    fold uppercase to lowercase, then compare against `SCHEME_KEYS[hash]`.
///
/// Guarantees **at most one** data comparison per call.
#[inline]
pub fn get_scheme_type(scheme: &str) -> SchemeType {
    let b = scheme.as_bytes();
    let n = b.len();
    if n == 0 || n > 5 {
        return SchemeType::NotSpecial;
    }

    let hash = (2usize.wrapping_mul(n).wrapping_add(b[0] as usize)) & 7;
    if SCHEME_LENGTHS[hash] as usize != n {
        return SchemeType::NotSpecial;
    }

    // Branchless load + in-place lowercase (only the n valid bytes).
    // SAFETY: b.len() == n >= 1.
    let loaded = unsafe { branchless_load5(b.as_ptr(), n) } | LOWER_MASKS[n];

    if loaded == SCHEME_KEYS[hash] {
        // SAFETY: hash ∈ {0,2,3,4,5,6} all map to valid SchemeType discriminants.
        // SAFETY: hash ∈ {0,2,3,4,5,6} maps to valid SchemeType discriminants.
        unsafe { core::mem::transmute::<u8, SchemeType>(hash as u8) }
    } else {
        SchemeType::NotSpecial
    }
}

/// Like [`get_scheme_type`] but assumes input is already lowercase —
/// skips the LOWER_MASKS step.
#[inline]
#[allow(dead_code)]
pub fn get_scheme_type_lower(scheme: &str) -> SchemeType {
    let b = scheme.as_bytes();
    let n = b.len();
    if n == 0 || n > 5 {
        return SchemeType::NotSpecial;
    }
    let hash = (2usize.wrapping_mul(n).wrapping_add(b[0] as usize)) & 7;
    if SCHEME_LENGTHS[hash] as usize != n {
        return SchemeType::NotSpecial;
    }
    // SAFETY: b.len() == n >= 1.
    let loaded = unsafe { branchless_load5(b.as_ptr(), n) };
    if loaded == SCHEME_KEYS[hash] {
        unsafe { core::mem::transmute::<u8, SchemeType>(hash as u8) }
    } else {
        SchemeType::NotSpecial
    }
}

#[allow(dead_code)]
pub fn is_special(scheme: &str) -> bool {
    get_scheme_type(scheme).is_special()
}
