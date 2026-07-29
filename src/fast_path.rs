//! Conservative fast path for already-normalized HTTP(S) URLs.

use alloc::{
    borrow::Cow,
    format,
    string::{String, ToString},
};
use core::{
    fmt::{self, Write},
    net::{Ipv4Addr, Ipv6Addr},
};
use memchr::{memchr, memchr2, memchr3};
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};

use crate::components::{Components, HostType, SchemeType};
use crate::idna::domain_to_ascii;

pub(crate) struct FastPath {
    pub buffer: String,
    pub components: Components,
    pub scheme_type: SchemeType,
    pub host_type: HostType,
    pub has_authority: bool,
    pub opaque_path: bool,
}

/// Parse the overwhelmingly common `http(s)://host` shape without running the
/// general scanner. This path is particularly important for IPv4 workloads:
/// the general DNS scanner deliberately rejects numeric final labels, which
/// otherwise makes an IPv4 URL traverse the input twice.
#[inline]
pub(crate) fn parse_bare_special(input: &str) -> Option<FastPath> {
    let (protocol_end, host_start, scheme_type) = special_prefix(input.as_bytes())?;
    let tail = input.as_bytes().get(host_start..)?;
    let (raw_host, has_path_slash) = tail
        .strip_suffix(b"/")
        .map_or((tail, false), |host| (host, true));
    if raw_host.is_empty() || raw_host.len() > 253 {
        return None;
    }
    if scheme_type == SchemeType::File && raw_host.eq_ignore_ascii_case(b"localhost") {
        return None;
    }

    if is_simple_domain(raw_host) && !ends_in_number(raw_host) {
        return finish_bare_copied_host(
            input,
            protocol_end,
            host_start,
            has_path_slash,
            scheme_type,
            HostType::Domain,
        );
    }

    if let Some(address) = raw_host
        .strip_prefix(b"[")
        .and_then(|host| host.strip_suffix(b"]"))
        .and_then(|host| core::str::from_utf8(host).ok())
        .and_then(|host| host.parse::<Ipv6Addr>().ok())
    {
        let mut canonical = Ipv6Buffer::default();
        write!(&mut canonical, "{address}").ok()?;
        if canonical.as_bytes() == &raw_host[1..raw_host.len() - 1] {
            return finish_bare_copied_host(
                input,
                protocol_end,
                host_start,
                has_path_slash,
                scheme_type,
                HostType::IPV6,
            );
        }
    }

    let address = parse_ipv4_bytes(raw_host)?;
    let mut buffer = String::with_capacity(host_start + 16);
    buffer.push_str(&input[..host_start]);
    push_ipv4(&mut buffer, address);
    let host_end = buffer.len();
    buffer.push('/');
    let components = Components::new(
        u32::try_from(protocol_end).ok()?,
        u32::try_from(host_start).ok()?,
        u32::try_from(host_start).ok()?,
        u32::try_from(host_end).ok()?,
        None,
        u32::try_from(host_end).ok()?,
        None,
        None,
    );
    debug_assert!(components.validate(buffer.len()));
    Some(FastPath {
        buffer,
        components,
        scheme_type,
        host_type: HostType::IPV4,
        has_authority: true,
        opaque_path: false,
    })
}

fn finish_bare_copied_host(
    input: &str,
    protocol_end: usize,
    host_start: usize,
    has_path_slash: bool,
    scheme_type: SchemeType,
    host_type: HostType,
) -> Option<FastPath> {
    let mut buffer = String::with_capacity(input.len() + usize::from(!has_path_slash));
    buffer.push_str(input);
    if !has_path_slash {
        buffer.push('/');
    }
    let host_end = input.len() - usize::from(has_path_slash);
    let components = Components::new(
        u32::try_from(protocol_end).ok()?,
        u32::try_from(host_start).ok()?,
        u32::try_from(host_start).ok()?,
        u32::try_from(host_end).ok()?,
        None,
        u32::try_from(host_end).ok()?,
        None,
        None,
    );
    debug_assert!(components.validate(buffer.len()));
    Some(FastPath {
        buffer,
        components,
        scheme_type,
        host_type,
        has_authority: true,
        opaque_path: false,
    })
}

struct Ipv6Buffer {
    bytes: [u8; 39],
    length: usize,
}

impl Default for Ipv6Buffer {
    fn default() -> Self {
        Self {
            bytes: [0; 39],
            length: 0,
        }
    }
}

