// Enable std::simd when the nightly-simd feature is active.
#![cfg_attr(feature = "nightly-simd", feature(portable_simd))]
//! # Ada URL
//!
//! Fast, WHATWG-compliant URL parser written in pure Rust.
//!
//! The entire normalized URL is stored in a single `String` buffer;
//! all getters return `&str` slices into that buffer — zero allocation.
//! Setters use `Cow<'_, str>` internally so already-canonical values are
//! never copied.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

#[cfg(not(feature = "std"))]
use alloc::{
    borrow::Cow,
    string::{String, ToString},
};
#[cfg(feature = "std")]
use std::{borrow::Cow, string::String};

use core::{borrow, fmt, hash, ops};

// ---------------------------------------------------------------------------
// Submodules
// ---------------------------------------------------------------------------

pub(crate) mod character_sets;
pub(crate) mod checkers;
pub(crate) mod helpers;
pub(crate) mod idna;
pub(crate) mod idna_impl;
pub(crate) mod idna_norm_tables;
pub(crate) mod idna_tables;
pub(crate) mod parser;
#[cfg(feature = "nightly-simd")]
pub(crate) mod portable_simd_impl;
pub(crate) mod scheme;
pub(crate) mod serializers;
pub(crate) mod unicode;
pub(crate) mod url_search_params;
pub(crate) mod validator;

pub use idna::Idna;
pub use url_search_params::{
    UrlSearchParams, UrlSearchParamsEntry, UrlSearchParamsEntryIterator,
    UrlSearchParamsKeyIterator, UrlSearchParamsValueIterator,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Error returned by [`Url::parse`] when the input is not a valid URL.
#[derive(Debug, PartialEq, Eq)]
pub struct ParseUrlError<Input> {
    pub input: Input,
}

impl<I: fmt::Debug> fmt::Display for ParseUrlError<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid url: {:?}", self.input)
    }
}

#[cfg(feature = "std")]
impl<I: fmt::Debug> std::error::Error for ParseUrlError<I> {}

/// Host classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostType {
    Domain = 0,
    IPV4 = 1,
    IPV6 = 2,
}

/// Scheme classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemeType {
    Http = 0,
    NotSpecial = 1,
    Https = 2,
    Ws = 3,
    Ftp = 4,
    Wss = 5,
    File = 6,
}

/// Byte-offset components of a parsed URL.
///
/// ```text
/// https://user:pass@example.com:1234/foo/bar?baz#quux
///       |     |    |          | ^^^^|       |   |
///       |     |    |          | |   |       |   `-- hash_start
///       |     |    |          | |   |       `----- search_start
///       |     |    |          | |   `------------- pathname_start
///       |     |    |          | `----------------- port
///       |     |    |          `------------------- host_end
///       |     |    `------------------------------ host_start
///       |     `----------------------------------- username_end
///       `----------------------------------------- protocol_end
/// ```
#[derive(Debug)]
pub struct UrlComponents {
    pub protocol_end: u32,
    pub username_end: u32,
    pub host_start: u32,
    pub host_end: u32,
    pub port: Option<u32>,
    pub pathname_start: Option<u32>,
    pub search_start: Option<u32>,
    pub hash_start: Option<u32>,
}

// ---------------------------------------------------------------------------
// Internal constants / types (used inside the Url impl below)
// ---------------------------------------------------------------------------

/// Sentinel meaning "this optional component is absent".
const OMITTED: u32 = u32::MAX;

use character_sets::{
    C0_CONTROL_PERCENT_ENCODE, FRAGMENT_PERCENT_ENCODE, QUERY_PERCENT_ENCODE,
    SPECIAL_QUERY_PERCENT_ENCODE, USERINFO_PERCENT_ENCODE,
};
use checkers::{
    is_ipv4, is_windows_drive_letter, path_signature, try_parse_ipv4_fast, verify_dns_length,
};
use helpers::{
    get_host_delimiter_location, parse_prepared_path, shorten_path, strip_tabs_newlines,
    strip_trailing_spaces_from_opaque_path,
};
use scheme::SchemeType as Scheme;
use unicode::{
    contains_forbidden_domain_code_point, contains_forbidden_domain_code_point_or_upper,
    contains_xn_prefix_pub, is_alnum_plus, is_ascii_digit, is_ascii_hex_digit,
    is_forbidden_host_code_point, percent_encode, to_ascii,
};

/// Internal host classification (distinct from the public `HostType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostKind {
    Domain,
    Ipv4,
    Ipv6,
}

/// Byte offsets of URL components within `Url::buffer`.
#[derive(Clone, Debug)]
struct Components {
    protocol_end: u32,
    username_end: u32,
    host_start: u32,
    host_end: u32,
    port: u32, // OMITTED when absent
    pathname_start: u32,
    search_start: u32, // OMITTED when absent
    hash_start: u32,   // OMITTED when absent
}

impl Components {
    fn new() -> Self {
        Self {
            protocol_end: 0,
            username_end: 0,
            host_start: 0,
            host_end: 0,
            port: OMITTED,
            pathname_start: 0,
            search_start: OMITTED,
            hash_start: OMITTED,
        }
    }
}

// ===========================================================================
// Url
// ===========================================================================

/// A parsed, normalized WHATWG URL.
///
/// The entire URL string lives in one contiguous `buffer`; `components` holds
/// byte offsets so that all getters return `&str` slices with **zero**
/// allocation.  Setters use `Cow` internally so values that are already in
/// canonical form are never copied.
pub struct Url {
    buffer: String,
    components: Components,
    is_valid: bool,
    has_opaque_path: bool,
    scheme: Scheme,
    host_kind: HostKind,
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

impl Url {
    fn empty() -> Self {
        Self {
            buffer: String::new(),
            components: Components::new(),
            is_valid: true,
            has_opaque_path: false,
            scheme: Scheme::NotSpecial,
            host_kind: HostKind::Domain,
        }
    }

    /// Parse `input`, optionally relative to a `base` URL string.
    pub fn parse<Input>(input: Input, base: Option<&str>) -> Result<Self, ParseUrlError<Input>>
    where
        Input: AsRef<str>,
    {
        let base_url = if let Some(b) = base {
            match parser::parse_url(b, None) {
                Some(u) if u.is_valid => Some(u),
                _ => return Err(ParseUrlError { input }),
            }
        } else {
            None
        };

        match parser::parse_url(input.as_ref(), base_url.as_ref()) {
            Some(u) if u.is_valid => Ok(u),
            _ => Err(ParseUrlError { input }),
        }
    }

