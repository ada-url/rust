//! Dependency-free UTS #46 domain conversion.

use alloc::{string::String, vec::Vec};
use core::fmt;

#[path = "idna_joiners.rs"]
mod joiners;
#[path = "idna_normalization.rs"]
mod normalization;
#[path = "idna_punycode.rs"]
mod punycode;
#[path = "idna_tables.rs"]
mod tables;

use joiners::{JOINING_DUAL, JOINING_LEFT, JOINING_RIGHT, VIRAMA};
use tables::Direction;

const MAX_DOMAIN_INPUT_BYTES: usize = 16_384;

/// An error produced during UTS #46 processing.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct IdnaError;

impl fmt::Display for IdnaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid internationalized domain name")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for IdnaError {}

/// UTS #46 conversion helpers retained for API compatibility.
pub struct Idna;

impl Idna {
    /// Converts an ASCII/Punycode domain to Unicode.
    #[must_use]
    pub fn unicode(input: &str) -> String {
        domain_to_unicode(input).0
    }

    /// Converts a Unicode domain to ASCII, returning an empty string on error.
    #[must_use]
    pub fn ascii(input: &str) -> String {
        domain_to_ascii(input).unwrap_or_default()
    }
}

/// Converts a Unicode domain to its ASCII form using non-transitional UTS #46.
pub fn domain_to_ascii(domain: &str) -> Result<String, IdnaError> {
    if domain.len() > MAX_DOMAIN_INPUT_BYTES {
        return Err(IdnaError);
    }
    if domain.is_ascii() {
        if contains_forbidden_domain_byte(domain.as_bytes()) {
            return Err(IdnaError);
        }
        return Ok(domain.to_ascii_lowercase());
    }

    let input: Vec<u32> = domain.chars().map(u32::from).collect();
    let mut mapped = map_code_points(&input)?;
    if !normalization::is_nfc(&mapped) {
        normalization::normalize(&mut mapped);
    }

    let mut output = String::with_capacity(domain.len() + 8);
    let mut label_start = 0;
    loop {
        let label_end = mapped[label_start..]
            .iter()
            .position(|code_point| *code_point == u32::from(b'.'))
            .map_or(mapped.len(), |relative| label_start + relative);
        let label = &mapped[label_start..label_end];
        append_ascii_label(label, &mut output)?;
        if label_end == mapped.len() {
            break;
        }
        output.push('.');
        label_start = label_end + 1;
    }

    if contains_forbidden_domain_byte(output.as_bytes()) {
        return Err(IdnaError);
    }
    Ok(output)
}

/// Converts an ASCII/Punycode domain to Unicode.
pub fn domain_to_unicode(domain: &str) -> (String, Result<(), IdnaError>) {
    if domain.len() > MAX_DOMAIN_INPUT_BYTES {
        return (String::from(domain), Err(IdnaError));
    }
    let mut output = String::with_capacity(domain.len());
    for (index, label) in domain.split('.').enumerate() {
        if index != 0 {
            output.push('.');
        }
        let Some(punycode) = label.strip_prefix("xn--").filter(|_| label.is_ascii()) else {
            output.push_str(label);
            continue;
        };
        let Some(decoded) = punycode::decode(punycode) else {
            output.push_str(label);
            continue;
        };
        let accepted = !decoded.iter().all(|code_point| *code_point < 0x80)
            && map_code_points(&decoded).is_ok_and(|mapped| mapped == decoded)
            && normalization::is_nfc(&decoded)
            && is_label_valid(&decoded);
        if accepted {
            for code_point in decoded {
                if let Some(character) = char::from_u32(code_point) {
                    output.push(character);
                }
            }
        } else {
            output.push_str(label);
        }
    }
    (output, Ok(()))
}

fn append_ascii_label(label: &[u32], output: &mut String) -> Result<(), IdnaError> {
    if label.is_empty() {
        return Ok(());
    }
    if label.starts_with(&[
        u32::from(b'x'),
        u32::from(b'n'),
        u32::from(b'-'),
        u32::from(b'-'),
    ]) {
        if !label.iter().all(|code_point| *code_point < 0x80) {
            return Err(IdnaError);
        }
        let ascii_start = output.len();
        for &code_point in label {
            output.push(char::from(code_point as u8));
        }
        let decoded = punycode::decode(&output[ascii_start + 4..]).ok_or(IdnaError)?;
        if decoded.iter().all(|code_point| *code_point < 0x80)
            || map_code_points(&decoded)? != decoded
            || !normalization::is_nfc(&decoded)
            || !is_label_valid(&decoded)
        {
            return Err(IdnaError);
        }
    } else if label.iter().all(|code_point| *code_point < 0x80) {
        for &code_point in label {
            output.push(char::from(code_point as u8));
        }
    } else {
        if !is_label_valid(label) {
            return Err(IdnaError);
        }
        output.push_str("xn--");
        punycode::encode(label, output).ok_or(IdnaError)?;
    }
    Ok(())
}