impl Ipv6Buffer {
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

impl Write for Ipv6Buffer {
    fn write_str(&mut self, input: &str) -> fmt::Result {
        let end = self.length.checked_add(input.len()).ok_or(fmt::Error)?;
        let destination = self.bytes.get_mut(self.length..end).ok_or(fmt::Error)?;
        destination.copy_from_slice(input.as_bytes());
        self.length = end;
        Ok(())
    }
}

/// Validate a common absolute URL without allocating or producing
/// component offsets. `None` means the input needs the full WHATWG parser.
#[inline]
pub(crate) fn can_parse_special_absolute(input: &str) -> Option<bool> {
    let bytes = input.as_bytes();
    let (host_start, special) = if let Some((_, host_start, scheme_type)) = special_prefix(bytes) {
        if scheme_type == SchemeType::File {
            return None;
        }
        (host_start, true)
    } else {
        let colon = memchr(b':', bytes)?;
        let scheme = &bytes[..colon];
        if scheme.is_empty()
            || !scheme[0].is_ascii_alphabetic()
            || !scheme[1..]
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
            || !bytes.get(colon + 1..)?.starts_with(b"//")
        {
            return None;
        }
        (colon + 3, false)
    };
    let authority_tail = bytes.get(host_start..)?;
    let authority_len = memchr3(b'/', b'?', b'#', authority_tail).unwrap_or(authority_tail.len());
    let authority = &authority_tail[..authority_len];
    if authority.is_empty() {
        return Some(!special);
    }

    if special && memchr(b'\\', authority_tail).is_some_and(|index| index < authority_len) {
        return None;
    }

    let host_port = authority
        .iter()
        .rposition(|byte| *byte == b'@')
        .map_or(authority, |index| &authority[index + 1..]);
    if host_port.is_empty() {
        return Some(false);
    }
    if host_port.starts_with(b"[") {
        let closing = host_port.iter().position(|byte| *byte == b']')?;
        let address = core::str::from_utf8(&host_port[1..closing]).ok()?;
        if address.parse::<Ipv6Addr>().is_err() {
            return Some(false);
        }
        let after = &host_port[closing + 1..];
        if after.is_empty() {
            return Some(true);
        }
        let port = after.strip_prefix(b":")?;
        return Some(port.is_empty() || parse_port(port).is_some());
    }

    let (host, port) = host_port
        .iter()
        .rposition(|byte| *byte == b':')
        .map_or((host_port, None), |index| {
            (&host_port[..index], Some(&host_port[index + 1..]))
        });
    if host.is_empty() {
        return Some(!special && port.is_none());
    }
    if host.len() > 253 {
        return Some(false);
    }
    if let Some(port) = port {
        if port.is_empty() {
            // An empty port is accepted and stripped by the URL parser.
        } else if parse_port(port).is_none() {
            return Some(false);
        }
    }
    if !special {
        if host.iter().copied().any(is_forbidden_opaque_host_byte) {
            return Some(false);
        }
        return Some(true);
    }
    if ends_in_number(host) {
        return Some(parse_ipv4_bytes(host).is_some());
    }
    if !is_simple_domain_case_insensitive(host) {
        return None;
    }
    Some(true)
}

#[inline]
fn special_prefix(input: &[u8]) -> Option<(usize, usize, SchemeType)> {
    if input.starts_with(b"http://") {
        Some((5, 7, SchemeType::Http))
    } else if input.starts_with(b"https://") {
        Some((6, 8, SchemeType::Https))
    } else if input.starts_with(b"ws://") {
        Some((3, 5, SchemeType::Ws))
    } else if input.starts_with(b"wss://") {
        Some((4, 6, SchemeType::Wss))
    } else if input.starts_with(b"ftp://") {
        Some((4, 6, SchemeType::Ftp))
    } else if input.starts_with(b"file://") {
        Some((5, 7, SchemeType::File))
    } else {
        None
    }
}

#[inline]
fn is_simple_domain(host: &[u8]) -> bool {
    host.iter().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
    })
}

