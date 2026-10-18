//! Compact Unicode 17 tables generated from Ada 4.0.0.

const DATA: &[u8; 224_608] = include_bytes!("idna_tables.bin");

const OFF_IDNA_STAGE1: usize = 0;
const OFF_IDNA_STAGE2: usize = 6_564;
const OFF_IDNA_BOOL_BLOCKS: usize = 29_480;
const OFF_IDNA_UTF8_MAPPINGS: usize = 31_280;
const IDNA_UTF8_MAPPINGS_LEN: usize = 17_383;
const OFF_DECOMPOSITION_INDEX: usize = 48_663;
const OFF_DECOMPOSITION_BLOCK: usize = 53_016;
const OFF_DECOMPOSITION_DATA: usize = 87_456;
const DECOMPOSITION_DATA_LEN: usize = 9_102;
const OFF_CCC_INDEX: usize = 123_864;
const OFF_CCC_BLOCK: usize = 128_216;
const OFF_COMPOSITION_INDEX: usize = 145_368;
const OFF_COMPOSITION_BLOCK: usize = 149_720;
const OFF_COMPOSITION_DATA: usize = 184_160;
const COMPOSITION_DATA_LEN: usize = 1_883;
const OFF_DIR_START: usize = 209_244;
const OFF_DIR_FINAL: usize = 215_040;
const OFF_DIR_VALUE: usize = 220_836;
const DIR_COUNT: usize = 1_449;
const OFF_COMBINING_RANGES: usize = 222_288;
const COMBINING_RANGE_COUNT: usize = 290;

const DECOMPOSITION_BLOCK_COLS: usize = 257;
const CCC_BLOCK_COLS: usize = 256;
const COMPOSITION_BLOCK_COLS: usize = 257;

const IDNA_VALID: u16 = 0xffff;
const IDNA_DISALLOWED: u16 = 0xfffe;
const IDNA_BOOL_FLAG: u16 = 0x8000;
const IDNA_LOW_RANGE_END: u32 = 0x33480;
const IDNA_HIGH_IGNORED_START: u32 = 0xe0100;
const IDNA_HIGH_IGNORED_END: u32 = 0xe01f0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Mapping {
    Valid,
    Disallowed,
    Bytes(usize),
}

#[inline]
pub(super) fn mapping(code_point: u32) -> Mapping {
    let status = if code_point < IDNA_LOW_RANGE_END {
        let block = read_u16(OFF_IDNA_STAGE1, (code_point >> 6) as usize);
        if block & IDNA_BOOL_FLAG != 0 {
            let bit = usize::from(block & !IDNA_BOOL_FLAG) * 64 + (code_point as usize & 63);
            if read_u64(OFF_IDNA_BOOL_BLOCKS, bit >> 6) & (1_u64 << (bit & 63)) != 0 {
                IDNA_VALID
            } else {
                IDNA_DISALLOWED
            }
        } else {
            read_u16(
                OFF_IDNA_STAGE2,
                usize::from(block) + (code_point as usize & 63),
            )
        }
    } else if (IDNA_HIGH_IGNORED_START..IDNA_HIGH_IGNORED_END).contains(&code_point) {
        0
    } else {
        IDNA_DISALLOWED
    };

    match status {
        IDNA_VALID => Mapping::Valid,
        IDNA_DISALLOWED => Mapping::Disallowed,
        offset => Mapping::Bytes(usize::from(offset)),
    }
}

#[inline]
pub(super) fn mapping_bytes(offset: usize) -> Option<&'static [u8]> {
    if offset >= IDNA_UTF8_MAPPINGS_LEN {
        return None;
    }
    let start = OFF_IDNA_UTF8_MAPPINGS + offset;
    let tail = DATA.get(start..OFF_IDNA_UTF8_MAPPINGS + IDNA_UTF8_MAPPINGS_LEN)?;
    let length = tail.iter().position(|byte| *byte == 0)?;
    Some(&tail[..length])
}

#[inline]
pub(super) fn decomposition(code_point: u32) -> Option<&'static [u8]> {
    if code_point >= 0x110000 {
        return None;
    }
    let row = usize::from(DATA[OFF_DECOMPOSITION_INDEX + (code_point >> 8) as usize]);
    let cell = row * DECOMPOSITION_BLOCK_COLS + (code_point as usize & 0xff);
    let first = read_u16(OFF_DECOMPOSITION_BLOCK, cell);
    let second = read_u16(OFF_DECOMPOSITION_BLOCK, cell + 1);
    let length = usize::from((second >> 2).saturating_sub(first >> 2));
    if length == 0 || first & 1 != 0 {
        return None;
    }
    let start = usize::from(first >> 2);
    (start + length <= DECOMPOSITION_DATA_LEN).then(|| {
        let byte_start = OFF_DECOMPOSITION_DATA + start * 4;
        &DATA[byte_start..byte_start + length * 4]
    })
}

