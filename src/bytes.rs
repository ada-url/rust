//! Small dependency-free byte-search primitives.
//!
//! The SIMD kernels mirror Ada's delimiter scanners: NEON is baseline on
//! AArch64 and SSE2 is baseline on x86-64. Other targets, Miri, and short
//! inputs use the scalar implementation.

/// Returns the index of the first matching byte.
#[inline]
pub(crate) fn find_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
    #[cfg(all(
        not(miri),
        any(
            target_arch = "aarch64",
            target_arch = "x86_64",
            all(target_arch = "x86", target_feature = "sse2")
        )
    ))]
    {
        simd::find_byte(needle, haystack)
    }

    #[cfg(not(all(
        not(miri),
        any(
            target_arch = "aarch64",
            target_arch = "x86_64",
            all(target_arch = "x86", target_feature = "sse2")
        )
    )))]
    scalar::find_byte(needle, haystack)
}

/// Returns the index of the first byte matching either needle.
#[inline]
pub(crate) fn find_byte2(first: u8, second: u8, haystack: &[u8]) -> Option<usize> {
    #[cfg(all(
        not(miri),
        any(
            target_arch = "aarch64",
            target_arch = "x86_64",
            all(target_arch = "x86", target_feature = "sse2")
        )
    ))]
    {
        simd::find_byte2(first, second, haystack)
    }

    #[cfg(not(all(
        not(miri),
        any(
            target_arch = "aarch64",
            target_arch = "x86_64",
            all(target_arch = "x86", target_feature = "sse2")
        )
    )))]
    scalar::find_byte2(first, second, haystack)
}

/// Returns the index of the first byte matching any needle.
#[inline]
pub(crate) fn find_byte3(first: u8, second: u8, third: u8, haystack: &[u8]) -> Option<usize> {
    #[cfg(all(
        not(miri),
        any(
            target_arch = "aarch64",
            target_arch = "x86_64",
            all(target_arch = "x86", target_feature = "sse2")
        )
    ))]
    {
        simd::find_byte3(first, second, third, haystack)
    }

    #[cfg(not(all(
        not(miri),
        any(
            target_arch = "aarch64",
            target_arch = "x86_64",
            all(target_arch = "x86", target_feature = "sse2")
        )
    )))]
    scalar::find_byte3(first, second, third, haystack)
}

mod scalar {
    const BYTE_ONES: u64 = u64::MAX / 0xff;
    const BYTE_HIGHS: u64 = BYTE_ONES * 0x80;

    #[inline]
    const fn contains_zero_byte(value: u64) -> bool {
        value.wrapping_sub(BYTE_ONES) & !value & BYTE_HIGHS != 0
    }

    #[inline]
    pub(super) fn find_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
        let repeated = BYTE_ONES * u64::from(needle);
        let mut chunks = haystack.chunks_exact(8);
        for (chunk_index, chunk) in chunks.by_ref().enumerate() {
            let word = u64::from_ne_bytes(chunk.try_into().expect("eight-byte chunk"));
            if contains_zero_byte(word ^ repeated)
                && let Some(offset) = chunk.iter().position(|byte| *byte == needle)
            {
                return Some(chunk_index * 8 + offset);
            }
        }
        let tail_start = haystack.len() - chunks.remainder().len();
        chunks
            .remainder()
            .iter()
            .position(|byte| *byte == needle)
            .map(|offset| tail_start + offset)
    }

    #[inline]
    pub(super) fn find_byte2(first: u8, second: u8, haystack: &[u8]) -> Option<usize> {
        let repeated_first = BYTE_ONES * u64::from(first);
        let repeated_second = BYTE_ONES * u64::from(second);
        find_chunked(haystack, |word| {
            contains_zero_byte(word ^ repeated_first) || contains_zero_byte(word ^ repeated_second)
        })
        .and_then(|start| {
            haystack[start..start + 8.min(haystack.len() - start)]
                .iter()
                .position(|byte| *byte == first || *byte == second)
                .map(|offset| start + offset)
        })
        .or_else(|| {
            let tail_start = haystack.len() / 8 * 8;
            haystack[tail_start..]
                .iter()
                .position(|byte| *byte == first || *byte == second)
                .map(|offset| tail_start + offset)
        })
    }

    #[inline]
    pub(super) fn find_byte3(first: u8, second: u8, third: u8, haystack: &[u8]) -> Option<usize> {
        let repeated_first = BYTE_ONES * u64::from(first);
        let repeated_second = BYTE_ONES * u64::from(second);
        let repeated_third = BYTE_ONES * u64::from(third);
        find_chunked(haystack, |word| {
            contains_zero_byte(word ^ repeated_first)
                || contains_zero_byte(word ^ repeated_second)
                || contains_zero_byte(word ^ repeated_third)
        })
        .and_then(|start| {
            haystack[start..start + 8.min(haystack.len() - start)]
                .iter()
                .position(|byte| *byte == first || *byte == second || *byte == third)
                .map(|offset| start + offset)
        })
        .or_else(|| {
            let tail_start = haystack.len() / 8 * 8;
            haystack[tail_start..]
                .iter()
                .position(|byte| *byte == first || *byte == second || *byte == third)
                .map(|offset| tail_start + offset)
        })
    }

    #[inline]
    fn find_chunked(haystack: &[u8], mut matches: impl FnMut(u64) -> bool) -> Option<usize> {
        haystack
            .chunks_exact(8)
            .enumerate()
            .find_map(|(index, chunk)| {
                let word = u64::from_ne_bytes(chunk.try_into().expect("eight-byte chunk"));
                matches(word).then_some(index * 8)
            })
    }
}