#[inline]
fn is_simple_domain_case_insensitive(host: &[u8]) -> bool {
    host.iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

#[inline]
fn parse_port(input: &[u8]) -> Option<u16> {
    let mut value = 0_u32;
    for &byte in input {
        let digit = byte.checked_sub(b'0')?;
        if digit > 9 {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(digit))?;
        if value > u32::from(u16::MAX) {
            return None;
        }
    }
    Some(value as u16)
}

#[derive(Clone, Copy)]
struct Scan {
    protocol_end: usize,
    host_start: usize,
    host_end: usize,
    path_start: usize,
    query_start: Option<usize>,
    hash_start: Option<usize>,
    scheme_type: SchemeType,
    needs_slash: bool,
    host_has_uppercase: bool,
}

const HOST_OK: u8 = 0;
const HOST_DELIMITER: u8 = 1;
const HOST_REJECT: u8 = 2;

const fn host_table() -> [u8; 256] {
    let mut table = [HOST_REJECT; 256];
    let mut byte = 0x21_usize;
    while byte <= 0x7e {
        table[byte] = HOST_OK;
        byte += 1;
    }
    let reject = b"#/:<>?@[\\]^|%";
    let mut index = 0;
    while index < reject.len() {
        table[reject[index] as usize] = HOST_REJECT;
        index += 1;
    }
    table[b'/' as usize] = HOST_DELIMITER;
    table[b'?' as usize] = HOST_DELIMITER;
    table[b'#' as usize] = HOST_DELIMITER;
    table
}

const HOST_TABLE: [u8; 256] = host_table();

const PATH_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'}');
const QUERY_ENCODE_SET: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'<').add(b'>');
const SPECIAL_QUERY_ENCODE_SET: &AsciiSet = &QUERY_ENCODE_SET.add(b'\'');
const FRAGMENT_ENCODE_SET: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`');
const USERINFO_ENCODE_SET: &AsciiSet = &PATH_ENCODE_SET
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'=')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'|');

#[inline]
pub(crate) fn parse(input: &str) -> Option<FastPath> {
    let scan = scan(input.as_bytes())?;
    let mut buffer = String::from(input);
    if scan.needs_slash {
        buffer.insert(scan.host_end, '/');
    }
    if scan.host_has_uppercase {
        buffer[scan.host_start..scan.host_end].make_ascii_lowercase();
    }

    let shift = u32::from(scan.needs_slash);
    let components = Components::new(
        u32::try_from(scan.protocol_end).ok()?,
        u32::try_from(scan.host_start).ok()?,
        u32::try_from(scan.host_start).ok()?,
        u32::try_from(scan.host_end).ok()?,
        None,
        u32::try_from(scan.path_start).ok()?,
        scan.query_start
            .map(u32::try_from)
            .transpose()
            .ok()?
            .map(|offset| offset + shift),
        scan.hash_start
            .map(u32::try_from)
            .transpose()
            .ok()?
            .map(|offset| offset + shift),
    );
    debug_assert!(components.validate(buffer.len()));

    Some(FastPath {
        buffer,
        components,
        scheme_type: scan.scheme_type,
        host_type: HostType::Domain,
        has_authority: true,
        opaque_path: false,
    })
}

#[inline]
pub(crate) fn normalized_len(input: &str) -> Option<usize> {
    let scan = scan(input.as_bytes())?;
    input.len().checked_add(usize::from(scan.needs_slash))
}

pub(crate) fn parse_normalized_file(input: &str) -> Option<FastPath> {
    let tail = input.strip_prefix("file://")?;
    let authority_length = memchr3(b'/', b'?', b'#', tail.as_bytes()).unwrap_or(tail.len());
    let host = &tail[..authority_length];
    let rest = &tail[authority_length..];
    if !host.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
    }) || host.eq_ignore_ascii_case("localhost")
        || ends_in_number(host.as_bytes())
        || !rest.starts_with('/')
    {
        return None;
    }

    let (path_end, query_start, hash_start) = scan_suffix(rest, 7 + authority_length)?;
    let path = &input.as_bytes()[7 + authority_length..path_end];
    if (path.len() >= 3 && path[0] == b'/' && path[1].is_ascii_alphabetic() && path[2] == b'|')
        || !valid_path(path)
    {
        return None;
    }
    let components = Components::new(
        5,
        7,
        7,
        u32::try_from(7 + authority_length).ok()?,
        None,
        u32::try_from(7 + authority_length).ok()?,
        query_start.map(u32::try_from).transpose().ok()?,
        hash_start.map(u32::try_from).transpose().ok()?,
    );
    debug_assert!(components.validate(input.len()));
    Some(FastPath {
        buffer: String::from(input),
        components,
        scheme_type: SchemeType::File,
        host_type: HostType::Domain,
        has_authority: true,
        opaque_path: false,
    })
}

pub(crate) enum CommonParse {
    Parsed(FastPath),
    Invalid,
    Unsupported,
}

pub(crate) fn parse_common_absolute(input: &str) -> CommonParse {
    if memchr3(b'\t', b'\n', b'\r', input.as_bytes()).is_some() {
        return CommonParse::Unsupported;
    }
    let (scheme, scheme_type, authority_start) = if input.starts_with("http://") {
        ("http", SchemeType::Http, 7)
    } else if input.starts_with("https://") {
        ("https", SchemeType::Https, 8)
    } else if input.starts_with("ws://") {
        ("ws", SchemeType::Ws, 5)
    } else if input.starts_with("wss://") {
        ("wss", SchemeType::Wss, 6)
    } else if input.starts_with("ftp://") {
        ("ftp", SchemeType::Ftp, 6)
    } else if input.starts_with("postgresql://") {
        ("postgresql", SchemeType::NotSpecial, 13)
    } else {
        let Some(colon) = memchr(b':', input.as_bytes()) else {
            return CommonParse::Unsupported;
        };
        let scheme = &input[..colon];
        if scheme.is_empty()
            || !scheme.as_bytes()[0].is_ascii_lowercase()
            || !scheme.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'+' | b'-' | b'.')
            })
            || scheme == "file"
            || !input[colon + 1..].starts_with("//")
        {
            return CommonParse::Unsupported;
        }
        (scheme, SchemeType::NotSpecial, colon + 3)
    };
    let special = scheme_type.is_special();
    let authority_tail = &input[authority_start..];
    let authority_length = if special {
        special_authority_length(authority_tail)
    } else {
        memchr3(b'/', b'?', b'#', authority_tail.as_bytes()).unwrap_or(authority_tail.len())
    };
    let authority = &authority_tail[..authority_length];
    let rest = &authority_tail[authority_length..];
    if authority.is_empty() {
        return if special {
            CommonParse::Invalid
        } else {
            CommonParse::Unsupported
        };
    }

    let (userinfo, host_port) = authority.rfind('@').map_or((None, authority), |index| {
        (Some(&authority[..index]), &authority[index + 1..])
    });
    let Some((raw_host, raw_port)) = split_host_port(host_port) else {
        return CommonParse::Invalid;
    };
    if raw_host.is_empty() {
        return if special {
            CommonParse::Invalid
        } else {
            CommonParse::Unsupported
        };
    }

    let (host, host_type): (Cow<'_, str>, HostType) = if let Some(address) = raw_host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
    {
        let Ok(address) = address.parse::<Ipv6Addr>() else {
            return CommonParse::Invalid;
        };
        (Cow::Owned(format!("[{address}]")), HostType::IPV6)
    } else if special {
        let Some(host) = normalize_special_host(raw_host) else {
            return CommonParse::Invalid;
        };
        (Cow::Owned(host.0), host.1)
    } else {
        if raw_host.bytes().any(is_forbidden_opaque_host_byte) {
            return CommonParse::Invalid;
        }
        let host =
            if raw_host.is_ascii() && raw_host.bytes().all(|byte| byte > 0x1f && byte != 0x7f) {
                Cow::Borrowed(raw_host)
            } else {
                Cow::Owned(utf8_percent_encode(raw_host, CONTROLS).to_string())
            };
        (host, HostType::Domain)
    };

    let port = match raw_port {
        Some("") | None => None,
        Some(port) if port.bytes().all(|byte| byte.is_ascii_digit()) => {
            let Ok(port) = port.parse::<u16>() else {
                return CommonParse::Invalid;
            };
            Some(port)
        }
        Some(_) => return CommonParse::Invalid,
    };
    let port = port.filter(|port| Some(*port) != scheme_type.default_port());

    let (before_fragment, fragment) = rest.find('#').map_or((rest, None), |index| {
        (&rest[..index], Some(&rest[index + 1..]))
    });
    let (raw_path, query) = before_fragment
        .find('?')
        .map_or((before_fragment, None), |index| {
            (
                &before_fragment[..index],
                Some(&before_fragment[index + 1..]),
            )
        });
    if special && raw_path.contains('\\') {
        return CommonParse::Unsupported;
    }
    let mut buffer = String::with_capacity(input.len() + 16);
    buffer.push_str(scheme);
    buffer.push_str("://");
    let protocol_end = scheme.len() + 1;
    let authority_offset = buffer.len();
    let username_end;
    if let Some(userinfo) = userinfo {
        let (username, password) = userinfo
            .split_once(':')
            .map_or((userinfo, None), |(username, password)| {
                (username, Some(password))
            });
        if username.is_empty() && password.is_none_or(str::is_empty) {
            username_end = authority_offset;
        } else {
            buffer.extend(utf8_percent_encode(username, USERINFO_ENCODE_SET));
            username_end = buffer.len();
            if let Some(password) = password.filter(|password| !password.is_empty()) {
                buffer.push(':');
                buffer.extend(utf8_percent_encode(password, USERINFO_ENCODE_SET));
            }
            buffer.push('@');
        }
    } else {
        username_end = authority_offset;
    }
    let host_start = buffer.len();
    buffer.push_str(&host);
    let host_end = buffer.len();
    if let Some(port) = port {
        buffer.push(':');
        push_u16_decimal(&mut buffer, port);
    }
    let pathname_start = buffer.len();
    if raw_path.is_empty() && special {
        buffer.push('/');
    } else {
        append_normalized_path(&mut buffer, raw_path);
    }
    let search_start = query.map(|query| {
        let start = buffer.len();
        buffer.push('?');
        let encode_set = if special {
            SPECIAL_QUERY_ENCODE_SET
        } else {
            QUERY_ENCODE_SET
        };
        buffer.extend(utf8_percent_encode(query, encode_set));
        start
    });
    let hash_start = fragment.map(|fragment| {
        let start = buffer.len();
        buffer.push('#');
        buffer.extend(utf8_percent_encode(fragment, FRAGMENT_ENCODE_SET));
        start
    });

    let (Ok(protocol_end), Ok(username_end), Ok(host_start), Ok(host_end), Ok(pathname_start)) = (
        u32::try_from(protocol_end),
        u32::try_from(username_end),
        u32::try_from(host_start),
        u32::try_from(host_end),
        u32::try_from(pathname_start),
    ) else {
        return CommonParse::Invalid;
    };
    let Ok(search_start) = search_start.map(u32::try_from).transpose() else {
        return CommonParse::Invalid;
    };
    let Ok(hash_start) = hash_start.map(u32::try_from).transpose() else {
        return CommonParse::Invalid;
    };
    let components = Components::new(
        protocol_end,
        username_end,
        host_start,
        host_end,
        port,
        pathname_start,
        search_start,
        hash_start,
    );
    debug_assert!(components.validate(buffer.len()));
    CommonParse::Parsed(FastPath {
        buffer,
        components,
        scheme_type,
        host_type,
        has_authority: true,
        opaque_path: false,
    })
}

pub(crate) fn normalize_special_host(raw_host: &str) -> Option<(String, HostType)> {
    if raw_host.is_empty() {
        return None;
    }
    if let Some(address) = raw_host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
    {
        let address = address.parse::<Ipv6Addr>().ok()?;
        return Some((format!("[{address}]"), HostType::IPV6));
    }

    if raw_host.is_ascii() && !raw_host.contains('%') {
        if ends_in_number(raw_host.as_bytes()) {
            let address = parse_ipv4(raw_host)?;
            return Some((Ipv4Addr::from(address).to_string(), HostType::IPV4));
        }
        if is_simple_domain_case_insensitive(raw_host.as_bytes()) {
            let mut host = String::from(raw_host);
            host.make_ascii_lowercase();
            return Some((host, HostType::Domain));
        }
    }

    let decoded = percent_decode_str(raw_host).decode_utf8().ok()?;
    let ascii_host = domain_to_ascii(&decoded).ok()?;
    if ascii_host.is_empty() {
        return None;
    }
    if ends_in_number(ascii_host.as_bytes()) {
        let address = parse_ipv4(&ascii_host)?;
        Some((Ipv4Addr::from(address).to_string(), HostType::IPV4))
    } else {
        Some((ascii_host, HostType::Domain))
    }
}

pub(crate) fn parse_opaque_absolute(input: &str) -> Option<FastPath> {
    if input
        .chars()
        .next()
        .is_some_and(|character| character <= '\u{20}')
        || input
            .chars()
            .next_back()
            .is_some_and(|character| character <= '\u{20}')
    {
        return None;
    }

    let colon = memchr(b':', input.as_bytes())?;
    let raw_scheme = &input[..colon];
    if raw_scheme.is_empty()
        || !raw_scheme.as_bytes()[0].is_ascii_alphabetic()
        || !raw_scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
    {
        return None;
    }
    let rest = &input[colon + 1..];
    if rest.starts_with('/') {
        return None;
    }
    if ["http", "https", "ws", "wss", "ftp", "file"]
        .iter()
        .any(|special| raw_scheme.eq_ignore_ascii_case(special))
    {
        return None;
    }
    if memchr3(b'\t', b'\n', b'\r', input.as_bytes()).is_some() {
        return None;
    }
    let scheme_type = SchemeType::NotSpecial;

    let (before_fragment, fragment) = rest.find('#').map_or((rest, None), |index| {
        (&rest[..index], Some(&rest[index + 1..]))
    });
    let (mut raw_path, query) =
        before_fragment
            .find('?')
            .map_or((before_fragment, None), |index| {
                (
                    &before_fragment[..index],
                    Some(&before_fragment[index + 1..]),
                )
            });
    let encode_final_space = (query.is_some() || fragment.is_some()) && raw_path.ends_with(' ');
    if query.is_none() && fragment.is_none() {
        raw_path = raw_path.trim_end_matches(' ');
    } else if encode_final_space {
        raw_path = &raw_path[..raw_path.len() - 1];
    }

    let mut buffer = String::with_capacity(input.len());
    if raw_scheme.bytes().any(|byte| byte.is_ascii_uppercase()) {
        buffer.extend(
            raw_scheme
                .chars()
                .map(|character| character.to_ascii_lowercase()),
        );
    } else {
        buffer.push_str(raw_scheme);
    }
    buffer.push(':');
    let protocol_end = buffer.len();
    let pathname_start = protocol_end;
    buffer.extend(utf8_percent_encode(raw_path, CONTROLS));
    if encode_final_space {
        buffer.push_str("%20");
    }
    let search_start = query.map(|query| {
        let start = buffer.len();
        buffer.push('?');
        buffer.extend(utf8_percent_encode(query, QUERY_ENCODE_SET));
        start
    });
    let hash_start = fragment.map(|fragment| {
        let start = buffer.len();
        buffer.push('#');
        buffer.extend(utf8_percent_encode(fragment, FRAGMENT_ENCODE_SET));
        start
    });
    let components = Components::new(
        u32::try_from(protocol_end).ok()?,
        u32::try_from(protocol_end).ok()?,
        u32::try_from(protocol_end).ok()?,
        u32::try_from(protocol_end).ok()?,
        None,
        u32::try_from(pathname_start).ok()?,
        search_start.map(u32::try_from).transpose().ok()?,
        hash_start.map(u32::try_from).transpose().ok()?,
    );
    debug_assert!(components.validate(buffer.len()));
    Some(FastPath {
        buffer,
        components,
        scheme_type,
        host_type: HostType::Domain,
        has_authority: false,
        opaque_path: true,
    })
}

fn split_host_port(input: &str) -> Option<(&str, Option<&str>)> {
    if input.starts_with('[') {
        let closing = input.find(']')?;
        let host = &input[..=closing];
        let after = &input[closing + 1..];
        if after.is_empty() {
            Some((host, None))
        } else {
            Some((host, Some(after.strip_prefix(':')?)))
        }
    } else {
        Some(input.rfind(':').map_or((input, None), |index| {
            (&input[..index], Some(&input[index + 1..]))
        }))
    }
}

fn is_forbidden_opaque_host_byte(byte: u8) -> bool {
    matches!(
        byte,
        0x00 | b' '
            | b'#'
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
}

fn is_single_dot(segment: &str) -> bool {
    segment == "." || segment.eq_ignore_ascii_case("%2e")
}

fn is_double_dot(segment: &str) -> bool {
    segment == ".."
        || segment.eq_ignore_ascii_case(".%2e")
        || segment.eq_ignore_ascii_case("%2e.")
        || segment.eq_ignore_ascii_case("%2e%2e")
}

fn scan(input: &[u8]) -> Option<Scan> {
    let (protocol_end, host_start, scheme_type) = if input.starts_with(b"http://") {
        (5, 7, SchemeType::Http)
    } else if input.starts_with(b"https://") {
        (6, 8, SchemeType::Https)
    } else {
        return None;
    };
    if input.len() <= host_start || matches!(input[host_start], b'/' | b'\\') {
        return None;
    }

    let authority = &input[host_start..];
    let relative_host_end = memchr3(b'/', b'?', b'#', authority).unwrap_or(authority.len());
    let host_end = host_start + relative_host_end;
    let host = &input[host_start..host_end];
    if host.is_empty() || host.len() > 253 {
        return None;
    }

    let mut has_upper = false;
    for &byte in host {
        if HOST_TABLE[byte as usize] != HOST_OK {
            return None;
        }
        has_upper |= byte.is_ascii_uppercase();
    }
    if ends_in_number(host) {
        return None;
    }

    let mut path_start = host_end;
    let mut query_start = None;
    let mut hash_start = None;
    let mut needs_slash = true;

    if host_end < input.len() {
        match input[host_end] {
            b'/' => {
                needs_slash = false;
                let rest = &input[host_end..];
                let query_or_hash = memchr2(b'?', b'#', rest);
                let path_end = query_or_hash.map_or(input.len(), |offset| host_end + offset);
                if !valid_path(&input[host_end..path_end]) {
                    return None;
                }
                if let Some(offset) = query_or_hash {
                    let delimiter = host_end + offset;
                    if input[delimiter] == b'?' {
                        query_start = Some(delimiter);
                        hash_start = memchr(b'#', &input[delimiter + 1..])
                            .map(|relative| delimiter + 1 + relative);
                    } else {
                        hash_start = Some(delimiter);
                    }
                }
            }
            b'?' => {
                query_start = Some(host_end);
                hash_start =
                    memchr(b'#', &input[host_end + 1..]).map(|relative| host_end + 1 + relative);
            }
            b'#' => hash_start = Some(host_end),
            _ => return None,
        }
    }

    let query_end = hash_start.unwrap_or(input.len());
    if let Some(start) = query_start
        && !valid_query_or_fragment(&input[start + 1..query_end])
    {
        return None;
    }
    if let Some(start) = hash_start
        && !valid_query_or_fragment(&input[start + 1..])
    {
        return None;
    }

    if needs_slash {
        path_start = host_end;
    }

    Some(Scan {
        protocol_end,
        host_start,
        host_end,
        path_start,
        query_start,
        hash_start,
        scheme_type,
        needs_slash,
        host_has_uppercase: has_upper,
    })
}

#[inline]
fn valid_path(path: &[u8]) -> bool {
    path.split(|&byte| byte == b'/').all(|segment| {
        segment != b"."
            && segment != b".."
            && segment.iter().all(|&byte| {
                matches!(byte, 0x21..=0x7e)
                    && !matches!(
                        byte,
                        b'"' | b'<' | b'>' | b'`' | b'{' | b'}' | b'^' | b'\\' | b'%' | b'\''
                    )
            })
    })
}