    /// Returns `true` when `input` can be parsed as a valid URL.
    ///
    /// When `base` is `None` this uses a zero-allocation fast-path validator
    /// so it is significantly cheaper than calling [`Url::parse`].
    #[must_use]
    pub fn can_parse(input: &str, base: Option<&str>) -> bool {
        match base {
            None => {
                // Ultra-fast path: single-scan validator, zero allocations.
                // Handles the common case (absolute ASCII URL, simple host) in
                // one forward pass with no function-call overhead.
                if parser::try_validate_absolute_fast(input).is_some() {
                    return true;
                }
                // Full validator for edge cases: credentials, IPv4/IPv6, IDNA,
                // non-special schemes, relative URLs (still zero-allocation).
                validator::can_parse_no_base(input)
            }
            // With a base we still need relative resolution — parse both URLs.
            Some(b) => {
                let base_url = match parser::parse_url(b, None) {
                    Some(u) if u.is_valid => u,
                    _ => return false,
                };
                matches!(parser::parse_url(input, Some(&base_url)), Some(u) if u.is_valid)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Boolean queries
// ---------------------------------------------------------------------------

impl Url {
    #[inline]
    pub(crate) fn is_special(&self) -> bool {
        self.scheme.is_special()
    }

    #[inline]
    pub(crate) fn has_authority(&self) -> bool {
        let p = self.components.protocol_end as usize;
        self.components.protocol_end + 2 <= self.components.host_start
            && self.buffer.len() >= p + 2
            && self.buffer.as_bytes()[p] == b'/'
            && self.buffer.as_bytes()[p + 1] == b'/'
    }

    #[inline]
    pub fn has_credentials(&self) -> bool {
        self.has_non_empty_username() || self.has_non_empty_password()
    }
    #[inline]
    pub fn has_non_empty_username(&self) -> bool {
        self.components.protocol_end + 2 < self.components.username_end
    }
    #[inline]
    pub fn has_non_empty_password(&self) -> bool {
        self.components.host_start > self.components.username_end
    }
    #[inline]
    pub fn has_password(&self) -> bool {
        self.components.host_start > self.components.username_end
            && self.buffer.len() > self.components.username_end as usize
            && self.buffer.as_bytes()[self.components.username_end as usize] == b':'
    }
    #[inline]
    pub fn has_port(&self) -> bool {
        self.has_hostname() && self.components.pathname_start != self.components.host_end
    }
    #[inline]
    pub fn has_hash(&self) -> bool {
        self.components.hash_start != OMITTED
    }
    #[inline]
    pub fn has_search(&self) -> bool {
        self.components.search_start != OMITTED
    }
    #[inline]
    pub fn has_hostname(&self) -> bool {
        self.has_authority()
    }
    #[inline]
    pub fn has_empty_hostname(&self) -> bool {
        if !self.has_hostname() {
            return false;
        }
        if self.components.host_start == self.components.host_end {
            return true;
        }
        if self.components.host_end > self.components.host_start + 1 {
            return false;
        }
        self.components.username_end != self.components.host_start
    }
    #[inline]
    pub(crate) fn cannot_have_credentials_or_port(&self) -> bool {
        self.scheme == Scheme::File || self.components.host_start == self.components.host_end
    }
    #[inline]
    pub(crate) fn has_dash_dot(&self) -> bool {
        self.components.pathname_start == self.components.host_end + 2
            && !self.has_opaque_path
            && self.buffer.len() > (self.components.host_end + 1) as usize
            && self.buffer.as_bytes()[self.components.host_end as usize] == b'/'
            && self.buffer.as_bytes()[self.components.host_end as usize + 1] == b'.'
    }
    #[inline]
    pub(crate) fn is_at_path(&self) -> bool {
        self.buffer.len() == self.components.pathname_start as usize
    }
    #[inline]
    pub(crate) fn get_pathname_length(&self) -> u32 {
        let end = if self.components.search_start != OMITTED {
            self.components.search_start
        } else if self.components.hash_start != OMITTED {
            self.components.hash_start
        } else {
            self.buffer.len() as u32
        };
        end - self.components.pathname_start
    }
    #[inline]
    pub(crate) fn default_port(&self) -> u16 {
        self.scheme.default_port()
    }
    #[inline]
    pub(crate) fn retrieve_base_port(&self) -> u32 {
        self.components.port
    }

    /// Returns `true` when the hostname is a valid DNS label sequence.
    #[must_use]
    pub fn has_valid_domain(&self) -> bool {
        self.components.host_start != self.components.host_end && verify_dns_length(self.hostname())
    }
}

// ---------------------------------------------------------------------------
// Getters — zero-allocation &str slices into buffer
// ---------------------------------------------------------------------------

impl Url {
    /// Full serialized URL.
    #[must_use]
    #[inline]
    pub fn href(&self) -> &str {
        &self.buffer
    }
    /// `href()` alias.
    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.buffer
    }
    /// Scheme + colon, e.g. `"https:"`.
    #[must_use]
    #[inline]
    pub fn protocol(&self) -> &str {
        &self.buffer[..self.components.protocol_end as usize]
    }

    /// Percent-encoded username, or `""` when absent.
    #[must_use]
    #[inline]
    pub fn username(&self) -> &str {
        if self.has_non_empty_username() {
            &self.buffer
                [self.components.protocol_end as usize + 2..self.components.username_end as usize]
        } else {
            ""
        }
    }

    /// Percent-encoded password, or `""` when absent.
    #[must_use]
    #[inline]
    pub fn password(&self) -> &str {
        if self.has_non_empty_password() {
            &self.buffer
                [self.components.username_end as usize + 1..self.components.host_start as usize]
        } else {
            ""
        }
    }

    /// Port as a decimal string, or `""` when default/absent.
    #[must_use]
    #[inline]
    pub fn port(&self) -> &str {
        if self.components.port == OMITTED {
            ""
        } else {
            &self.buffer
                [self.components.host_end as usize + 1..self.components.pathname_start as usize]
        }
    }

    /// `host:port` (or just `host` when no explicit port is present).
    #[must_use]
    #[inline]
    pub fn host(&self) -> &str {
        let mut s = self.components.host_start as usize;
        if self.components.host_end > self.components.host_start
            && self.buffer.len() > s
            && self.buffer.as_bytes()[s] == b'@'
        {
            s += 1;
        }
        if s == self.components.host_end as usize {
            return "";
        }
        &self.buffer[s..self.components.pathname_start as usize]
    }

    /// Hostname without port.
    #[must_use]
    #[inline]
    pub fn hostname(&self) -> &str {
        let mut s = self.components.host_start as usize;
        if self.components.host_end > self.components.host_start
            && self.buffer.len() > s
            && self.buffer.as_bytes()[s] == b'@'
        {
            s += 1;
        }
        &self.buffer[s..self.components.host_end as usize]
    }

    /// Path component.
    #[must_use]
    #[inline]
    pub fn pathname(&self) -> &str {
        let end = if self.components.search_start != OMITTED {
            self.components.search_start as usize
        } else if self.components.hash_start != OMITTED {
            self.components.hash_start as usize
        } else {
            self.buffer.len()
        };
        &self.buffer[self.components.pathname_start as usize..end]
    }

    /// Query string including `?`, or `""` when absent/empty.
    #[must_use]
    #[inline]
    pub fn search(&self) -> &str {
        if self.components.search_start == OMITTED {
            return "";
        }
        let s = self.components.search_start as usize;
        let e = if self.components.hash_start != OMITTED {
            self.components.hash_start as usize
        } else {
            self.buffer.len()
        };
        if e - s <= 1 { "" } else { &self.buffer[s..e] }
    }

    /// Fragment including `#`, or `""` when absent/empty.
    #[must_use]
    #[inline]
    pub fn hash(&self) -> &str {
        if self.components.hash_start == OMITTED {
            return "";
        }
        let s = self.components.hash_start as usize;
        if self.buffer.len() - s <= 1 {
            ""
        } else {
            &self.buffer[s..]
        }
    }

    /// Serialized origin.
    #[must_use]
    #[cfg(feature = "std")]
    pub fn origin(&self) -> String {
        if self.is_special() {
            if self.scheme == Scheme::File {
                return "null".to_string();
            }
            return std::format!("{}//{}", self.protocol(), self.host());
        }
        if self.protocol() == "blob:" {
            let path = self.pathname();
            if !path.is_empty()
                && let Some(out) = parser::parse_url(path, None)
                && (out.scheme == Scheme::Http || out.scheme == Scheme::Https)
            {
                return std::format!("{}//{}", out.protocol(), out.host());
            }
        }
        "null".to_string()
    }

    /// Returns the byte-offset `UrlComponents` struct.
    #[must_use]
    pub fn components(&self) -> UrlComponents {
        UrlComponents {
            protocol_end: self.components.protocol_end,
            username_end: self.components.username_end,
            host_start: self.components.host_start,
            host_end: self.components.host_end,
            port: (self.components.port != OMITTED).then_some(self.components.port),
            pathname_start: (self.components.pathname_start != OMITTED)
                .then_some(self.components.pathname_start),
            search_start: (self.components.search_start != OMITTED)
                .then_some(self.components.search_start),
            hash_start: (self.components.hash_start != OMITTED)
                .then_some(self.components.hash_start),
        }
    }

    /// Returns the host classification.
    #[must_use]
    pub fn host_type(&self) -> HostType {
        match self.host_kind {
            HostKind::Domain => HostType::Domain,
            HostKind::Ipv4 => HostType::IPV4,
            HostKind::Ipv6 => HostType::IPV6,
        }
    }

    /// Returns the scheme classification.
    #[must_use]
    pub fn scheme_type(&self) -> SchemeType {
        match self.scheme {
            Scheme::Http => SchemeType::Http,
            Scheme::NotSpecial => SchemeType::NotSpecial,
            Scheme::Https => SchemeType::Https,
            Scheme::Ws => SchemeType::Ws,
            Scheme::Ftp => SchemeType::Ftp,
            Scheme::Wss => SchemeType::Wss,
            Scheme::File => SchemeType::File,
        }
    }
}

// ---------------------------------------------------------------------------
// Public setters
// ---------------------------------------------------------------------------

type SetterResult = Result<(), ()>;
#[inline]
fn ok(b: bool) -> SetterResult {
    if b { Ok(()) } else { Err(()) }
}

impl Url {
    /// Replace the entire URL.
    #[allow(clippy::result_unit_err)]
    pub fn set_href(&mut self, input: &str) -> SetterResult {
        ok(match parser::parse_url(input, None) {
            Some(u) => {
                *self = u;
                true
            }
            None => false,
        })
    }

    /// Update the scheme.
    #[allow(clippy::result_unit_err)]
    pub fn set_protocol(&mut self, input: &str) -> SetterResult {
        ok(set_protocol_impl(self, input))
    }

    /// Update the username (`None` / `Some("")` clears it).
    #[allow(clippy::result_unit_err)]
    pub fn set_username(&mut self, input: Option<&str>) -> SetterResult {
        if self.cannot_have_credentials_or_port() {
            return Err(());
        }
        let enc: Cow<'_, str> = percent_encode(input.unwrap_or(""), &USERINFO_PERCENT_ENCODE);
        self.update_base_username(&enc);
        Ok(())
    }

    /// Update the password (`None` / `Some("")` clears it).
    #[allow(clippy::result_unit_err)]
    pub fn set_password(&mut self, input: Option<&str>) -> SetterResult {
        if self.cannot_have_credentials_or_port() {
            return Err(());
        }
        let enc: Cow<'_, str> = percent_encode(input.unwrap_or(""), &USERINFO_PERCENT_ENCODE);
        self.update_base_password(&enc);
        Ok(())
    }

    /// Update the port (`None` removes it).
    #[allow(clippy::result_unit_err)]
    pub fn set_port(&mut self, input: Option<&str>) -> SetterResult {
        match input {
            None => {
                self.clear_port();
                Ok(())
            }
            Some(v) => ok(set_port_impl(self, v)),
        }
    }

    /// Update the fragment (`None` removes it).
    pub fn set_hash(&mut self, input: Option<&str>) {
        set_hash_impl(self, input.unwrap_or(""));
    }

    /// Update `host:port`.
    #[allow(clippy::result_unit_err)]
    pub fn set_host(&mut self, input: Option<&str>) -> SetterResult {
        ok(set_host_or_hostname(self, input.unwrap_or(""), false))
    }

    /// Update the hostname (without port).
    #[allow(clippy::result_unit_err)]
    pub fn set_hostname(&mut self, input: Option<&str>) -> SetterResult {
        ok(set_host_or_hostname(self, input.unwrap_or(""), true))
    }

    /// Update the pathname (`None` / `Some("")` sets it to `"/"`).
    #[allow(clippy::result_unit_err)]
    pub fn set_pathname(&mut self, input: Option<&str>) -> SetterResult {
        ok(set_pathname_impl(self, input.unwrap_or("")))
    }

    /// Update the search/query (`None` removes it).
    pub fn set_search(&mut self, input: Option<&str>) {
        set_search_impl(self, input.unwrap_or(""));
    }
}

// ---------------------------------------------------------------------------
// Internal buffer helpers
// ---------------------------------------------------------------------------

impl Url {
    #[inline]
    fn replace_range(&mut self, start: u32, end: u32, input: &str) -> i64 {
        let diff = input.len() as i64 - (end - start) as i64;
        self.buffer
            .replace_range(start as usize..end as usize, input);
        diff
    }

    /// Shift every component offset ≥ `from` by `diff` (skipping OMITTED sentinels).
    #[inline]
    fn shift_from(&mut self, diff: i64, from: u32) {
        macro_rules! adj {
            ($f:expr) => {
                if $f != OMITTED && $f >= from {
                    $f = ($f as i64 + diff) as u32;
                }
            };
        }
        adj!(self.components.username_end);
        adj!(self.components.host_start);
        adj!(self.components.host_end);
        adj!(self.components.pathname_start);
        if self.components.search_start != OMITTED {
            adj!(self.components.search_start);
        }
        if self.components.hash_start != OMITTED {
            adj!(self.components.hash_start);
        }
    }
}

// ---------------------------------------------------------------------------
// Scheme helpers (used by parser)
// ---------------------------------------------------------------------------

impl Url {
    pub(crate) fn set_protocol_as_file(&mut self) {
        let diff = 5i64 - self.components.protocol_end as i64;
        self.buffer
            .replace_range(..self.components.protocol_end as usize, "file:");
        self.components.protocol_end = 5;
        if diff != 0 {
            self.shift_from(diff, self.components.username_end);
        }
        self.scheme = Scheme::File;
    }

    pub(crate) fn set_scheme_from_view_with_colon(&mut self, s: &str) {
        let diff = s.len() as i64 - self.components.protocol_end as i64;
        if self.buffer.is_empty() {
            self.buffer.push_str(s);
        } else {
            self.buffer
                .replace_range(..self.components.protocol_end as usize, s);
        }
        self.components.protocol_end = s.len() as u32;
        if diff != 0 {
            self.shift_from(diff, self.components.username_end);
        }
    }

    pub(crate) fn copy_scheme(&mut self, other: &Url) {
        let proto = other.protocol();
        let diff = proto.len() as i64 - self.components.protocol_end as i64;
        self.scheme = other.scheme;
        if self.buffer.is_empty() {
            self.buffer.push_str(proto);
        } else {
            self.buffer
                .replace_range(..self.components.protocol_end as usize, proto);
        }
        self.components.protocol_end = proto.len() as u32;
        if diff != 0 {
            self.shift_from(diff, self.components.username_end);
        }
    }

    pub(crate) fn parse_scheme_with_colon(&mut self, s: &str) -> bool {
        let scheme = &s[..s.len() - 1];
        let slen = scheme.len();
        // Fast path: stack buffer for schemes ≤ 63 bytes (covers all real schemes).
        // This eliminates the heap allocation on the hot parse path.
        if slen <= 63 {
            let mut buf = [0u8; 64];
            buf[..slen].copy_from_slice(scheme.as_bytes());
            // SAFETY: bytes are ASCII alnum/+/-/. only
            unicode::to_lower_ascii(&mut buf[..slen]);
            buf[slen] = b':';
            let lo_with_colon = unsafe { core::str::from_utf8_unchecked(&buf[..slen + 1]) };
            let lo_scheme = unsafe { core::str::from_utf8_unchecked(&buf[..slen]) };
            self.scheme = scheme::get_scheme_type(lo_scheme);
            self.set_scheme_from_view_with_colon(lo_with_colon);
        } else {
            // Rare: very long scheme — fall back to owned String
            let mut lo = String::from(scheme);
            unicode::to_lower_ascii(unsafe { lo.as_bytes_mut() });
            self.scheme = scheme::get_scheme_type(&lo);
            lo.push(':');
            self.set_scheme_from_view_with_colon(&lo);
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Authority slashes
// ---------------------------------------------------------------------------

impl Url {
    pub(crate) fn add_authority_slashes_if_needed(&mut self) {
        if self.has_authority() {
            return;
        }
        self.buffer
            .insert_str(self.components.protocol_end as usize, "//");
        self.shift_from(2, self.components.username_end);
    }
}

// ---------------------------------------------------------------------------
// Hostname mutation
// ---------------------------------------------------------------------------

impl Url {
    pub(crate) fn update_base_hostname(&mut self, input: &str) {
        self.add_authority_slashes_if_needed();
        let has_creds = self.components.protocol_end + 2 < self.components.host_start;
        let (hs, he) = (self.components.host_start, self.components.host_end);
        let diff = self.replace_range(hs, he, input);
        if has_creds {
            let nhs = self.components.host_start as usize;
            if self.buffer.len() > nhs && self.buffer.as_bytes()[nhs] != b'@' {
                self.buffer.insert(nhs, '@');
                let d = diff + 1;
                self.components.host_end = (he as i64 + d) as u32;
                self.components.pathname_start = (self.components.pathname_start as i64 + d) as u32;
                if self.components.search_start != OMITTED {
                    self.components.search_start = (self.components.search_start as i64 + d) as u32;
                }
                if self.components.hash_start != OMITTED {
                    self.components.hash_start = (self.components.hash_start as i64 + d) as u32;
                }
                return;
            }
        }
        self.components.host_end = (he as i64 + diff) as u32;
        self.components.pathname_start = (self.components.pathname_start as i64 + diff) as u32;
        if self.components.search_start != OMITTED {
            self.components.search_start = (self.components.search_start as i64 + diff) as u32;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start = (self.components.hash_start as i64 + diff) as u32;
        }
    }

    pub(crate) fn update_host_to_base_host(&mut self, input: &str) {
        if self.scheme != Scheme::File && input.is_empty() && !self.is_special() {
            if self.has_hostname() {
                self.clear_hostname();
            } else if self.has_dash_dot() {
                self.add_authority_slashes_if_needed();
                self.delete_dash_dot();
            }
            return;
        }
        self.update_base_hostname(input);
    }
}

// ---------------------------------------------------------------------------
// Username / password mutation
// ---------------------------------------------------------------------------

impl Url {
    pub(crate) fn update_base_username(&mut self, input: &str) {
        self.add_authority_slashes_if_needed();
        let has_pw = self.has_non_empty_password();
        let at = self.buffer.len() > self.components.host_start as usize
            && self.buffer.as_bytes()[self.components.host_start as usize] == b'@';
        let (us, ue) = (
            self.components.protocol_end + 2,
            self.components.username_end,
        );
        let diff = self.replace_range(us, ue, input);
        self.components.username_end = (ue as i64 + diff) as u32;
        self.components.host_start = (self.components.host_start as i64 + diff) as u32;
        let total: i64;
        if !input.is_empty() && !at {
            self.buffer.insert(self.components.host_start as usize, '@');
            // host_start stays pointing AT '@' — same invariant as Ada C++
            total = diff + 1;
        } else if input.is_empty() && at && !has_pw {
            self.buffer.remove(self.components.host_start as usize);
            total = diff - 1;
        } else {
            total = diff;
        }
        self.components.host_end = (self.components.host_end as i64 + total) as u32;
        self.components.pathname_start = (self.components.pathname_start as i64 + total) as u32;
        if self.components.search_start != OMITTED {
            self.components.search_start = (self.components.search_start as i64 + total) as u32;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start = (self.components.hash_start as i64 + total) as u32;
        }
    }

    pub(crate) fn append_base_username(&mut self, input: &str) {
        if input.is_empty() {
            return;
        }
        self.add_authority_slashes_if_needed();
        self.buffer
            .insert_str(self.components.username_end as usize, input);
        let d = input.len() as u32;
        self.components.username_end += d;
        self.components.host_start += d;
        let hs = self.components.host_start as usize;
        if self.buffer.len() > hs
            && self.buffer.as_bytes()[hs] != b'@'
            && self.components.host_start != self.components.host_end
        {
            self.buffer.insert(hs, '@');
            // host_start stays pointing AT '@'
            self.components.host_end += d + 1;
            self.components.pathname_start += d + 1;
            if self.components.search_start != OMITTED {
                self.components.search_start += d + 1;
            }
            if self.components.hash_start != OMITTED {
                self.components.hash_start += d + 1;
            }
            return;
        }
        self.components.host_end += d;
        self.components.pathname_start += d;
        if self.components.search_start != OMITTED {
            self.components.search_start += d;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start += d;
        }
    }

    pub(crate) fn update_base_password(&mut self, input: &str) {
        self.add_authority_slashes_if_needed();
        if input.is_empty() {
            self.clear_password();
            if !self.has_non_empty_username() {
                self.update_base_username("");
            }
            return;
        }
        let pw = self.has_password();
        let mut diff = input.len() as i64;
        if pw {
            let cur = (self.components.host_start - self.components.username_end - 1) as usize;
            let s = self.components.username_end as usize + 1;
            self.buffer.replace_range(s..s + cur, input);
            diff -= cur as i64;
        } else {
            self.buffer
                .insert(self.components.username_end as usize, ':');
            self.buffer
                .insert_str(self.components.username_end as usize + 1, input);
            diff += 1;
        }
        self.components.host_start = (self.components.host_start as i64 + diff) as u32;
        if self.buffer.len() > self.components.host_start as usize
            && self.buffer.as_bytes()[self.components.host_start as usize] != b'@'
        {
            self.buffer.insert(self.components.host_start as usize, '@');
            diff += 1;
        }
        self.components.host_end = (self.components.host_end as i64 + diff) as u32;
        self.components.pathname_start = (self.components.pathname_start as i64 + diff) as u32;
        if self.components.search_start != OMITTED {
            self.components.search_start = (self.components.search_start as i64 + diff) as u32;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start = (self.components.hash_start as i64 + diff) as u32;
        }
    }

    pub(crate) fn append_base_password(&mut self, input: &str) {
        if input.is_empty() {
            return;
        }
        self.add_authority_slashes_if_needed();
        let mut diff = input.len() as i64;
        if self.has_password() {
            self.buffer
                .insert_str(self.components.host_start as usize, input);
        } else {
            diff += 1;
            self.buffer
                .insert(self.components.username_end as usize, ':');
            self.buffer
                .insert_str(self.components.username_end as usize + 1, input);
        }
        self.components.host_start = (self.components.host_start as i64 + diff) as u32;
        if self.buffer.len() > self.components.host_start as usize
            && self.buffer.as_bytes()[self.components.host_start as usize] != b'@'
        {
            self.buffer.insert(self.components.host_start as usize, '@');
            diff += 1;
        }
        self.components.host_end = (self.components.host_end as i64 + diff) as u32;
        self.components.pathname_start = (self.components.pathname_start as i64 + diff) as u32;
        if self.components.search_start != OMITTED {
            self.components.search_start = (self.components.search_start as i64 + diff) as u32;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start = (self.components.hash_start as i64 + diff) as u32;
        }
    }
}

// ---------------------------------------------------------------------------
// Port mutation
// ---------------------------------------------------------------------------

impl Url {
    pub(crate) fn update_base_port(&mut self, port: u32) {
        if port == OMITTED {
            self.clear_port();
            return;
        }
        let ps = fmt_port(port);
        let diff: i64 = if self.components.port != OMITTED {
            let old = (self.components.pathname_start - self.components.host_end) as usize;
            let s = self.components.host_end as usize;
            self.buffer.replace_range(s..s + old, &ps);
            ps.len() as i64 - old as i64
        } else {
            self.buffer
                .insert_str(self.components.host_end as usize, &ps);
            ps.len() as i64
        };
        self.components.pathname_start = (self.components.pathname_start as i64 + diff) as u32;
        if self.components.search_start != OMITTED {
            self.components.search_start = (self.components.search_start as i64 + diff) as u32;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start = (self.components.hash_start as i64 + diff) as u32;
        }
        self.components.port = port;
    }
}

// ---------------------------------------------------------------------------
// Pathname mutation
// ---------------------------------------------------------------------------

impl Url {
    pub(crate) fn update_base_pathname(&mut self, input: &str) {
        let ss = input.starts_with("//");
        if !ss && self.has_dash_dot() {
            self.delete_dash_dot();
        }
        if ss && !self.has_opaque_path && !self.has_authority() && !self.has_dash_dot() {
            let ins = self.components.pathname_start as usize;
            self.buffer.insert_str(ins, "/.");
            self.components.pathname_start += 2;
            if self.components.search_start != OMITTED {
                self.components.search_start += 2;
            }
            if self.components.hash_start != OMITTED {
                self.components.hash_start += 2;
            }
        }
        let plen = self.get_pathname_length();
        let (s, e) = (
            self.components.pathname_start,
            self.components.pathname_start + plen,
        );
        let diff = self.replace_range(s, e, input);
        if self.components.search_start != OMITTED {
            self.components.search_start = (self.components.search_start as i64 + diff) as u32;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start = (self.components.hash_start as i64 + diff) as u32;
        }
    }

    pub(crate) fn append_base_pathname(&mut self, input: &str) {
        let ins = if self.components.search_start != OMITTED {
            self.components.search_start as usize
        } else if self.components.hash_start != OMITTED {
            self.components.hash_start as usize
        } else {
            self.buffer.len()
        };
        self.buffer.insert_str(ins, input);
        let d = input.len() as u32;
        if self.components.search_start != OMITTED {
            self.components.search_start += d;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start += d;
        }
    }
}

// ---------------------------------------------------------------------------
// Search mutation
// ---------------------------------------------------------------------------

impl Url {
    /// Set search from already-clean content (no leading `?`, may be empty).
    /// Used by `set_search` and the parser's QUERY state via `update_base_search_with_encode`.
    pub(crate) fn write_search_content(&mut self, content: &str) {
        if self.components.hash_start == OMITTED {
            if self.components.search_start == OMITTED {
                self.components.search_start = self.buffer.len() as u32;
                self.buffer.push('?');
            } else {
                self.buffer
                    .truncate(self.components.search_start as usize + 1);
            }
            self.buffer.push_str(content);
        } else {
            if self.components.search_start == OMITTED {
                self.components.search_start = self.components.hash_start;
            } else {
                let (ss, hs) = (
                    self.components.search_start as usize,
                    self.components.hash_start as usize,
                );
                self.buffer.replace_range(ss..hs, "");
                self.components.hash_start = self.components.search_start;
            }
            let ins = self.components.search_start as usize;
            self.buffer.insert(ins, '?');
            self.buffer.insert_str(ins + 1, content);
            self.components.hash_start += (content.len() + 1) as u32;
        }
    }

    /// Used when copying from a base URL: strips a leading `?` if present, then
    /// writes the content. Only clears when `input` itself is truly empty (no `?`).
    pub(crate) fn update_base_search(&mut self, input: &str) {
        if input.is_empty() {
            self.clear_search();
            return;
        }
        let s = input.strip_prefix('?').unwrap_or(input);
        self.write_search_content(s);
    }

    /// Used by the parser's QUERY state: percent-encodes `input` verbatim (no
    /// leading-`?` stripping) then writes it as the search component.  An empty
    /// `input` sets the query to the empty string (not null) — `href` will end
    /// with `?`.  CoW: borrows `input` directly when no encoding is needed.
    pub(crate) fn update_base_search_with_encode(&mut self, input: &str, set: &[u8; 32]) {
        // CoW: borrows when no encoding needed, owns otherwise.
        let enc: Cow<'_, str> = percent_encode(input, set);
        self.write_search_content(&enc);
    }

    pub(crate) fn update_unencoded_base_hash(&mut self, input: &str) {
        if self.components.hash_start != OMITTED {
            self.buffer.truncate(self.components.hash_start as usize);
        }
        self.components.hash_start = self.buffer.len() as u32;
        self.buffer.push('#');
        let enc: Cow<'_, str> = percent_encode(input, &FRAGMENT_PERCENT_ENCODE);
        self.buffer.push_str(&enc);
    }

    pub(crate) fn update_base_authority(&mut self, base_buf: &str, base: &Components) {
        let input = &base_buf[base.protocol_end as usize..base.host_start as usize];
        let slash2 = input.starts_with("//");
        let old = self.components.host_start - self.components.protocol_end;
        self.buffer.replace_range(
            self.components.protocol_end as usize..self.components.host_start as usize,
            "",
        );
        self.components.username_end = self.components.protocol_end;
        let actual = if slash2 { &input[2..] } else { input };
        let mut added = 0u32;
        if slash2 {
            self.buffer
                .insert_str(self.components.protocol_end as usize, "//");
            self.components.username_end += 2;
            added += 2;
        }
        if let Some(cp) = actual.find(':') {
            let (u, p) = (&actual[..cp], &actual[cp + 1..]);
            let ins = self.components.protocol_end as usize + added as usize;
            self.buffer.insert_str(ins, u);
            let ul = u.len() as u32;
            self.buffer.insert(ins + ul as usize, ':');
            self.components.username_end = self.components.protocol_end + added + ul;
            self.buffer.insert_str(ins + ul as usize + 1, p);
            added += ul + 1 + p.len() as u32;
        } else if !actual.is_empty() {
            let ins = self.components.protocol_end as usize + added as usize;
            self.buffer.insert_str(ins, actual);
            self.components.username_end =
                self.components.protocol_end + added + actual.len() as u32;
            added += actual.len() as u32;
        }
        self.components.host_start = self.components.protocol_end + added;
        if !actual.is_empty()
            && self.buffer.len() > self.components.host_start as usize
            && self.buffer.as_bytes()[self.components.host_start as usize] != b'@'
        {
            self.buffer.insert(self.components.host_start as usize, '@');
            added += 1;
        }
        let net = added as i64 - old as i64;
        self.components.host_end = (self.components.host_end as i64 + net) as u32;
        self.components.pathname_start = (self.components.pathname_start as i64 + net) as u32;
        if self.components.search_start != OMITTED {
            self.components.search_start = (self.components.search_start as i64 + net) as u32;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start = (self.components.hash_start as i64 + net) as u32;
        }
    }
}

// ---------------------------------------------------------------------------
// Clear helpers
// ---------------------------------------------------------------------------

impl Url {
    pub(crate) fn clear_port(&mut self) {
        if self.components.port == OMITTED {
            return;
        }
        let (s, e) = (
            self.components.host_end as usize,
            self.components.pathname_start as usize,
        );
        let d = (e - s) as u32;
        self.buffer.replace_range(s..e, "");
        self.components.pathname_start -= d;
        if self.components.search_start != OMITTED {
            self.components.search_start -= d;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start -= d;
        }
        self.components.port = OMITTED;
    }

    pub(crate) fn clear_search(&mut self) {
        if self.components.search_start == OMITTED {
            return;
        }
        if self.components.hash_start == OMITTED {
            self.buffer.truncate(self.components.search_start as usize);
        } else {
            let (s, e) = (
                self.components.search_start as usize,
                self.components.hash_start as usize,
            );
            self.buffer.replace_range(s..e, "");
            self.components.hash_start = self.components.search_start;
        }
        self.components.search_start = OMITTED;
    }

    #[allow(dead_code)]
    pub(crate) fn clear_hash(&mut self) {
        if self.components.hash_start == OMITTED {
            return;
        }
        self.buffer.truncate(self.components.hash_start as usize);
        self.components.hash_start = OMITTED;
    }

    pub(crate) fn clear_pathname(&mut self) {
        let end = if self.components.search_start != OMITTED {
            self.components.search_start as usize
        } else if self.components.hash_start != OMITTED {
            self.components.hash_start as usize
        } else {
            self.buffer.len()
        };
        let start = self.components.pathname_start as usize;
        let plen = (end - start) as u32;
        self.buffer.replace_range(start..end, "");
        let he = self.components.host_end as usize;
        let mut total = plen;
        if self.buffer.len() > he + 1
            && self.buffer.as_bytes().get(he) == Some(&b'/')
            && self.buffer.as_bytes().get(he + 1) == Some(&b'.')
            && start >= he + 2
        {
            self.buffer.replace_range(he..he + 2, "");
            self.components.pathname_start -= 2;
            total += 2;
        }
        if self.components.search_start != OMITTED {
            self.components.search_start -= total;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start -= total;
        }
    }

    pub(crate) fn clear_hostname(&mut self) {
        if !self.has_authority() {
            return;
        }
        let mut start = self.components.host_start as usize;
        let mut len = (self.components.host_end - self.components.host_start) as usize;
        if len > 0 && self.buffer.len() > start && self.buffer.as_bytes()[start] == b'@' {
            start += 1;
            len -= 1;
        }
        self.buffer.replace_range(start..start + len, "");
        self.components.host_end = start as u32;
        let d = len as u32;
        self.components.pathname_start -= d;
        if self.components.search_start != OMITTED {
            self.components.search_start -= d;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start -= d;
        }
    }

    pub(crate) fn clear_password(&mut self) {
        if !self.has_password() {
            return;
        }
        let d = self.components.host_start - self.components.username_end;
        self.buffer.replace_range(
            self.components.username_end as usize..self.components.host_start as usize,
            "",
        );
        self.components.host_start -= d;
        self.components.host_end -= d;
        self.components.pathname_start -= d;
        if self.components.search_start != OMITTED {
            self.components.search_start -= d;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start -= d;
        }
    }

    pub(crate) fn delete_dash_dot(&mut self) {
        let s = self.components.host_end as usize;
        self.buffer.replace_range(s..s + 2, "");
        self.components.pathname_start -= 2;
        if self.components.search_start != OMITTED {
            self.components.search_start -= 2;
        }
        if self.components.hash_start != OMITTED {
            self.components.hash_start -= 2;
        }
    }
}

// ---------------------------------------------------------------------------
// Port parsing
// ---------------------------------------------------------------------------

impl Url {
    pub(crate) fn parse_port(&mut self, view: &str, check_trailing: bool) -> usize {
        let b = view.as_bytes();
        if !b.is_empty() && b[0] == b'-' {
            self.is_valid = false;
            return 0;
        }
        let consumed = b.iter().take_while(|&&c| is_ascii_digit(c)).count();
        if consumed == 0 {
            // No digits — valid only if we're immediately at a path/query delimiter
            if check_trailing && !b.is_empty() {
                let ok = b[0] == b'/' || b[0] == b'?' || (self.is_special() && b[0] == b'\\');
                self.is_valid &= ok;
            }
            return 0;
        }
        let n: u64 = view[..consumed]
            .bytes()
            .try_fold(0u64, |a, c| {
                a.checked_mul(10)?.checked_add((c - b'0') as u64)
            })
            .unwrap_or(u64::MAX);
        if n > 65535 {
            self.is_valid = false;
            return 0;
        }
        if check_trailing {
            let rest = &view[consumed..];
            self.is_valid &= rest.is_empty()
                || rest.starts_with('/')
                || rest.starts_with('?')
                || (self.is_special() && rest.starts_with('\\'));
        }
        if self.is_valid {
            let def = self.default_port();
            if def != 0 && n as u16 == def {
                self.clear_port();
            } else {
                self.update_base_port(n as u32);
            }
        }
        consumed
    }
}

// ---------------------------------------------------------------------------
// Host parsing
// ---------------------------------------------------------------------------

impl Url {
    pub(crate) fn parse_host(&mut self, input: &str) -> bool {
        if input.is_empty() {
            self.is_valid = false;
            return false;
        }
        if input.starts_with('[') {
            return if input.ends_with(']') {
                self.parse_ipv6(&input[1..input.len() - 1])
            } else {
                self.is_valid = false;
                false
            };
        }
        if !self.is_special() {
            return self.parse_opaque_host(input);
        }
        let fast = try_parse_ipv4_fast(input);
        if fast != u64::MAX {
            self.update_base_hostname(input.trim_end_matches('.'));
            self.host_kind = HostKind::Ipv4;
            return true;
        }
        let b = input.as_bytes();
        let status = contains_forbidden_domain_code_point_or_upper(b);
        if status == 0 && !contains_xn_prefix_pub(input) {
            self.update_base_hostname(input);
            if is_ipv4(self.hostname()) {
                let hn = self.hostname().to_string();
                return self.parse_ipv4(&hn, true);
            }
            return true;
        }
        match to_ascii(input, input.find('%')) {
            None => {
                self.is_valid = false;
                false
            }
            Some(ascii) => {
                if contains_forbidden_domain_code_point(ascii.as_bytes()) {
                    self.is_valid = false;
                    return false;
                }
                if is_ipv4(&ascii) {
                    let s = ascii.into_owned();
                    return self.parse_ipv4(&s, false);
                }
                self.update_base_hostname(&ascii);
                true
            }
        }
    }

    pub(crate) fn parse_ipv4(&mut self, input: &str, in_place: bool) -> bool {
        let input = input.trim_end_matches('.');
        let mut digits = 0usize;
        let mut pure_dec = 0i32;
        let mut ipv4: u64 = 0;
        let mut rem = input;
        loop {
            if digits >= 4 || rem.is_empty() {
                break;
            }
            let b = rem.as_bytes();
            let (val, n): (u64, usize) =
                if b.len() >= 2 && b[0] == b'0' && (b[1] == b'x' || b[1] == b'X') {
                    if b.len() == 2 || (b.len() > 2 && b[2] == b'.') {
                        (0, 2)
                    } else {
                        match parse_uint(&rem[2..], 16) {
                            None => {
                                self.is_valid = false;
                                return false;
                            }
                            Some(x) => (x.0, 2 + x.1),
                        }
                    }
                } else if b.len() >= 2 && b[0] == b'0' && is_ascii_digit(b[1]) {
                    match parse_uint(&rem[1..], 8) {
                        None => {
                            self.is_valid = false;
                            return false;
                        }
                        Some(x) => (x.0, 1 + x.1),
                    }
                } else {
                    match parse_uint(rem, 10) {
                        None => {
                            self.is_valid = false;
                            return false;
                        }
                        Some(x) => {
                            pure_dec += 1;
                            x
                        }
                    }
                };
            rem = &rem[n..];
            if rem.is_empty() {
                let bits = 32 - digits as u32 * 8;
                if val >= (1u64 << bits) {
                    self.is_valid = false;
                    return false;
                }
                ipv4 = (ipv4 << bits) | val;
                break;
            } else {
                if val > 255 || rem.as_bytes()[0] != b'.' {
                    self.is_valid = false;
                    return false;
                }
                ipv4 = (ipv4 << 8) | val;
                rem = &rem[1..];
                digits += 1;
            }
        }
        // 1–4 dot-separated parts are valid (last part may carry multiple octets).
        // Only fail if there is un-consumed input after the final part.
        if !rem.is_empty() {
            self.is_valid = false;
            return false;
        }
        if !(in_place && pure_dec == 4) {
            self.update_base_hostname(&serializers::ipv4(ipv4));
        }
        self.host_kind = HostKind::Ipv4;
        true
    }

    pub(crate) fn parse_ipv6(&mut self, input: &str) -> bool {
        if input.is_empty() {
            self.is_valid = false;
            return false;
        }
        let mut addr = [0u16; 8];
        let mut piece: i32 = 0;
        let mut compress: Option<i32> = None;
        let b = input.as_bytes();
        let mut pos = 0usize;
        if b[0] == b':' {
            if b.len() < 2 || b[1] != b':' {
                self.is_valid = false;
                return false;
            }
            pos += 2;
            piece += 1;
            compress = Some(piece);
        }
        while pos < b.len() {
            if piece == 8 {
                self.is_valid = false;
                return false;
            }
            if b[pos] == b':' {
                if compress.is_some() {
                    self.is_valid = false;
                    return false;
                }
                pos += 1;
                piece += 1;
                compress = Some(piece);
                continue;
            }
            let mut val: u16 = 0;
            let mut len = 0u32;
            while len < 4 && pos < b.len() && is_ascii_hex_digit(b[pos]) {
                val = val.wrapping_mul(16).wrapping_add(nibble(b[pos]) as u16);
                pos += 1;
                len += 1;
            }
            if pos < b.len() && b[pos] == b'.' {
                if len == 0 || piece > 6 {
                    self.is_valid = false;
                    return false;
                }
                pos -= len as usize;
                let mut seen = 0;
                while pos < b.len() {
                    let mut pv: Option<u16> = None;
                    if seen > 0 {
                        if b[pos] == b'.' && seen < 4 {
                            pos += 1;
                        } else {
                            self.is_valid = false;
                            return false;
                        }
                    }
                    if pos >= b.len() || !is_ascii_digit(b[pos]) {
                        self.is_valid = false;
                        return false;
                    }
                    while pos < b.len() && is_ascii_digit(b[pos]) {
                        let n = (b[pos] - b'0') as u16;
                        pv = Some(match pv {
                            None => n,
                            Some(0) => {
                                self.is_valid = false;
                                return false;
                            }
                            Some(p) => p * 10 + n,
                        });
                        if pv.unwrap() > 255 {
                            self.is_valid = false;
                            return false;
                        }
                        pos += 1;
                    }
                    addr[piece as usize] = addr[piece as usize]
                        .wrapping_mul(256)
                        .wrapping_add(pv.unwrap());
                    seen += 1;
                    if seen == 2 || seen == 4 {
                        piece += 1;
                    }
                }
                if seen != 4 {
                    self.is_valid = false;
                    return false;
                }
                break;
            }
            if pos < b.len() && b[pos] == b':' {
                pos += 1;
                if pos >= b.len() {
                    self.is_valid = false;
                    return false;
                }
            } else if pos < b.len() {
                self.is_valid = false;
                return false;
            }
            addr[piece as usize] = val;
            piece += 1;
        }
        if let Some(c) = compress {
            let mut sw = piece - c;
            piece = 7;
            while piece != 0 && sw > 0 {
                addr.swap(piece as usize, (c + sw - 1) as usize);
                piece -= 1;
                sw -= 1;
            }
        } else if piece != 8 {
            self.is_valid = false;
            return false;
        }
        self.update_base_hostname(&serializers::ipv6(&addr));
        self.host_kind = HostKind::Ipv6;
        true
    }

    pub(crate) fn parse_opaque_host(&mut self, input: &str) -> bool {
        if input.bytes().any(is_forbidden_host_code_point) {
            self.is_valid = false;
            return false;
        }
        let enc: Cow<'_, str> = percent_encode(input, &C0_CONTROL_PERCENT_ENCODE);
        self.update_base_hostname(&enc);
        true
    }
}

// ---------------------------------------------------------------------------
// Path parsing
// ---------------------------------------------------------------------------

impl Url {
    pub(crate) fn consume_prepared_path(&mut self, input: &str) {
        const NE: u8 = 1;
        const BS: u8 = 2;
        const DOT: u8 = 4;
        const PCT: u8 = 8;
        let acc = path_signature(input);
        let special = self.is_special();
        let may_slow = self.scheme == Scheme::File && is_windows_drive_letter(input);
        let mut trivial = (if special {
            acc == 0
        } else {
            (acc & (NE | DOT | PCT)) == 0
        }) && !may_slow;
        if acc == DOT && !may_slow && !input.is_empty() && input.as_bytes()[0] != b'.' {
            let mut sd = 0;
            let mut ok = true;
            loop {
                match input[sd..].find("/.") {
                    None => break,
                    Some(p) => {
                        sd += p + 2;
                        let r = &input[sd..];
                        ok &= !(r.is_empty() || r.starts_with('.') || r.starts_with('/'));
                    }
                }
            }
            trivial = ok;
        }
        if trivial && self.is_at_path() {
            self.buffer.push('/');
            self.buffer.push_str(input);
            return;
        }
        let mut new_path = if self.is_at_path() {
            String::new()
        } else {
            self.pathname().to_string()
        };
        let fast = special && (acc & (NE | BS | PCT)) == 0 && self.scheme != Scheme::File;
        if fast {
            path_fast(input, self.scheme, &mut new_path);
        } else {
            parse_prepared_path(input, self.scheme, &mut new_path);
        }
        self.update_base_pathname(&new_path);
    }

    pub(crate) fn parse_path(&mut self, input: &str) {
        let cleaned = strip_tabs_newlines(input);
        let s: &str = &cleaned;
        if self.is_special() {
            if s.is_empty() {
                self.update_base_pathname("/");
            } else {
                let f = s.as_bytes()[0];
                self.consume_prepared_path(if f == b'/' || f == b'\\' { &s[1..] } else { s });
            }
        } else if !s.is_empty() {
            self.consume_prepared_path(if s.as_bytes()[0] == b'/' { &s[1..] } else { s });
        } else if self.components.host_start == self.components.host_end && !self.has_authority() {
            self.update_base_pathname("/");
        }
    }
}

// ===========================================================================
// Free setter-implementation functions (avoid borrowck issues)
// ===========================================================================

fn set_protocol_impl(url: &mut Url, input: &str) -> bool {
    // CoW: borrow when no tabs/newlines (common path), own otherwise
    let v_cow = strip_tabs_newlines(input);
    let v: &str = &v_cow;
    if v.is_empty() {
        return true;
    }
    if !checkers::is_alpha(v.as_bytes()[0]) {
        return false;
    }

    // Find the end of scheme characters (alnum+/-/.)
    let end = v.bytes().position(|b| !is_alnum_plus(b)).unwrap_or(v.len());

    if end < v.len() && v.as_bytes()[end] == b':' {
        // Input already ends with ':' — pass directly
        return parse_scheme_override(url, &v[..end + 1]);
    }

    if end == v.len() {
        // Input has no ':' — the whole thing is the scheme. Append ':' via a
        // stack buffer so we avoid a heap allocation on this path too.
        let slen = end;
        if slen > 15 {
            return false;
        }
        let mut buf = [0u8; 64];
        buf[..slen].copy_from_slice(&v.as_bytes()[..slen]);
        buf[slen] = b':';
        // SAFETY: only valid ASCII bytes
        let with_colon = unsafe { core::str::from_utf8_unchecked(&buf[..slen + 1]) };
        return parse_scheme_override(url, with_colon);
    }

    false
}

fn parse_scheme_override(url: &mut Url, with_colon: &str) -> bool {
    let scheme_str = &with_colon[..with_colon.len() - 1];
    let slen = scheme_str.len();
    // Use a macro-like helper to avoid duplication between stack/heap paths
    let do_override = |url: &mut Url, lo_with_colon: &str, lo_scheme: &str| -> bool {
        let parsed = scheme::get_scheme_type(lo_scheme);
        if (parsed != Scheme::NotSpecial) != url.is_special() {
            return false;
        }
        if (url.has_credentials() || url.components.port != OMITTED) && parsed == Scheme::File {
            return false;
        }
        if url.scheme == Scheme::File && url.components.host_start == url.components.host_end {
            return false;
        }
        url.scheme = parsed;
        url.set_scheme_from_view_with_colon(lo_with_colon);
        let def = url.default_port();
        if def != 0 && url.components.port == def as u32 {
            url.clear_port();
        }
        true
    };
    if slen <= 63 {
        let mut buf = [0u8; 64];
        buf[..slen].copy_from_slice(scheme_str.as_bytes());
        unicode::to_lower_ascii(&mut buf[..slen]);
        buf[slen] = b':';
        // SAFETY: buf[..slen+1] contains only valid ASCII
        let lo_wc = unsafe { core::str::from_utf8_unchecked(&buf[..slen + 1]) };
        let lo_sc = unsafe { core::str::from_utf8_unchecked(&buf[..slen]) };
        do_override(url, lo_wc, lo_sc)
    } else {
        let mut lo = String::from(scheme_str);
        unicode::to_lower_ascii(unsafe { lo.as_bytes_mut() });
        let lo_sc = lo.clone();
        lo.push(':');
        do_override(url, &lo, &lo_sc)
    }
}

fn set_port_impl(url: &mut Url, input: &str) -> bool {
    if url.cannot_have_credentials_or_port() {
        return false;
    }
    if input.is_empty() {
        url.clear_port();
        return true;
    }
    let t_cow2 = strip_tabs_newlines(input);
    let t: &str = &t_cow2;
    if t.is_empty() {
        return true;
    }
    if !is_ascii_digit(t.as_bytes()[0]) {
        return false;
    }
    let end = t
        .as_bytes()
        .iter()
        .position(|&c| !is_ascii_digit(c))
        .unwrap_or(t.len());
    let prev = url.components.port;
    url.parse_port(&t[..end], false);
    if url.is_valid {
        return true;
    }
    url.update_base_port(prev);
    url.is_valid = true;
    false
}

fn set_hash_impl(url: &mut Url, input: &str) {
    if input.is_empty() {
        if url.components.hash_start != OMITTED {
            url.buffer.truncate(url.components.hash_start as usize);
            url.components.hash_start = OMITTED;
        }
        if url.has_opaque_path {
            let mut p = url.pathname().to_string();
            strip_trailing_spaces_from_opaque_path(&mut p);
            url.update_base_pathname(&p);
        }
        return;
    }
    let s = input.strip_prefix('#').unwrap_or(input);
    let c = strip_tabs_newlines(s);
    url.update_unencoded_base_hash(&c);
}

fn set_search_impl(url: &mut Url, input: &str) {
    if input.is_empty() {
        url.clear_search();
        if url.has_opaque_path {
            let mut p = url.pathname().to_string();
            strip_trailing_spaces_from_opaque_path(&mut p);
            url.update_base_pathname(&p);
        }
        return;
    }
    let s = input.strip_prefix('?').unwrap_or(input);
    let c = strip_tabs_newlines(s);
    let set = if url.is_special() {
        &SPECIAL_QUERY_PERCENT_ENCODE
    } else {
        &QUERY_PERCENT_ENCODE
    };
    url.update_base_search_with_encode(&c, set);
}

fn set_pathname_impl(url: &mut Url, input: &str) -> bool {
    if url.has_opaque_path {
        return false;
    }
    url.clear_pathname();
    url.parse_path(input);
    if url.pathname().starts_with("//") && !url.has_authority() && !url.has_dash_dot() {
        let ins = url.components.pathname_start as usize;
        url.buffer.insert_str(ins, "/.");
        url.components.pathname_start += 2;
        if url.components.search_start != OMITTED {
            url.components.search_start += 2;
        }
        if url.components.hash_start != OMITTED {
            url.components.hash_start += 2;
        }
    }
    true
}

fn set_host_or_hostname(url: &mut Url, input: &str, hostname_only: bool) -> bool {
    if url.has_opaque_path {
        return false;
    }
    let prev_host = url.hostname().to_string();
    let prev_port = url.components.port;
    let input = if let Some(p) = input.find('#') {
        &input[..p]
    } else {
        input
    };
    let hs = strip_tabs_newlines(input);
    let hv: &str = &hs;
    if url.scheme != Scheme::File {
        let (loc, found_colon, trimmed) = get_host_delimiter_location(url.is_special(), hv);
        if found_colon {
            if trimmed.is_empty() {
                return false;
            }
            if hostname_only {
                return false;
            }
            if !url.parse_host(trimmed) {
                url.update_base_hostname(&prev_host);
                url.update_base_port(prev_port);
                return false;
            }
            let pb = &hv[loc + 1..];
            if !pb.is_empty() {
                set_port_impl(url, pb);
            }
            return true;
        } else {
            if trimmed.is_empty() && url.is_special() {
                return false;
            }
            if trimmed.is_empty() && (url.has_credentials() || url.has_port()) {
                return false;
            }
            if trimmed.is_empty() && !url.is_special() {
                if url.has_hostname() {
                    url.clear_hostname();
                } else if url.has_dash_dot() {
                    url.add_authority_slashes_if_needed();
                    url.delete_dash_dot();
                }
                return true;
            }
            if !url.parse_host(trimmed) {
                url.update_base_hostname(&prev_host);
                url.update_base_port(prev_port);
                return false;
            }
            if url.has_dash_dot() {
                url.delete_dash_dot();
            }
            return true;
        }
    }
    let end = hv.find(['/', '\\', '?']).unwrap_or(hv.len());
    let nh = &hv[..end];
    if nh.is_empty() {
        url.clear_hostname();
    } else {
        if !url.parse_host(nh) {
            url.update_base_hostname(&prev_host);
            url.update_base_port(prev_port);
            return false;
        }
        if url.hostname() == "localhost" {
            url.clear_hostname();
        }
    }
    true
}

// ===========================================================================
// Tiny private helpers
// ===========================================================================

fn path_fast(input: &str, scheme: Scheme, path: &mut String) {
    let mut rem = input;
    loop {
        let (seg, rest, fin) = match rem.find('/') {
            None => (rem, "", true),
            Some(p) => (&rem[..p], &rem[p + 1..], false),
        };
        rem = rest;
        if seg == ".." {
            shorten_path(path, scheme);
            if fin {
                path.push('/');
            }
        } else if seg == "." {
            if fin {
                path.push('/');
            }
        } else {
            path.push('/');
            path.push_str(seg);
        }
        if fin {
            break;
        }
    }
}

fn fmt_port(port: u32) -> String {
    let mut s = String::with_capacity(7);
    s.push(':');
    let mut n = port;
    if n == 0 {
        s.push('0');
        return s;
    }
    let mut buf = [0u8; 10];
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    for k in (0..i).rev() {
        s.push(buf[k] as char);
    }
    s
}

fn parse_uint(s: &str, radix: u64) -> Option<(u64, usize)> {
    let b = s.as_bytes();
    if b.is_empty() {
        return None;
    }
    let mut v = 0u64;
    let mut c = 0usize;
    for &byte in b {
        let d = match radix {
            16 => match byte {
                b'0'..=b'9' => (byte - b'0') as u64,
                b'a'..=b'f' => (byte - b'a' + 10) as u64,
                b'A'..=b'F' => (byte - b'A' + 10) as u64,
                _ => break,
            },
            8 => match byte {
                b'0'..=b'7' => (byte - b'0') as u64,
                _ => break,
            },
            _ => match byte {
                b'0'..=b'9' => (byte - b'0') as u64,
                _ => break,
            },
        };
        v = v.checked_mul(radix)?.checked_add(d)?;
        c += 1;
    }
    if c == 0 { None } else { Some((v, c)) }
}

fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

// ===========================================================================
// Standard trait impls for Url
// ===========================================================================

impl Clone for Url {
    fn clone(&self) -> Self {
        Self {
            buffer: self.buffer.clone(),
            components: self.components.clone(),
            is_valid: self.is_valid,
            has_opaque_path: self.has_opaque_path,
            scheme: self.scheme,
            host_kind: self.host_kind,
        }
    }
}

impl PartialEq for Url {
    fn eq(&self, other: &Self) -> bool {
        self.href() == other.href()
    }
}
impl Eq for Url {}

impl PartialOrd for Url {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Url {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.href().cmp(other.href())
    }
}

impl hash::Hash for Url {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.href().hash(state);
    }
}

impl borrow::Borrow<str> for Url {
    fn borrow(&self) -> &str {
        self.href()
    }
}

impl AsRef<[u8]> for Url {
    fn as_ref(&self) -> &[u8] {
        self.href().as_bytes()
    }
}

impl AsRef<str> for Url {
    fn as_ref(&self) -> &str {
        self.href()
    }
}

impl ops::Deref for Url {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.href()
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.href())
    }
}

impl fmt::Debug for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Url")
            .field("href", &self.href())
            .field("components", &self.components())
            .finish()
    }
}

#[cfg(feature = "std")]
impl From<Url> for String {
    fn from(u: Url) -> Self {
        String::from(u.href())
    }
}

impl<'a> TryFrom<&'a str> for Url {
    type Error = ParseUrlError<&'a str>;
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Self::parse(value, None)
    }
}

#[cfg(feature = "std")]
impl TryFrom<String> for Url {
    type Error = ParseUrlError<String>;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value, None)
    }
}