fn map_code_points(input: &[u32]) -> Result<Vec<u32>, IdnaError> {
    let mut length = 0;
    for &code_point in input {
        match tables::mapping(code_point) {
            tables::Mapping::Valid => length += 1,
            tables::Mapping::Disallowed => return Err(IdnaError),
            tables::Mapping::Bytes(offset) => {
                let bytes = tables::mapping_bytes(offset).ok_or(IdnaError)?;
                length += core::str::from_utf8(bytes)
                    .map_err(|_| IdnaError)?
                    .chars()
                    .count();
            }
        }
    }
    let mut output = Vec::with_capacity(length);
    for &code_point in input {
        match tables::mapping(code_point) {
            tables::Mapping::Valid => output.push(code_point),
            tables::Mapping::Disallowed => return Err(IdnaError),
            tables::Mapping::Bytes(offset) => {
                let bytes = tables::mapping_bytes(offset).ok_or(IdnaError)?;
                output.extend(
                    core::str::from_utf8(bytes)
                        .map_err(|_| IdnaError)?
                        .chars()
                        .map(u32::from),
                );
            }
        }
    }
    debug_assert_eq!(output.len(), length);
    Ok(output)
}

fn is_label_valid(label: &[u32]) -> bool {
    if label.is_empty() {
        return true;
    }
    if tables::is_combining_mark(label[0]) {
        return false;
    }

    for (index, &code_point) in label.iter().enumerate() {
        if code_point == 0x200c {
            if index != 0 && VIRAMA.binary_search(&label[index - 1]).is_ok() {
                continue;
            }
            if index == 0 || index + 1 == label.len() {
                return false;
            }
            let valid_before = label[..index].iter().any(|candidate| {
                JOINING_LEFT.binary_search(candidate).is_ok()
                    || JOINING_DUAL.binary_search(candidate).is_ok()
            });
            let valid_after = label[index + 1..].iter().any(|candidate| {
                JOINING_RIGHT.binary_search(candidate).is_ok()
                    || JOINING_DUAL.binary_search(candidate).is_ok()
            });
            if !valid_before || !valid_after {
                return false;
            }
        } else if code_point == 0x200d
            && (index == 0 || VIRAMA.binary_search(&label[index - 1]).is_err())
        {
            return false;
        }
    }

    let rtl = label.iter().any(|code_point| {
        matches!(
            tables::direction(*code_point),
            Direction::R | Direction::AL | Direction::AN
        )
    });
    if !rtl {
        return true;
    }
    let Some(last_non_nsm) = label
        .iter()
        .rposition(|code_point| tables::direction(*code_point) != Direction::NSM)
    else {
        return false;
    };

    if tables::direction(label[0]) == Direction::L {
        if !label[..=last_non_nsm].iter().all(|code_point| {
            matches!(
                tables::direction(*code_point),
                Direction::L
                    | Direction::EN
                    | Direction::ES
                    | Direction::CS
                    | Direction::ET
                    | Direction::ON
                    | Direction::BN
                    | Direction::NSM
            )
        }) {
            return false;
        }
        matches!(
            tables::direction(label[last_non_nsm]),
            Direction::L | Direction::EN
        )
    } else {
        if !matches!(tables::direction(label[0]), Direction::R | Direction::AL) {
            return false;
        }
        let mut has_arabic_number = false;
        let mut has_european_number = false;
        for (index, &code_point) in label[..=last_non_nsm].iter().enumerate() {
            let direction = tables::direction(code_point);
            match direction {
                Direction::AN => has_arabic_number = true,
                Direction::EN => has_european_number = true,
                _ => {}
            }
            if has_arabic_number && has_european_number
                || !matches!(
                    direction,
                    Direction::R
                        | Direction::AL
                        | Direction::AN
                        | Direction::EN
                        | Direction::ES
                        | Direction::CS
                        | Direction::ET
                        | Direction::ON
                        | Direction::BN
                        | Direction::NSM
                )
                || index == last_non_nsm
                    && !matches!(
                        direction,
                        Direction::R | Direction::AL | Direction::AN | Direction::EN
                    )
            {
                return false;
            }
        }
        true
    }
}

#[inline]
fn contains_forbidden_domain_byte(bytes: &[u8]) -> bool {
    bytes.iter().any(|byte| {
        matches!(
            *byte,
            0x00..=0x20
                | 0x7f..=0xff
                | b'#'
                | b'%'
                | b'/'
                | b':'
                | b'<'
                | b'>'
                | b'?'
                | b'@'
                | b'['
                | b'\\'
                | b']'
                | b'^'
                | b'|'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{domain_to_ascii, domain_to_unicode};

    #[test]
    fn converts_unicode_and_punycode() {
        assert_eq!(
            domain_to_ascii("café.example").unwrap(),
            "xn--caf-dma.example"
        );
        assert_eq!(domain_to_unicode("xn--caf-dma.example").0, "café.example");
    }

    #[test]
    fn applies_uts46_mapping_and_nfc() {
        assert_eq!(domain_to_ascii("faß.de").unwrap(), "xn--fa-hia.de");
        assert_eq!(
            domain_to_ascii("CAFE\u{301}.example").unwrap(),
            "xn--caf-dma.example"
        );
    }
}