#[inline]
fn valid_query_or_fragment(input: &[u8]) -> bool {
    input.iter().all(|&byte| {
        matches!(byte, 0x21..=0x7e)
            && !matches!(
                byte,
                b'"' | b'<' | b'>' | b'`' | b'{' | b'}' | b'^' | b'\\' | b'\''
            )
    })
}

#[inline]
fn ends_in_number(host: &[u8]) -> bool {
    let host = host.strip_suffix(b".").unwrap_or(host);
    let final_label = host.rsplit(|&byte| byte == b'.').next().unwrap_or(host);
    !final_label.is_empty()
        && (final_label.iter().all(u8::is_ascii_digit)
            || (final_label.len() >= 2
                && final_label[0] == b'0'
                && matches!(final_label[1], b'x' | b'X')
                && final_label[2..].iter().all(u8::is_ascii_hexdigit)))
}

fn parse_ipv4(input: &str) -> Option<u32> {
    parse_ipv4_bytes(input.as_bytes())
}

fn parse_ipv4_bytes(input: &[u8]) -> Option<u32> {
    let input = input.strip_suffix(b".").unwrap_or(input);
    let mut numbers = [0_u64; 4];
    let mut count = 0_usize;
    for part in input.split(|byte| *byte == b'.') {
        if part.is_empty() || count == numbers.len() {
            return None;
        }
        numbers[count] = parse_ipv4_number(part)?;
        count += 1;
    }
    if count == 0 || numbers[..count - 1].iter().any(|number| *number > 255) {
        return None;
    }

    let last_limit = 1_u64 << (8 * (5 - count));
    if numbers[count - 1] >= last_limit {
        return None;
    }
    let mut value = numbers[count - 1];
    for (index, number) in numbers[..count - 1].iter().enumerate() {
        value += number << (8 * (3 - index));
    }
    u32::try_from(value).ok()
}

