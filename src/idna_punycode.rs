//! RFC 3492 Punycode.

use alloc::{string::String, vec::Vec};

const BASE: i32 = 36;
const TMIN: i32 = 1;
const TMAX: i32 = 26;
const SKEW: i32 = 38;
const DAMP: i32 = 700;
const INITIAL_BIAS: i32 = 72;
const INITIAL_N: u32 = 128;

#[inline]
const fn digit(byte: u8) -> Option<i32> {
    match byte {
        b'a'..=b'z' => Some((byte - b'a') as i32),
        b'0'..=b'9' => Some((byte - b'0') as i32 + 26),
        _ => None,
    }
}

#[inline]
const fn encode_digit(value: i32) -> u8 {
    if value < 26 {
        value as u8 + b'a'
    } else {
        value as u8 - 26 + b'0'
    }
}

#[inline]
const fn threshold(k: i32, bias: i32) -> i32 {
    if k <= bias {
        TMIN
    } else if k >= bias + TMAX {
        TMAX
    } else {
        k - bias
    }
}

fn adapt(mut delta: i32, count: i32, first: bool) -> i32 {
    delta = if first { delta / DAMP } else { delta / 2 };
    delta += delta / count;
    let mut k = 0;
    while delta > ((BASE - TMIN) * TMAX) / 2 {
        delta /= BASE - TMIN;
        k += BASE;
    }
    k + ((BASE - TMIN + 1) * delta) / (delta + SKEW)
}

pub(super) fn decode(input: &str) -> Option<Vec<u32>> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut suffix_start = 0;
    if let Some(delimiter) = bytes.iter().rposition(|byte| *byte == b'-') {
        if bytes[..delimiter].iter().any(|byte| *byte >= 0x80) {
            return None;
        }
        output.extend(bytes[..delimiter].iter().map(|byte| u32::from(*byte)));
        suffix_start = delimiter + 1;
    }

    let mut n = INITIAL_N;
    let mut index = 0_i32;
    let mut bias = INITIAL_BIAS;
    let mut cursor = suffix_start;
    while cursor < bytes.len() {
        let old_index = index;
        let mut weight = 1_i32;
        let mut k = BASE;
        loop {
            let value = digit(*bytes.get(cursor)?)?;
            cursor += 1;
            index = index.checked_add(value.checked_mul(weight)?)?;
            let limit = threshold(k, bias);
            if value < limit {
                break;
            }
            weight = weight.checked_mul(BASE - limit)?;
            k = k.checked_add(BASE)?;
        }
        let count = i32::try_from(output.len()).ok()?.checked_add(1)?;
        bias = adapt(index - old_index, count, old_index == 0);
        n = n.checked_add(u32::try_from(index / count).ok()?)?;
        index %= count;
        if !(0x80..=0x10ffff).contains(&n) || (0xd800..=0xdfff).contains(&n) {
            return None;
        }
        output.insert(usize::try_from(index).ok()?, n);
        index += 1;
    }
    if output.starts_with(&[
        u32::from(b'x'),
        u32::from(b'n'),
        u32::from(b'-'),
        u32::from(b'-'),
    ]) {
        return None;
    }
    Some(output)
}

pub(super) fn encode(input: &[u32], output: &mut String) -> Option<()> {
    let mut n = INITIAL_N;
    let mut delta = 0_i32;
    let mut bias = INITIAL_BIAS;
    let mut handled = 0_usize;
    for &code_point in input {
        if code_point < 0x80 {
            output.push(char::from(code_point as u8));
            handled += 1;
        }
        if code_point > 0x10ffff || (0xd800..=0xdfff).contains(&code_point) {
            return None;
        }
    }
    let basic = handled;
    if basic != 0 {
        output.push('-');
    }

    while handled < input.len() {
        let next = input
            .iter()
            .copied()
            .filter(|code_point| *code_point >= n)
            .min()?;
        let count = i32::try_from(handled).ok()?.checked_add(1)?;
        delta = delta.checked_add(
            i32::try_from(next.checked_sub(n)?)
                .ok()?
                .checked_mul(count)?,
        )?;
        n = next;
        for &code_point in input {
            if code_point < n {
                delta = delta.checked_add(1)?;
            } else if code_point == n {
                let mut value = delta;
                let mut k = BASE;
                loop {
                    let limit = threshold(k, bias);
                    if value < limit {
                        break;
                    }
                    output.push(char::from(encode_digit(
                        limit + (value - limit) % (BASE - limit),
                    )));
                    value = (value - limit) / (BASE - limit);
                    k = k.checked_add(BASE)?;
                }
                output.push(char::from(encode_digit(value)));
                bias = adapt(delta, count, handled == basic);
                delta = 0;
                handled += 1;
            }
        }
        delta = delta.checked_add(1)?;
        n = n.checked_add(1)?;
    }
    Some(())
}