#[cfg(all(
    not(miri),
    any(
        target_arch = "aarch64",
        target_arch = "x86_64",
        all(target_arch = "x86", target_feature = "sse2")
    )
))]
#[allow(unsafe_code)]
mod simd {
    const LANES: usize = 16;

    #[inline]
    pub(super) fn find_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
        if haystack.len() < LANES {
            return super::scalar::find_byte(needle, haystack);
        }

        // SAFETY: this module is compiled only where its SIMD ISA is a target
        // baseline (NEON on AArch64 or SSE2 on x86-64/enabled x86).
        let splat = unsafe { architecture::splat(needle) };
        search(haystack, |pointer| {
            // SAFETY: `search` supplies a pointer to 16 readable bytes.
            unsafe { architecture::mask1(pointer, splat) }
        })
    }

    #[inline]
    pub(super) fn find_byte2(first: u8, second: u8, haystack: &[u8]) -> Option<usize> {
        if haystack.len() < LANES {
            return super::scalar::find_byte2(first, second, haystack);
        }

        // SAFETY: this module is compiled only where its SIMD ISA is a target
        // baseline (NEON on AArch64 or SSE2 on x86-64/enabled x86).
        let first = unsafe { architecture::splat(first) };
        // SAFETY: same target guarantee as above.
        let second = unsafe { architecture::splat(second) };
        search(haystack, |pointer| {
            // SAFETY: `search` supplies a pointer to 16 readable bytes.
            unsafe { architecture::mask2(pointer, first, second) }
        })
    }

    #[inline]
    pub(super) fn find_byte3(first: u8, second: u8, third: u8, haystack: &[u8]) -> Option<usize> {
        if haystack.len() < LANES {
            return super::scalar::find_byte3(first, second, third, haystack);
        }

        // SAFETY: this module is compiled only where its SIMD ISA is a target
        // baseline (NEON on AArch64 or SSE2 on x86-64/enabled x86).
        let first = unsafe { architecture::splat(first) };
        // SAFETY: same target guarantee as above.
        let second = unsafe { architecture::splat(second) };
        // SAFETY: same target guarantee as above.
        let third = unsafe { architecture::splat(third) };
        search(haystack, |pointer| {
            // SAFETY: `search` supplies a pointer to 16 readable bytes.
            unsafe { architecture::mask3(pointer, first, second, third) }
        })
    }

    #[inline]
    fn search(haystack: &[u8], mut first_match_at: impl FnMut(*const u8) -> u32) -> Option<usize> {
        let mut offset = 0;
        while offset + LANES <= haystack.len() {
            // SAFETY: the loop condition proves that 16 bytes remain.
            let lane = first_match_at(unsafe { haystack.as_ptr().add(offset) });
            if lane != 0 {
                return Some(offset + lane as usize - 1);
            }
            offset += LANES;
        }

        if offset < haystack.len() {
            let tail_start = haystack.len() - LANES;
            // SAFETY: `len >= LANES`, so the final 16-byte load is in bounds.
            let lane = first_match_at(unsafe { haystack.as_ptr().add(tail_start) });
            if lane != 0 {
                return Some(tail_start + lane as usize - 1);
            }
        }
        None
    }

    #[cfg(target_arch = "aarch64")]
    mod architecture {
        use core::arch::aarch64::{
            uint8x16_t, vceqq_u8, vdupq_n_u8, vget_lane_u64, vld1q_u8, vorrq_u8,
            vreinterpret_u64_u8, vreinterpretq_u16_u8, vshrn_n_u16,
        };

        pub(super) type Vector = uint8x16_t;

        #[inline(always)]
        pub(super) unsafe fn splat(byte: u8) -> Vector {
            // SAFETY: NEON is mandatory on AArch64.
            unsafe { vdupq_n_u8(byte) }
        }

        #[inline(always)]
        unsafe fn load(pointer: *const u8) -> Vector {
            // SAFETY: the caller guarantees 16 readable bytes at `pointer`.
            unsafe { vld1q_u8(pointer) }
        }

        #[inline(always)]
        unsafe fn bitmask(comparison: Vector) -> u32 {
            // Each matching byte is 0xff. Shift four bits out of every u16,
            // producing four consecutive set bits per matching byte. The
            // trailing-zero count therefore identifies the first byte.
            // SAFETY: NEON is mandatory on AArch64.
            let nibbles = unsafe { vshrn_n_u16::<4>(vreinterpretq_u16_u8(comparison)) };
            // SAFETY: NEON is mandatory on AArch64.
            let mask = unsafe { vget_lane_u64::<0>(vreinterpret_u64_u8(nibbles)) };
            if mask == 0 {
                0
            } else {
                (mask.trailing_zeros() >> 2) + 1
            }
        }

        #[inline(always)]
        pub(super) unsafe fn mask1(pointer: *const u8, first: Vector) -> u32 {
            // SAFETY: inherited from this function's caller.
            let input = unsafe { load(pointer) };
            // SAFETY: NEON is mandatory on AArch64.
            let comparison = unsafe { vceqq_u8(input, first) };
            // SAFETY: NEON is mandatory on AArch64.
            unsafe { bitmask(comparison) }
        }

        #[inline(always)]
        pub(super) unsafe fn mask2(pointer: *const u8, first: Vector, second: Vector) -> u32 {
            // SAFETY: inherited from this function's caller.
            let input = unsafe { load(pointer) };
            // SAFETY: NEON is mandatory on AArch64.
            let comparison = unsafe { vorrq_u8(vceqq_u8(input, first), vceqq_u8(input, second)) };
            // SAFETY: NEON is mandatory on AArch64.
            unsafe { bitmask(comparison) }
        }

        #[inline(always)]
        pub(super) unsafe fn mask3(
            pointer: *const u8,
            first: Vector,
            second: Vector,
            third: Vector,
        ) -> u32 {
            // SAFETY: inherited from this function's caller.
            let input = unsafe { load(pointer) };
            // SAFETY: NEON is mandatory on AArch64.
            let comparison = unsafe {
                vorrq_u8(
                    vorrq_u8(vceqq_u8(input, first), vceqq_u8(input, second)),
                    vceqq_u8(input, third),
                )
            };
            // SAFETY: NEON is mandatory on AArch64.
            unsafe { bitmask(comparison) }
        }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    mod architecture {
        #[cfg(target_arch = "x86")]
        use core::arch::x86::{
            __m128i, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_or_si128,
            _mm_set1_epi8,
        };
        #[cfg(target_arch = "x86_64")]
        use core::arch::x86_64::{
            __m128i, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_or_si128,
            _mm_set1_epi8,
        };

        pub(super) type Vector = __m128i;

        #[inline(always)]
        pub(super) unsafe fn splat(byte: u8) -> Vector {
            // SAFETY: SSE2 is a target baseline wherever this module compiles.
            unsafe { _mm_set1_epi8(byte as i8) }
        }

        #[inline(always)]
        unsafe fn load(pointer: *const u8) -> Vector {
            // SAFETY: the caller guarantees 16 readable bytes. The unaligned
            // intrinsic imposes no alignment requirement.
            unsafe { _mm_loadu_si128(pointer.cast::<__m128i>()) }
        }

        #[inline(always)]
        pub(super) unsafe fn mask1(pointer: *const u8, first: Vector) -> u32 {
            // SAFETY: inherited from this function's caller.
            let input = unsafe { load(pointer) };
            // SAFETY: SSE2 is a target baseline wherever this module compiles.
            let mask = unsafe { _mm_movemask_epi8(_mm_cmpeq_epi8(input, first)) as u32 };
            if mask == 0 {
                0
            } else {
                mask.trailing_zeros() + 1
            }
        }

        #[inline(always)]
        pub(super) unsafe fn mask2(pointer: *const u8, first: Vector, second: Vector) -> u32 {
            // SAFETY: inherited from this function's caller.
            let input = unsafe { load(pointer) };
            // SAFETY: SSE2 is a target baseline wherever this module compiles.
            let mask = unsafe {
                _mm_movemask_epi8(_mm_or_si128(
                    _mm_cmpeq_epi8(input, first),
                    _mm_cmpeq_epi8(input, second),
                )) as u32
            };
            if mask == 0 {
                0
            } else {
                mask.trailing_zeros() + 1
            }
        }

        #[inline(always)]
        pub(super) unsafe fn mask3(
            pointer: *const u8,
            first: Vector,
            second: Vector,
            third: Vector,
        ) -> u32 {
            // SAFETY: inherited from this function's caller.
            let input = unsafe { load(pointer) };
            // SAFETY: SSE2 is a target baseline wherever this module compiles.
            let mask = unsafe {
                _mm_movemask_epi8(_mm_or_si128(
                    _mm_or_si128(_mm_cmpeq_epi8(input, first), _mm_cmpeq_epi8(input, second)),
                    _mm_cmpeq_epi8(input, third),
                )) as u32
            };
            if mask == 0 {
                0
            } else {
                mask.trailing_zeros() + 1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{find_byte, find_byte2, find_byte3, scalar};

    #[test]
    fn finds_needles_across_vector_boundaries() {
        let input = b"0123456789abcdefghijklmnopqrstuv";
        assert_eq!(find_byte(b'g', input), Some(16));
        assert_eq!(find_byte2(b'x', b'v', input), Some(31));
        assert_eq!(find_byte3(b'x', b'h', b'v', input), Some(17));
        assert_eq!(find_byte(b'X', input), None);
        assert_eq!(find_byte2(b'X', b'Y', input), None);
        assert_eq!(find_byte3(b'X', b'Y', b'Z', input), None);
    }

    #[test]
    fn finds_needles_in_short_inputs() {
        assert_eq!(find_byte(b'c', b"abc"), Some(2));
        assert_eq!(find_byte2(b'x', b'b', b"abc"), Some(1));
        assert_eq!(find_byte3(b'x', b'y', b'a', b"abc"), Some(0));
    }

    #[test]
    fn simd_and_scalar_match_oracles_at_every_alignment() {
        let storage: Vec<u8> = (0..192)
            .map(|index| (index * 37 + index / 7 * 11) as u8)
            .collect();
        // Miri exercises the scalar fallback; a smaller matrix keeps the
        // interpreted run practical. Native tests retain all alignments and
        // cross several SIMD-vector boundaries.
        let alignment_count = if cfg!(miri) { 4 } else { 32 };
        let maximum_length = if cfg!(miri) { 40 } else { 128 };

        for start in 0..alignment_count {
            for length in 0..=maximum_length {
                let input = &storage[start..start + length];
                for needle in [0, 1, 9, 0x7f, 0x80, 0xfe, 0xff] {
                    let expected = input.iter().position(|byte| *byte == needle);
                    assert_eq!(scalar::find_byte(needle, input), expected);
                    assert_eq!(find_byte(needle, input), expected);
                }

                let expected2 = input.iter().position(|byte| matches!(*byte, 1 | 0xff));
                assert_eq!(scalar::find_byte2(1, 0xff, input), expected2);
                assert_eq!(find_byte2(1, 0xff, input), expected2);

                let expected3 = input
                    .iter()
                    .position(|byte| matches!(*byte, 0 | 0x80 | 0xff));
                assert_eq!(scalar::find_byte3(0, 0x80, 0xff, input), expected3);
                assert_eq!(find_byte3(0, 0x80, 0xff, input), expected3);
            }
        }
    }
}