#[inline]
fn parse_ipv4_number(input: &[u8]) -> Option<u64> {
    let (digits, radix) = if let Some(digits) = input
        .strip_prefix(b"0x")
        .or_else(|| input.strip_prefix(b"0X"))
    {
        (digits, 16_u64)
    } else if input.len() >= 2 && input.starts_with(b"0") {
        (&input[1..], 8)
    } else {
        (input, 10)
    };
    if digits.is_empty() {
        return Some(0);
    }
    let mut value = 0_u64;
    for &byte in digits {
        let digit = match byte {
            b'0'..=b'9' => u64::from(byte - b'0'),
            b'a'..=b'f' => u64::from(byte - b'a' + 10),
            b'A'..=b'F' => u64::from(byte - b'A' + 10),
            _ => return None,
        };
        if digit >= radix {
            return None;
        }
        value = value.checked_mul(radix)?.checked_add(digit)?;
    }
    Some(value)
}

#[inline]
fn push_ipv4(buffer: &mut String, address: u32) {
    push_u8_decimal(buffer, (address >> 24) as u8);
    buffer.push('.');
    push_u8_decimal(buffer, (address >> 16) as u8);
    buffer.push('.');
    push_u8_decimal(buffer, (address >> 8) as u8);
    buffer.push('.');
    push_u8_decimal(buffer, address as u8);
}