#[cfg(feature = "std")]
impl<'a> TryFrom<&'a String> for Url {
    type Error = ParseUrlError<&'a String>;
    fn try_from(value: &'a String) -> Result<Self, Self::Error> {
        Self::parse(value, None)
    }
}

#[cfg(feature = "std")]
impl core::str::FromStr for Url {
    type Err = ParseUrlError<Box<str>>;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s, None).map_err(|ParseUrlError { input }| ParseUrlError {
            input: input.into(),
        })
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Url {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

#[cfg(all(feature = "serde", feature = "std"))]
impl<'de> serde::Deserialize<'de> for Url {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Url, D::Error> {
        use serde::de::{Error, Unexpected, Visitor};
        struct V;
        impl Visitor<'_> for V {
            type Value = Url;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a URL string")
            }
            fn visit_str<E: Error>(self, s: &str) -> Result<Url, E> {
                Url::parse(s, None).map_err(|e| {
                    E::invalid_value(Unexpected::Str(s), &std::format!("{e}").as_str())
                })
            }
        }
        d.deserialize_str(V)
    }
}

unsafe impl Send for Url {}
unsafe impl Sync for Url {}

// ===========================================================================
// Internal tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_parse() {
        let u = Url::parse("https://example.com/path?q=1#frag", None).unwrap();
        assert_eq!(u.protocol(), "https:");
        assert_eq!(u.hostname(), "example.com");
        assert_eq!(u.pathname(), "/path");
        assert_eq!(u.search(), "?q=1");
        assert_eq!(u.hash(), "#frag");
    }

    #[test]
    fn can_parse() {
        assert!(Url::can_parse("https://example.com", None));
        assert!(Url::can_parse("/p", Some("https://example.com")));
        assert!(!Url::can_parse("not a url", None));
    }
}