#[inline]
pub(super) fn combining_class(code_point: u32) -> u8 {
    if code_point >= 0x110000 {
        return 0;
    }
    let row = usize::from(DATA[OFF_CCC_INDEX + (code_point >> 8) as usize]);
    DATA[OFF_CCC_BLOCK + row * CCC_BLOCK_COLS + (code_point as usize & 0xff)]
}

#[inline]
pub(super) fn composition_bounds(starter: u32) -> (usize, usize) {
    if starter >= 0x110000 {
        return (0, 0);
    }
    let row = usize::from(DATA[OFF_COMPOSITION_INDEX + (starter >> 8) as usize]);
    let cell = row * COMPOSITION_BLOCK_COLS + (starter as usize & 0xff);
    (
        usize::from(read_u16(OFF_COMPOSITION_BLOCK, cell)),
        usize::from(read_u16(OFF_COMPOSITION_BLOCK, cell + 1)),
    )
}

pub(super) fn compose(starter: u32, combining: u32) -> Option<u32> {
    let (mut left, mut right) = composition_bounds(starter);
    if left == right || right > COMPOSITION_DATA_LEN {
        return None;
    }
    while left + 2 < right {
        let middle = left + (((right - left) >> 1) & !1);
        match composition_value(middle).cmp(&combining) {
            core::cmp::Ordering::Less => left = middle,
            core::cmp::Ordering::Greater => right = middle,
            core::cmp::Ordering::Equal => return Some(composition_value(middle + 1)),
        }
    }
    (left + 1 < COMPOSITION_DATA_LEN && composition_value(left) == combining)
        .then(|| composition_value(left + 1))
}

#[inline]
pub(super) fn decomposition_code_point(bytes: &[u8], index: usize) -> u32 {
    read_u32_slice(bytes, index)
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::upper_case_acronyms)]
pub(super) enum Direction {
    None,
    BN,
    CS,
    ES,
    ON,
    EN,
    L,
    R,
    NSM,
    AL,
    AN,
    ET,
    WS,
    RLO,
    LRO,
    PDF,
    RLE,
    RLI,
    FSI,
    PDI,
    LRI,
    B,
    S,
    LRE,
}

pub(super) fn direction(code_point: u32) -> Direction {
    let mut low = 0;
    let mut high = DIR_COUNT;
    while low < high {
        let middle = low + (high - low) / 2;
        if read_u32(OFF_DIR_FINAL, middle) < code_point {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    if low == DIR_COUNT || code_point < read_u32(OFF_DIR_START, low) {
        return Direction::None;
    }
    match DATA[OFF_DIR_VALUE + low] {
        1 => Direction::BN,
        2 => Direction::CS,
        3 => Direction::ES,
        4 => Direction::ON,
        5 => Direction::EN,
        6 => Direction::L,
        7 => Direction::R,
        8 => Direction::NSM,
        9 => Direction::AL,
        10 => Direction::AN,
        11 => Direction::ET,
        12 => Direction::WS,
        13 => Direction::RLO,
        14 => Direction::LRO,
        15 => Direction::PDF,
        16 => Direction::RLE,
        17 => Direction::RLI,
        18 => Direction::FSI,
        19 => Direction::PDI,
        20 => Direction::LRI,
        21 => Direction::B,
        22 => Direction::S,
        23 => Direction::LRE,
        _ => Direction::None,
    }
}

pub(super) fn is_combining_mark(code_point: u32) -> bool {
    let mut low = 0;
    let mut high = COMBINING_RANGE_COUNT;
    while low < high {
        let middle = low + (high - low) / 2;
        if range_end(OFF_COMBINING_RANGES, middle) < code_point {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low < COMBINING_RANGE_COUNT && code_point >= range_start(OFF_COMBINING_RANGES, low)
}

#[inline]
fn composition_value(index: usize) -> u32 {
    read_u32(OFF_COMPOSITION_DATA, index)
}

#[inline]
fn range_start(offset: usize, index: usize) -> u32 {
    read_u32(offset, index * 2)
}

#[inline]
fn range_end(offset: usize, index: usize) -> u32 {
    read_u32(offset, index * 2 + 1)
}

#[inline]
fn read_u16(offset: usize, index: usize) -> u16 {
    let position = offset + index * 2;
    u16::from_le_bytes([DATA[position], DATA[position + 1]])
}

#[inline]
fn read_u32(offset: usize, index: usize) -> u32 {
    read_u32_slice(DATA, offset / 4 + index)
}

#[inline]
fn read_u32_slice(bytes: &[u8], index: usize) -> u32 {
    let position = index * 4;
    u32::from_le_bytes([
        bytes[position],
        bytes[position + 1],
        bytes[position + 2],
        bytes[position + 3],
    ])
}

#[inline]
fn read_u64(offset: usize, index: usize) -> u64 {
    let position = offset + index * 8;
    u64::from_le_bytes([
        DATA[position],
        DATA[position + 1],
        DATA[position + 2],
        DATA[position + 3],
        DATA[position + 4],
        DATA[position + 5],
        DATA[position + 6],
        DATA[position + 7],
    ])
}