#[inline]
fn push_u8_decimal(buffer: &mut String, value: u8) {
    if value >= 100 {
        buffer.push(char::from(b'0' + value / 100));
        buffer.push(char::from(b'0' + (value / 10) % 10));
    } else if value >= 10 {
        buffer.push(char::from(b'0' + value / 10));
    }
    buffer.push(char::from(b'0' + value % 10));
}

#[inline]
fn push_u16_decimal(buffer: &mut String, mut value: u16) {
    let mut digits = [0_u8; 5];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    for digit in &digits[start..] {
        buffer.push(char::from(*digit));
    }
}

fn append_normalized_path(buffer: &mut String, raw_path: &str) {
    let path_start = buffer.len();
    let mut has_segment = false;
    let mut segments = raw_path.split('/').peekable();
    while let Some(segment) = segments.next() {
        if is_single_dot(segment) {
            if segments.peek().is_none() {
                append_empty_path_segment(buffer, has_segment);
                has_segment = true;
            }
            continue;
        }
        if is_double_dot(segment) {
            shorten_path(buffer, path_start);
            if segments.peek().is_none() {
                append_empty_path_segment(buffer, has_segment);
                has_segment = true;
            }
            continue;
        }
        if has_segment {
            buffer.push('/');
        }
        buffer.extend(utf8_percent_encode(segment, PATH_ENCODE_SET));
        has_segment = true;
    }
}

fn append_empty_path_segment(buffer: &mut String, has_segment: bool) {
    if has_segment {
        buffer.push('/');
    }
}

fn shorten_path(buffer: &mut String, path_start: usize) {
    if buffer.len() <= path_start {
        return;
    }
    if buffer.as_bytes().last() == Some(&b'/') {
        buffer.truncate(buffer.len() - 1);
        return;
    }
    let relative = &buffer.as_bytes()[path_start..];
    if let Some(slash) = relative.iter().rposition(|byte| *byte == b'/') {
        buffer.truncate(path_start + slash);
    } else {
        buffer.truncate(path_start);
    }
}

fn special_authority_length(authority_tail: &str) -> usize {
    let bytes = authority_tail.as_bytes();
    let standard = memchr3(b'/', b'?', b'#', bytes).unwrap_or(bytes.len());
    memchr(b'\\', bytes).map_or(standard, |backslash| standard.min(backslash))
}

fn scan_suffix(
    suffix: &str,
    absolute_start: usize,
) -> Option<(usize, Option<usize>, Option<usize>)> {
    let query_or_hash = memchr2(b'?', b'#', suffix.as_bytes());
    let path_end = query_or_hash.map_or(absolute_start + suffix.len(), |offset| {
        absolute_start + offset
    });
    let mut query_start = None;
    let mut hash_start = None;
    if let Some(offset) = query_or_hash {
        let delimiter = absolute_start + offset;
        if suffix.as_bytes()[offset] == b'?' {
            query_start = Some(delimiter);
            hash_start = memchr(b'#', &suffix.as_bytes()[offset + 1..])
                .map(|relative| delimiter + 1 + relative);
        } else {
            hash_start = Some(delimiter);
        }
    }
    let query_end = hash_start.unwrap_or(absolute_start + suffix.len());
    if let Some(start) = query_start
        && !valid_query_or_fragment(
            &suffix.as_bytes()[start + 1 - absolute_start..query_end - absolute_start],
        )
    {
        return None;
    }
    if let Some(start) = hash_start
        && !valid_query_or_fragment(&suffix.as_bytes()[start + 1 - absolute_start..])
    {
        return None;
    }
    Some((path_end, query_start, hash_start))
}

#[cfg(test)]
mod tests {
    use super::{CommonParse, parse, parse_common_absolute, parse_normalized_file};

    #[test]
    fn accepts_normalized_http() {
        let parsed = parse("https://Example.COM/a/b?q=1#f").unwrap();
        assert_eq!(parsed.buffer, "https://example.com/a/b?q=1#f");
        assert_eq!(parsed.components.pathname_start, 19);
    }

    #[test]
    fn inserts_empty_path_slash() {
        let parsed = parse("http://example.com?x").unwrap();
        assert_eq!(parsed.buffer, "http://example.com/?x");
        assert_eq!(parsed.components.search_start(), Some(19));
    }

    #[test]
    fn rejects_work_for_the_full_parser() {
        for input in [
            "HTTP://example.com",
            "http://127.0.0.1",
            "http://example.com/a/../b",
            "http://example.com/a b",
        ] {
            assert!(parse(input).is_none(), "{input}");
        }
    }

    #[test]
    fn parses_whatwg_ipv4_and_dot_segments() {
        let CommonParse::Parsed(parsed) = parse_common_absolute("http://0300.168.0xF0/a/b/../c")
        else {
            panic!("native common parser rejected a supported URL");
        };
        assert_eq!(parsed.buffer, "http://192.168.0.240/a/c");

        assert!(matches!(
            parse_common_absolute("http://256.256.256.256"),
            CommonParse::Invalid
        ));
    }

    #[test]
    fn parses_normalized_file_without_intermediates() {
        let parsed = parse_normalized_file("file:///C:/Users/test/file.txt?x#y").unwrap();
        assert_eq!(parsed.buffer, "file:///C:/Users/test/file.txt?x#y");
        assert_eq!(parsed.components.pathname_start, 7);
        assert_eq!(parsed.components.search_start(), Some(30));
    }
}
