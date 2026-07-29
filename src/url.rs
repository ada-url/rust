//! Single-buffer URL representation and public operations.

use alloc::{
    borrow::Cow,
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::{
    borrow::Borrow,
    fmt,
    hash::{Hash, Hasher},
    ops::Deref,
    str::FromStr,
    sync::atomic::{AtomicU32, Ordering},
};
#[cfg(feature = "std")]
use std::path::Path;

use memchr::{memchr, memchr2};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use url::{Host, Position, Url as BackendUrl};

use crate::{
    ParseUrlError,
    components::{Components, HostType, SchemeType, UrlComponents},
    error::{ParseError, ParseErrorKind},
    fast_path,
    search_params::UrlSearchParams,
};

const FLAG_OPAQUE_PATH: u8 = 1 << 0;
const FLAG_AUTHORITY: u8 = 1 << 1;

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
const SPECIAL_QUERY_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'\'');
const QUERY_ENCODE_SET: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'<').add(b'>');
const FRAGMENT_ENCODE_SET: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`');

static MAX_INPUT_LENGTH: AtomicU32 = AtomicU32::new(u32::MAX);

/// A parsed WHATWG URL backed by one normalized serialization.
#[derive(Clone)]
pub struct Url {
    components: Components,
    buffer: String,
    scheme_type: SchemeType,
    host_type: HostType,
    flags: u8,
}

impl Url {
    /// Parses `input`, optionally resolving it against a serialized base URL.
    pub fn parse<Input>(input: Input, base: Option<&str>) -> Result<Self, ParseUrlError<Input>>
    where
        Input: AsRef<str>,
    {
        let parsed = Self::parse_with_base(input.as_ref(), base);
        parsed.map_err(|_| ParseUrlError { input })
    }

    /// Parses `input`, optionally resolving it against an already parsed base.
    pub fn parse_with_url_base(input: &str, base: Option<&Self>) -> Result<Self, ParseError> {
        check_raw_length(input)?;

        if base.is_none()
            && input.len() <= 32
            && let Some(parsed) = fast_path::parse_bare_special(input)
        {
            return Self::from_fast_path(parsed);
        }
        if base.is_none() && might_be_explicit_file(input) {
            if let Some(parsed) = fast_path::parse_normalized_file(input) {
                return Self::from_fast_path(parsed);
            }
            if let Some(parsed) = parse_explicit_file(input, None) {
                return parsed;
            }
        }
        if let Some(parsed) = fast_path::parse(input) {
            return Self::from_fast_path(parsed);
        }
        let opaque_candidate = memchr(b':', input.as_bytes())
            .and_then(|colon| input.as_bytes().get(colon + 1))
            .is_some_and(|byte| *byte != b'/');
        if opaque_candidate && let Some(parsed) = fast_path::parse_opaque_absolute(input) {
            return Self::from_fast_path(parsed);
        }
        match fast_path::parse_common_absolute(input) {
            fast_path::CommonParse::Parsed(parsed) => return Self::from_fast_path(parsed),
            fast_path::CommonParse::Invalid => {
                return Err(ParseError::new(ParseErrorKind::InvalidUrl));
            }
            fast_path::CommonParse::Unsupported => {}
        }
        let parsed = if let Some(base) = base {
            if might_be_explicit_file(input)
                && let Some(parsed) = parse_explicit_file(input, Some(base))
            {
                return parsed;
            }
            if let Some(parsed) = resolve_simple_reference(input, base) {
                return parsed;
            }
            if let Some(parsed) = resolve_authority_reference(input, base) {
                return parsed;
            }
            if let Some(parsed) = resolve_normalized_file_relative(input, base) {
                return parsed;
            }
            if let Some(parsed) = resolve_file_reference(input, base) {
                return parsed;
            }
            if let Some(parsed) = resolve_non_special_backslash(input, base) {
                return parsed;
            }
            if let Some(parsed) = resolve_custom_file_base(input, base) {
                return parsed;
            }
            if let Some(parsed) = resolve_common_path_reference(input, base) {
                return parsed;
            }
            let mut backend_base = base
                .to_backend()
                .map_err(|_| ParseError::new(ParseErrorKind::InvalidBase))?;
            if let Some(parsed) = resolve_file_root(input, &mut backend_base) {
                return Self::from_backend(parsed);
            }
            let normalized_reference = normalize_special_authority_reference(input, base);
            let input = normalized_reference.as_deref().unwrap_or(input);
            let mut parsed = BackendUrl::options()
                .base_url(Some(&backend_base))
                .parse(input)
                .map_err(|_| ParseError::new(ParseErrorKind::InvalidUrl))?;
            normalize_non_file_drive_parent(input, base, &mut parsed);
            parsed
        } else {
            BackendUrl::parse(input).map_err(|_| ParseError::new(ParseErrorKind::InvalidUrl))?
        };
        Self::from_backend(parsed)
    }

    #[inline]
    fn from_fast_path(parsed: fast_path::FastPath) -> Result<Self, ParseError> {
        check_normalized_length(parsed.buffer.len())?;
        Ok(Self {
            components: parsed.components,
            buffer: parsed.buffer,
            scheme_type: parsed.scheme_type,
            host_type: parsed.host_type,
            flags: (u8::from(parsed.has_authority) * FLAG_AUTHORITY)
                | (u8::from(parsed.opaque_path) * FLAG_OPAQUE_PATH),
        })
    }

    /// Parses `input` with an optional base URL string.
    ///
    /// Unlike [`Self::parse`], this entry point also validates and parses the
    /// base string. It is convenient for Web-compatible constructor APIs.
    pub fn parse_with_base(input: &str, base: Option<&str>) -> Result<Self, ParseError> {
        let Some(base) = base else {
            return Self::parse_with_url_base(input, None);
        };
        check_raw_length(input)?;
        check_raw_length(base).map_err(|_| ParseError::new(ParseErrorKind::InvalidBase))?;
        let parsed_base = Self::parse_with_url_base(base, None)
            .map_err(|_| ParseError::new(ParseErrorKind::InvalidBase))?;
        if let Some(parsed) = parse_explicit_file(input, Some(&parsed_base)) {
            return parsed;
        }
        if let Some(parsed) = resolve_file_reference(input, &parsed_base) {
            return parsed;
        }
        if let Some(parsed) = resolve_non_special_backslash(input, &parsed_base) {
            return parsed;
        }
        if let Some(custom_base) = parse_absolute_file_with_drive_host(base) {
            let custom_base =
                custom_base.map_err(|_| ParseError::new(ParseErrorKind::InvalidBase))?;
            if let Some(parsed) = resolve_custom_file_base(input, &custom_base) {
                return parsed;
            }
        }
        let mut backend_base = parsed_base
            .to_backend()
            .map_err(|_| ParseError::new(ParseErrorKind::InvalidBase))?;
        if let Some(parsed) = resolve_file_root(input, &mut backend_base) {
            return Self::from_backend(parsed);
        }
        let normalized_reference = normalize_special_authority_reference(input, &parsed_base);
        let input = normalized_reference.as_deref().unwrap_or(input);
        let mut parsed = BackendUrl::options()
            .base_url(Some(&backend_base))
            .parse(input)
            .map_err(|_| ParseError::new(ParseErrorKind::InvalidUrl))?;
        normalize_non_file_drive_parent(input, &parsed_base, &mut parsed);
        Self::from_backend(parsed)
    }

    /// Returns whether `input` is parseable, optionally against a string base.
    #[must_use]
    pub fn can_parse(input: &str, base: Option<&str>) -> bool {
        let max_input_length = get_max_input_length();
        if input.len() > max_input_length as usize {
            return false;
        }
        if max_input_length == u32::MAX
            && base.is_none()
            && let Some(valid) = fast_path::can_parse_special_absolute(input)
        {
            return valid;
        }
        if base.is_none()
            && let Some(normalized_len) = fast_path::normalized_len(input)
        {
            return normalized_len <= max_input_length as usize;
        }

        Self::parse_with_base(input, base).is_ok()
    }

    fn from_backend(mut backend: BackendUrl) -> Result<Self, ParseError> {
        normalize_file_drive_letter(&mut backend);
        normalize_file_localhost(&mut backend);
        normalize_opaque_trailing_space(&mut backend);
        normalize_hierarchical_path_caret(&mut backend);
        let protocol_end = backend.scheme().len() + 1;
        let before_username = backend[..Position::BeforeUsername].len();
        let username_end = backend[..Position::AfterUsername].len();
        let host_start = backend[..Position::BeforeHost].len();
        let host_end = backend[..Position::AfterHost].len();
        let pathname_start = backend[..Position::BeforePath].len();
        let search_start = backend
            .query()
            .is_some()
            .then(|| backend[..Position::AfterPath].len());
        let hash_start = backend
            .fragment()
            .is_some()
            .then(|| backend[..Position::AfterQuery].len());
        let authority = before_username == protocol_end + 2;
        let scheme_type = SchemeType::from_scheme(backend.scheme());
        let host_type = match backend.host() {
            Some(Host::Ipv4(_)) => HostType::IPV4,
            Some(Host::Ipv6(_)) => HostType::IPV6,
            Some(Host::Domain(_)) | None => HostType::Domain,
        };
        let opaque_path = backend.cannot_be_a_base();
        let port = backend.port();
        let buffer = String::from(backend);
        check_normalized_length(buffer.len())?;

        let components = Components::new(
            to_u32(protocol_end)?,
            to_u32(username_end)?,
            to_u32(host_start)?,
            to_u32(host_end)?,
            port,
            to_u32(pathname_start)?,
            search_start.map(to_u32).transpose()?,
            hash_start.map(to_u32).transpose()?,
        );
        if !components.validate(buffer.len()) {
            return Err(ParseError::new(ParseErrorKind::InvalidUrl));
        }

        Ok(Self {
            components,
            buffer,
            scheme_type,
            host_type,
            flags: (u8::from(opaque_path) * FLAG_OPAQUE_PATH)
                | (u8::from(authority) * FLAG_AUTHORITY),
        })
    }

    fn from_file_parts(host: &str, host_type: HostType, suffix: &str) -> Result<Self, ParseError> {
        let suffix = if suffix.starts_with('/') {
            String::from(suffix)
        } else {
            format!("/{suffix}")
        };
        let mut buffer = String::with_capacity(7 + host.len() + suffix.len());
        buffer.push_str("file://");
        buffer.push_str(host);
        let pathname_start = buffer.len();
        buffer.push_str(&suffix);
        check_normalized_length(buffer.len())?;

        let search_start = memchr(b'?', suffix.as_bytes()).map(|offset| pathname_start + offset);
        let hash_start = memchr(b'#', suffix.as_bytes()).map(|offset| pathname_start + offset);
        let components = Components::new(
            5,
            7,
            7,
            to_u32(7 + host.len())?,
            None,
            to_u32(pathname_start)?,
            search_start.map(to_u32).transpose()?,
            hash_start.map(to_u32).transpose()?,
        );
        if !components.validate(buffer.len()) {
            return Err(ParseError::new(ParseErrorKind::InvalidUrl));
        }
        Ok(Self {
            components,
            buffer,
            scheme_type: SchemeType::File,
            host_type,
            flags: FLAG_AUTHORITY,
        })
    }

    #[inline]
    fn to_backend(&self) -> Result<BackendUrl, ParseError> {
        BackendUrl::parse(&self.buffer).map_err(|_| ParseError::new(ParseErrorKind::InvalidUrl))
    }

    fn replace_backend(&mut self, backend: BackendUrl) -> Result<(), ParseError> {
        let replacement = Self::from_backend(backend)?;
        *self = replacement;
        Ok(())
    }

    /// Replaces the entire URL. Failure leaves `self` unchanged.
    fn set_href_value(&mut self, input: &str) -> Result<(), ParseError> {
        let replacement = Self::parse_with_url_base(input, None)?;
        *self = replacement;
        Ok(())
    }

    /// Changes the scheme. Failure leaves `self` unchanged.
    fn set_protocol_value(&mut self, protocol: &str) -> Result<(), ParseError> {
        let cleaned = remove_ascii_tab_or_newline(protocol);
        let protocol = cleaned.as_ref();
        let mut backend = self.to_backend()?;
        let scheme = protocol.split(':').next().unwrap_or(protocol);
        backend
            .set_scheme(scheme)
            .map_err(|()| ParseError::new(ParseErrorKind::InvalidSetter))?;
        self.replace_backend(backend)
    }

    /// Changes the username. Failure leaves `self` unchanged.
    fn set_username_value(&mut self, username: &str) -> Result<(), ParseError> {
        let mut backend = self.to_backend()?;
        backend
            .set_username(username)
            .map_err(|()| ParseError::new(ParseErrorKind::InvalidSetter))?;
        self.replace_backend(backend)
    }

    /// Changes the password. Failure leaves `self` unchanged.
    fn set_password_value(&mut self, password: &str) -> Result<(), ParseError> {
        let mut backend = self.to_backend()?;
        backend
            .set_password(Some(password))
            .map_err(|()| ParseError::new(ParseErrorKind::InvalidSetter))?;
        self.replace_backend(backend)
    }

    /// Changes the host and optional port. Failure leaves `self` unchanged.
    fn set_host_value(&mut self, host: &str) -> Result<(), ParseError> {
        let cleaned = remove_ascii_tab_or_newline(host);
        let host = setter_host_prefix(cleaned.as_ref(), self.is_special());
        let (hostname, port) = split_host_port(host);
        if hostname.is_empty()
            && (self.has_credentials() || self.has_password() || !self.port().is_empty())
        {
            return Err(ParseError::new(ParseErrorKind::InvalidSetter));
        }
        if hostname.eq_ignore_ascii_case("xn--") && port.is_none() {
            return self.replace_hostname_serialized("xn--");
        }
        if !self.has_authority() && !self.has_opaque_path() {
            return self.install_authority(hostname, port);
        }
        let fallback_domain = self
            .is_special()
            .then(|| fast_path::normalize_special_host(hostname))
            .flatten()
            .and_then(|(hostname, host_type)| (host_type == HostType::Domain).then_some(hostname));
        let mut backend = match self.to_backend() {
            Ok(backend) => backend,
            Err(_) if fallback_domain.is_some() => {
                self.replace_hostname_serialized(fallback_domain.as_deref().unwrap_or(""))?;
                return port.map_or(Ok(()), |port| self.set_port_value(port));
            }
            Err(error) => return Err(error),
        };
        let hostname = if self.scheme_type == SchemeType::File && hostname.is_empty() {
            None
        } else {
            Some(hostname)
        };
        if backend.set_host(hostname).is_err() {
            let Some(normalized) = fallback_domain else {
                return Err(ParseError::new(ParseErrorKind::InvalidSetter));
            };
            self.replace_hostname_serialized(&normalized)?;
            return port.map_or(Ok(()), |port| self.set_port_value(port));
        }
        if let Some(port) = port {
            let digits = port
                .as_bytes()
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if digits == 0 {
                return self.replace_backend(backend);
            }
            let port = port[..digits].parse::<u16>().ok();
            backend
                .set_port(port)
                .map_err(|()| ParseError::new(ParseErrorKind::InvalidSetter))?;
        }
        self.replace_backend(backend)
    }

    /// Changes only the hostname, preserving the port.
    fn set_hostname_value(&mut self, hostname: &str) -> Result<(), ParseError> {
        let cleaned = remove_ascii_tab_or_newline(hostname);
        let hostname = setter_host_prefix(cleaned.as_ref(), self.is_special());
        let (hostname, port) = split_host_port(hostname);
        if port.is_some() {
            return Err(ParseError::new(ParseErrorKind::InvalidSetter));
        }
        if hostname.is_empty()
            && (self.has_credentials() || self.has_password() || !self.port().is_empty())
        {
            return Err(ParseError::new(ParseErrorKind::InvalidSetter));
        }
        if hostname.eq_ignore_ascii_case("xn--") {
            return self.replace_hostname_serialized("xn--");
        }
        if !self.has_authority() && !self.has_opaque_path() {
            return self.install_authority(hostname, None);
        }
        let fallback_domain = self
            .is_special()
            .then(|| fast_path::normalize_special_host(hostname))
            .flatten()
            .and_then(|(hostname, host_type)| (host_type == HostType::Domain).then_some(hostname));
        let mut backend = match self.to_backend() {
            Ok(backend) => backend,
            Err(_) if fallback_domain.is_some() => {
                return self.replace_hostname_serialized(fallback_domain.as_deref().unwrap_or(""));
            }
            Err(error) => return Err(error),
        };
        let hostname = if self.scheme_type == SchemeType::File && hostname.is_empty() {
            None
        } else {
            Some(hostname)
        };
        if backend.set_host(hostname).is_err() {
            let Some(normalized) = fallback_domain else {
                return Err(ParseError::new(ParseErrorKind::InvalidSetter));
            };
            return self.replace_hostname_serialized(&normalized);
        }
        self.replace_backend(backend)
    }

    fn replace_hostname_serialized(&mut self, hostname: &str) -> Result<(), ParseError> {
        let start = self.components.host_start as usize;
        let end = self.components.host_end as usize;
        let difference = isize::try_from(hostname.len())
            .ok()
            .and_then(|new| isize::try_from(end - start).ok().map(|old| new - old))
            .ok_or_else(|| ParseError::new(ParseErrorKind::TooLong))?;
        let mut buffer = self.buffer.clone();
        buffer.replace_range(start..end, hostname);
        check_normalized_length(buffer.len())?;

        let components = Components::new(
            self.components.protocol_end,
            self.components.username_end,
            self.components.host_start,
            shift_offset(self.components.host_end, difference)?,
            self.components.port(),
            shift_offset(self.components.pathname_start, difference)?,
            self.components
                .search_start()
                .map(|offset| shift_offset(offset, difference))
                .transpose()?,
            self.components
                .hash_start()
                .map(|offset| shift_offset(offset, difference))
                .transpose()?,
        );
        if !components.validate(buffer.len()) {
            return Err(ParseError::new(ParseErrorKind::InvalidSetter));
        }
        self.buffer = buffer;
        self.components = components;
        self.host_type = HostType::Domain;
        Ok(())
    }

    fn install_authority(&mut self, hostname: &str, port: Option<&str>) -> Result<(), ParseError> {
        let candidate = BackendUrl::parse(&format!("{}//{hostname}/", self.protocol()))
            .map_err(|_| ParseError::new(ParseErrorKind::InvalidSetter))?;
        let normalized_host = String::from(&candidate[Position::BeforeHost..Position::AfterHost]);
        let host_type = match candidate.host() {
            Some(Host::Ipv4(_)) => HostType::IPV4,
            Some(Host::Ipv6(_)) => HostType::IPV6,
            Some(Host::Domain(_)) => HostType::Domain,
            None => HostType::Domain,
        };

        let parsed_port = port.and_then(|port| {
            let digits = port
                .as_bytes()
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            (digits > 0)
                .then(|| port[..digits].parse::<u16>().ok())
                .flatten()
        });
        let mut buffer = String::new();
        buffer.push_str(self.protocol());
        buffer.push_str("//");
        buffer.push_str(&normalized_host);
        if let Some(port) =
            parsed_port.filter(|port| Some(*port) != self.scheme_type.default_port())
        {
            buffer.push(':');
            buffer.push_str(&port.to_string());
        }
        let pathname_start = buffer.len();
        buffer.push_str(self.pathname_and_later());
        check_normalized_length(buffer.len())?;

        let search_start =
            memchr(b'?', &buffer.as_bytes()[pathname_start..]).map(|index| pathname_start + index);
        let hash_start =
            memchr(b'#', &buffer.as_bytes()[pathname_start..]).map(|index| pathname_start + index);
        let host_start = self.components.protocol_end as usize + 2;
        let components = Components::new(
            self.components.protocol_end,
            to_u32(host_start)?,
            to_u32(host_start)?,
            to_u32(host_start + normalized_host.len())?,
            parsed_port.filter(|port| Some(*port) != self.scheme_type.default_port()),
            to_u32(pathname_start)?,
            search_start.map(to_u32).transpose()?,
            hash_start.map(to_u32).transpose()?,
        );
        if !components.validate(buffer.len()) {
            return Err(ParseError::new(ParseErrorKind::InvalidSetter));
        }
        self.buffer = buffer;
        self.components = components;
        self.host_type = host_type;
        self.flags |= FLAG_AUTHORITY;
        Ok(())
    }

    fn replace_non_special_path(&mut self, pathname: &str) -> Result<(), ParseError> {
        let pathname = parse_non_special_path(pathname);
        let mut buffer = String::from(self.protocol());
        if pathname.starts_with("//") {
            buffer.push_str("/.");
        }
        let pathname_start = buffer.len();
        buffer.push_str(&pathname);
        let later_start = self
            .components
            .search_start()
            .or(self.components.hash_start())
            .map_or(self.buffer.len(), |offset| offset as usize);
        buffer.push_str(&self.buffer[later_start..]);
        check_normalized_length(buffer.len())?;

        let search_start =
            memchr(b'?', &buffer.as_bytes()[pathname_start..]).map(|index| pathname_start + index);
        let hash_start =
            memchr(b'#', &buffer.as_bytes()[pathname_start..]).map(|index| pathname_start + index);
        let components = Components::new(
            self.components.protocol_end,
            self.components.protocol_end,
            self.components.protocol_end,
            self.components.protocol_end,
            None,
            to_u32(pathname_start)?,
            search_start.map(to_u32).transpose()?,
            hash_start.map(to_u32).transpose()?,
        );
        if !components.validate(buffer.len()) {
            return Err(ParseError::new(ParseErrorKind::InvalidSetter));
        }
        self.buffer = buffer;
        self.components = components;
        Ok(())
    }

    /// Changes or clears the port.
    fn set_port_value(&mut self, port: &str) -> Result<(), ParseError> {
        let was_empty = port.is_empty();
        let cleaned = remove_ascii_tab_or_newline(port);
        let port = cleaned.as_ref();
        let parsed = if port.is_empty() {
            if was_empty {
                None
            } else {
                return Err(ParseError::new(ParseErrorKind::InvalidSetter));
            }
        } else {
            let digits = port
                .as_bytes()
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if digits == 0 {
                return Err(ParseError::new(ParseErrorKind::InvalidSetter));
            }
            Some(
                port[..digits]
                    .parse::<u16>()
                    .map_err(|_| ParseError::new(ParseErrorKind::InvalidSetter))?,
            )
        };
        let mut backend = self.to_backend()?;
        backend
            .set_port(parsed)
            .map_err(|()| ParseError::new(ParseErrorKind::InvalidSetter))?;
        self.replace_backend(backend)
    }

    /// Changes the pathname.
    fn set_pathname_value(&mut self, pathname: &str) -> Result<(), ParseError> {
        if self.has_opaque_path() {
            return Err(ParseError::new(ParseErrorKind::InvalidSetter));
        }
        let cleaned = remove_ascii_tab_or_newline(pathname);
        let pathname = cleaned.as_ref();
        if self.scheme_type == SchemeType::File {
            let encoded = utf8_percent_encode(pathname, PATH_ENCODE_SET).to_string();
            let path_input = encoded
                .strip_prefix('/')
                .or_else(|| encoded.strip_prefix('\\'))
                .unwrap_or(&encoded);
            let mut suffix = parse_file_suffix(path_input, Vec::new());
            let later_start = self
                .components
                .search_start()
                .or(self.components.hash_start())
                .map_or(self.buffer.len(), |offset| offset as usize);
            suffix.push_str(&self.buffer[later_start..]);
            let replacement = Self::from_file_parts(self.hostname(), self.host_type, &suffix)?;
            *self = replacement;
            return Ok(());
        }
        let mut backend = self.to_backend()?;
        let pathname = if pathname.is_empty() && (!self.has_authority() || self.is_special()) {
            "/"
        } else {
            pathname
        };
        let pathname = utf8_percent_encode(pathname, PATH_ENCODE_SET).to_string();
        if self.scheme_type == SchemeType::NotSpecial && !self.has_authority() {
            return self.replace_non_special_path(&pathname);
        }
        backend.set_path(&pathname);
        self.replace_backend(backend)
    }

    /// Changes or clears the query. An empty input clears it.
    fn set_search_value(&mut self, search: &str) -> Result<(), ParseError> {
        let mut backend = self.to_backend()?;
        if search.is_empty() {
            backend.set_query(None);
        } else {
            backend.set_query(Some(search.strip_prefix('?').unwrap_or(search)));
        }
        self.replace_backend(backend)
    }

    /// Replaces the query from serialized URL search parameters.
    pub fn set_search_params(&mut self, params: &UrlSearchParams) -> Result<(), ParseError> {
        self.set_search_value(&params.to_string())
    }

    /// Changes or clears the fragment. An empty input clears it.
    fn set_hash_value(&mut self, hash: &str) -> Result<(), ParseError> {
        let mut backend = self.to_backend()?;
        if hash.is_empty() {
            backend.set_fragment(None);
        } else {
            backend.set_fragment(Some(hash.strip_prefix('#').unwrap_or(hash)));
        }
        self.replace_backend(backend)
    }

    /// Replaces the entire URL.
    #[allow(clippy::result_unit_err)]
    pub fn set_href(&mut self, input: &str) -> Result<(), ()> {
        self.set_href_value(input).map_err(|_| ())
    }

    /// Changes the scheme.
    #[allow(clippy::result_unit_err)]
    pub fn set_protocol(&mut self, input: &str) -> Result<(), ()> {
        self.set_protocol_value(input).map_err(|_| ())
    }

    /// Changes or clears the username.
    #[allow(clippy::result_unit_err)]
    pub fn set_username(&mut self, input: Option<&str>) -> Result<(), ()> {
        self.set_username_value(input.unwrap_or("")).map_err(|_| ())
    }

    /// Changes or clears the password.
    #[allow(clippy::result_unit_err)]
    pub fn set_password(&mut self, input: Option<&str>) -> Result<(), ()> {
        self.set_password_value(input.unwrap_or("")).map_err(|_| ())
    }

    /// Changes the host and optional port.
    #[allow(clippy::result_unit_err)]
    pub fn set_host(&mut self, input: Option<&str>) -> Result<(), ()> {
        self.set_host_value(input.unwrap_or("")).map_err(|_| ())
    }

    /// Changes only the hostname.
    #[allow(clippy::result_unit_err)]
    pub fn set_hostname(&mut self, input: Option<&str>) -> Result<(), ()> {
        self.set_hostname_value(input.unwrap_or("")).map_err(|_| ())
    }

    /// Changes or clears the port.
    #[allow(clippy::result_unit_err)]
    pub fn set_port(&mut self, input: Option<&str>) -> Result<(), ()> {
        self.set_port_value(input.unwrap_or("")).map_err(|_| ())
    }

    /// Changes the pathname.
    #[allow(clippy::result_unit_err)]
    pub fn set_pathname(&mut self, input: Option<&str>) -> Result<(), ()> {
        self.set_pathname_value(input.unwrap_or("")).map_err(|_| ())
    }

    /// Changes or clears the query.
    pub fn set_search(&mut self, input: Option<&str>) {
        let _ = self.set_search_value(input.unwrap_or(""));
    }

    /// Changes or clears the fragment.
    pub fn set_hash(&mut self, input: Option<&str>) {
        let _ = self.set_hash_value(input.unwrap_or(""));
    }

    /// Removes the port.
    pub fn clear_port(&mut self) {
        let _ = self.set_port_value("");
    }

    /// Removes the query.
    pub fn clear_search(&mut self) {
        let _ = self.set_search_value("");
    }

    /// Removes the fragment.
    pub fn clear_hash(&mut self) {
        let _ = self.set_hash_value("");
    }

    /// Returns the complete normalized serialization.
    #[inline]
    #[must_use]
    pub fn href(&self) -> &str {
        &self.buffer
    }

    /// Returns the complete normalized serialization.
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.href()
    }

    /// Returns the complete serialization's byte length.
    #[inline]
    #[must_use]
    pub fn href_size(&self) -> usize {
        self.buffer.len()
    }

    /// Returns the protocol including `:`.
    #[inline]
    #[must_use]
    pub fn protocol(&self) -> &str {
        &self.buffer[..self.components.protocol_end as usize]
    }

    /// Returns the scheme without `:`.
    #[inline]
    #[must_use]
    pub fn scheme(&self) -> &str {
        &self.protocol()[..self.protocol().len() - 1]
    }

    /// Returns the username.
    #[inline]
    #[must_use]
    pub fn username(&self) -> &str {
        if !self.has_authority() {
            return "";
        }
        let start = self.components.protocol_end as usize + 2;
        &self.buffer[start..self.components.username_end as usize]
    }

    /// Returns the password without `:`.
    #[inline]
    #[must_use]
    pub fn password(&self) -> &str {
        if !self.has_password() {
            return "";
        }
        let start = self.components.username_end as usize + 1;
        let end = self.components.host_start as usize - 1;
        &self.buffer[start..end]
    }

    /// Returns the host including an optional port.
    #[inline]
    #[must_use]
    pub fn host(&self) -> &str {
        if !self.has_authority() {
            return "";
        }
        &self.buffer[self.components.host_start as usize..self.components.pathname_start as usize]
    }

    /// Returns the hostname without a port.
    #[inline]
    #[must_use]
    pub fn hostname(&self) -> &str {
        if !self.has_authority() {
            return "";
        }
        &self.buffer[self.components.host_start as usize..self.components.host_end as usize]
    }

    /// Returns the explicit non-default port.
    #[inline]
    #[must_use]
    pub fn port(&self) -> &str {
        if self.components.port().is_none() {
            return "";
        }
        &self.buffer[self.components.host_end as usize + 1..self.components.pathname_start as usize]
    }

    /// Returns the pathname.
    #[inline]
    #[must_use]
    pub fn pathname(&self) -> &str {
        let end = self
            .components
            .search_start()
            .or(self.components.hash_start())
            .map_or(self.buffer.len(), |offset| offset as usize);
        &self.buffer[self.components.pathname_start as usize..end]
    }

    /// Returns the query including `?`, or an empty string.
    #[inline]
    #[must_use]
    pub fn search(&self) -> &str {
        let Some(start) = self.components.search_start() else {
            return "";
        };
        let end = self
            .components
            .hash_start()
            .map_or(self.buffer.len(), |offset| offset as usize);
        if start as usize + 1 == end {
            ""
        } else {
            &self.buffer[start as usize..end]
        }
    }

    /// Parses the query into an independent URL search-parameter collection.
    ///
    /// Call [`Self::set_search_params`] after mutation to publish changes back
    /// to this URL.
    #[must_use]
    pub fn search_params(&self) -> UrlSearchParams {
        UrlSearchParams::new(self.search())
    }

    /// Returns the fragment including `#`, or an empty string.
    #[inline]
    #[must_use]
    pub fn hash(&self) -> &str {
        let Some(start) = self.components.hash_start() else {
            return "";
        };
        if start as usize + 1 == self.buffer.len() {
            ""
        } else {
            &self.buffer[start as usize..]
        }
    }

    #[inline]
    fn pathname_and_later(&self) -> &str {
        &self.buffer[self.components.pathname_start as usize..]
    }

    /// Returns the component metadata.
    #[inline]
    #[must_use]
    pub fn components(&self) -> UrlComponents {
        let host_start = if self
            .components
            .host_start
            .checked_sub(1)
            .is_some_and(|index| self.buffer.as_bytes()[index as usize] == b'@')
        {
            self.components.host_start - 1
        } else {
            self.components.host_start
        };
        UrlComponents {
            protocol_end: self.components.protocol_end,
            username_end: self.components.username_end,
            host_start,
            host_end: self.components.host_end,
            port: self.components.port().map(u32::from),
            pathname_start: Some(self.components.pathname_start),
            search_start: self.components.search_start(),
            hash_start: self.components.hash_start(),
        }
    }

    /// Returns the host representation.
    #[inline]
    #[must_use]
    pub const fn host_type(&self) -> HostType {
        self.host_type
    }

    /// Returns the scheme classification.
    #[inline]
    #[must_use]
    pub const fn scheme_type(&self) -> SchemeType {
        self.scheme_type
    }

    /// Returns whether the URL uses a special scheme.
    #[inline]
    #[must_use]
    pub const fn is_special(&self) -> bool {
        self.scheme_type.is_special()
    }

    /// Returns whether this URL has an opaque path.
    #[inline]
    #[must_use]
    pub const fn has_opaque_path(&self) -> bool {
        self.flags & FLAG_OPAQUE_PATH != 0
    }

    /// Returns whether this URL has an authority section.
    #[inline]
    #[must_use]
    pub const fn has_authority(&self) -> bool {
        self.flags & FLAG_AUTHORITY != 0
    }

    /// Returns whether the URL contains a host, including an empty file host.
    #[inline]
    #[must_use]
    pub const fn has_hostname(&self) -> bool {
        self.has_authority()
    }

    /// Returns whether the URL has an empty host.
    #[inline]
    #[must_use]
    pub fn has_empty_hostname(&self) -> bool {
        self.has_authority() && self.hostname().is_empty()
    }

    /// Returns whether the URL has a non-empty username.
    #[inline]
    #[must_use]
    pub fn has_non_empty_username(&self) -> bool {
        !self.username().is_empty()
    }

    /// Returns whether the URL has a non-empty password.
    #[inline]
    #[must_use]
    pub fn has_non_empty_password(&self) -> bool {
        !self.password().is_empty()
    }

    /// Returns whether the URL has an explicit non-default port.
    #[inline]
    #[must_use]
    pub const fn has_port(&self) -> bool {
        self.components.port().is_some()
    }

    /// Returns whether a query is present, including an empty query.
    #[inline]
    #[must_use]
    pub const fn has_search(&self) -> bool {
        self.components.search_start().is_some()
    }

    /// Returns whether a fragment is present, including an empty fragment.
    #[inline]
    #[must_use]
    pub const fn has_hash(&self) -> bool {
        self.components.hash_start().is_some()
    }

    /// Returns whether a password field is present.
    #[inline]
    #[must_use]
    pub fn has_password(&self) -> bool {
        let index = self.components.username_end as usize;
        self.has_authority() && self.buffer.as_bytes().get(index) == Some(&b':')
    }

    /// Returns whether non-empty credentials are present.
    #[inline]
    #[must_use]
    pub fn has_credentials(&self) -> bool {
        !self.username().is_empty() || !self.password().is_empty()
    }

    /// Returns the serialized origin.
    #[must_use]
    pub fn origin(&self) -> String {
        match self.scheme_type {
            SchemeType::Ftp
            | SchemeType::Http
            | SchemeType::Https
            | SchemeType::Ws
            | SchemeType::Wss => format!("{}//{}", self.protocol(), self.host()),
            SchemeType::File => String::from("null"),
            SchemeType::NotSpecial => {
                if self.scheme() == "blob"
                    && let Ok(inner) = Self::parse_with_url_base(self.pathname(), None)
                    && matches!(inner.scheme_type, SchemeType::Http | SchemeType::Https)
                {
                    return inner.origin();
                }
                String::from("null")
            }
        }
    }

    /// Checks RFC 1034 DNS wire-length constraints.
    #[must_use]
    pub fn has_valid_domain(&self) -> bool {
        if self.host_type != HostType::Domain {
            return false;
        }
        let domain = self.hostname();
        if domain.is_empty()
            || if domain.ends_with('.') {
                domain.len() > 254
            } else {
                domain.len() > 253
            }
        {
            return false;
        }
        domain
            .strip_suffix('.')
            .unwrap_or(domain)
            .split('.')
            .all(|label| !label.is_empty() && label.len() <= 63)
    }

    /// Checks internal offset and serialization invariants.
    #[must_use]
    pub fn validate(&self) -> bool {
        self.components.validate(self.buffer.len())
            && self.protocol().ends_with(':')
            && self
                .components
                .search_start()
                .is_none_or(|index| self.buffer.as_bytes()[index as usize] == b'?')
            && self
                .components
                .hash_start()
                .is_none_or(|index| self.buffer.as_bytes()[index as usize] == b'#')
    }
}

impl fmt::Display for Url {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.buffer)
    }
}

impl fmt::Debug for Url {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Url")
            .field("href", &self.buffer)
            .field("components", &self.components)
            .field("scheme_type", &self.scheme_type)
            .field("host_type", &self.host_type)
            .field("opaque_path", &self.has_opaque_path())
            .finish()
    }
}

impl PartialEq for Url {
    fn eq(&self, other: &Self) -> bool {
        self.buffer == other.buffer
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
        self.buffer.cmp(&other.buffer)
    }
}

impl Hash for Url {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.buffer.hash(state);
    }
}

impl Deref for Url {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl AsRef<str> for Url {
    fn as_ref(&self) -> &str {
        &self.buffer
    }
}

impl AsRef<[u8]> for Url {
    fn as_ref(&self) -> &[u8] {
        self.buffer.as_bytes()
    }
}

impl Borrow<str> for Url {
    fn borrow(&self) -> &str {
        &self.buffer
    }
}

impl<'input> TryFrom<&'input str> for Url {
    type Error = ParseUrlError<&'input str>;

    fn try_from(input: &'input str) -> Result<Self, Self::Error> {
        Self::parse(input, None)
    }
}

impl TryFrom<String> for Url {
    type Error = ParseUrlError<String>;

    fn try_from(input: String) -> Result<Self, Self::Error> {
        Self::parse(input, None)
    }
}

impl<'input> TryFrom<&'input String> for Url {
    type Error = ParseUrlError<&'input String>;

    fn try_from(input: &'input String) -> Result<Self, Self::Error> {
        Self::parse(input, None)
    }
}

impl From<Url> for String {
    fn from(url: Url) -> Self {
        url.buffer
    }
}

impl FromStr for Url {
    type Err = ParseUrlError<Box<str>>;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input, None).map_err(|ParseUrlError { input }| ParseUrlError {
            input: input.into(),
        })
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Url {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.href())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Url {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let input = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::parse_with_url_base(&input, None).map_err(serde::de::Error::custom)
    }
}

/// Parses a URL.
pub fn parse<Input>(input: Input, base: Option<&str>) -> Result<Url, ParseUrlError<Input>>
where
    Input: AsRef<str>,
{
    Url::parse(input, base)
}

/// Returns whether a URL string can be parsed.
#[must_use]
pub fn can_parse(input: &str, base: Option<&str>) -> bool {
    Url::can_parse(input, base)
}

/// Sets the process-wide input and normalized-output length limit.
pub fn set_max_input_length(length: u32) {
    MAX_INPUT_LENGTH.store(length, Ordering::Relaxed);
}

/// Returns the process-wide URL length limit.
#[must_use]
pub fn get_max_input_length() -> u32 {
    MAX_INPUT_LENGTH.load(Ordering::Relaxed)
}

/// Converts an absolute filesystem path to a `file:` URL.
#[cfg(feature = "std")]
pub fn href_from_file(path: impl AsRef<Path>) -> Result<String, ParseError> {
    let backend = BackendUrl::from_file_path(path)
        .map_err(|()| ParseError::new(ParseErrorKind::InvalidUrl))?;
    check_normalized_length(backend.as_str().len())?;
    Ok(String::from(backend))
}

fn check_raw_length(input: &str) -> Result<(), ParseError> {
    if input.len() > get_max_input_length() as usize {
        Err(ParseError::new(ParseErrorKind::TooLong))
    } else {
        Ok(())
    }
}

fn check_normalized_length(length: usize) -> Result<(), ParseError> {
    if length > get_max_input_length() as usize {
        Err(ParseError::new(ParseErrorKind::TooLong))
    } else {
        Ok(())
    }
}

fn to_u32(value: usize) -> Result<u32, ParseError> {
    u32::try_from(value).map_err(|_| ParseError::new(ParseErrorKind::TooLong))
}

fn shift_offset(offset: u32, difference: isize) -> Result<u32, ParseError> {
    let difference =
        i32::try_from(difference).map_err(|_| ParseError::new(ParseErrorKind::TooLong))?;
    offset
        .checked_add_signed(difference)
        .ok_or_else(|| ParseError::new(ParseErrorKind::TooLong))
}

fn normalize_file_drive_letter(url: &mut BackendUrl) {
    if url.scheme() != "file" {
        return;
    }
    let path = url.path().as_bytes();
    if path.len() >= 3
        && path[0] == b'/'
        && path[1].is_ascii_alphabetic()
        && path[2] == b'|'
        && (path.len() == 3 || path[3] == b'/')
    {
        let mut normalized = String::from(url.path());
        normalized.replace_range(2..3, ":");
        url.set_path(&normalized);
    }
}

fn normalize_file_localhost(url: &mut BackendUrl) {
    if url.scheme() == "file" && url.host_str() == Some("localhost") {
        let _ = url.set_host(None);
    }
}

fn normalize_opaque_trailing_space(url: &mut BackendUrl) {
    if !url.cannot_be_a_base()
        || (!url.path().ends_with(' '))
        || (url.query().is_none() && url.fragment().is_none())
    {
        return;
    }
    let mut normalized = String::from(url.path());
    normalized.pop();
    normalized.push_str("%20");
    url.set_path(&normalized);
}

fn normalize_hierarchical_path_caret(url: &mut BackendUrl) {
    if url.cannot_be_a_base() || !url.path().contains('^') {
        return;
    }
    url.set_path(&url.path().replace('^', "%5E"));
}

fn normalize_non_file_drive_parent(input: &str, base: &Url, parsed: &mut BackendUrl) {
    if input.trim() != ".."
        || base.scheme_type == SchemeType::File
        || !base.pathname().ends_with(":/")
    {
        return;
    }
    let without_slash = &base.pathname()[..base.pathname().len() - 1];
    let Some(parent_end) = without_slash.rfind('/') else {
        return;
    };
    parsed.set_path(&base.pathname()[..=parent_end]);
}

fn normalize_special_authority_reference(input: &str, base: &Url) -> Option<String> {
    if !base.is_special() || base.scheme_type == SchemeType::File || has_url_scheme(input) {
        return None;
    }
    let separator_count = input
        .bytes()
        .take_while(|byte| matches!(byte, b'/' | b'\\'))
        .count();
    if separator_count < 2 || (separator_count == 2 && input.as_bytes()[..2] == *b"//") {
        return None;
    }
    let mut normalized = String::with_capacity(input.len() - separator_count + 2);
    normalized.push_str("//");
    normalized.push_str(&input[separator_count..]);
    Some(normalized)
}

fn split_host_port(host: &str) -> (&str, Option<&str>) {
    if host.starts_with('[') {
        let Some(closing) = host.find(']') else {
            return (host, None);
        };
        let after = &host[closing + 1..];
        if let Some(port) = after.strip_prefix(':') {
            (&host[..=closing], Some(port))
        } else {
            (host, None)
        }
    } else {
        host.find(':').map_or((host, None), |index| {
            (&host[..index], Some(&host[index + 1..]))
        })
    }
}

fn setter_host_prefix(input: &str, special: bool) -> &str {
    let end = input
        .bytes()
        .position(|byte| matches!(byte, b'/' | b'?' | b'#') || (special && byte == b'\\'))
        .unwrap_or(input.len());
    &input[..end]
}

fn remove_ascii_tab_or_newline(input: &str) -> Cow<'_, str> {
    if !input
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'\t' | b'\n' | b'\r'))
    {
        return Cow::Borrowed(input);
    }
    Cow::Owned(
        input
            .chars()
            .filter(|character| !matches!(character, '\t' | '\n' | '\r'))
            .collect(),
    )
}

fn resolve_non_special_backslash(input: &str, base: &Url) -> Option<Result<Url, ParseError>> {
    if base.scheme_type != SchemeType::NotSpecial
        || !base.has_authority()
        || !input.starts_with('\\')
        || has_url_scheme(input)
    {
        return None;
    }
    Some((|| {
        let (path, query, fragment) = split_path_query_fragment(input);
        let parent_end = base.pathname().rfind('/').map_or(0, |index| index + 1);
        let mut resolved_path = String::from(&base.pathname()[..parent_end]);
        resolved_path.push_str(path);
        let mut backend = base.to_backend()?;
        backend.set_path(&resolved_path);
        backend.set_query(query);
        backend.set_fragment(fragment);
        Url::from_backend(backend)
    })())
}

fn resolve_simple_reference(input: &str, base: &Url) -> Option<Result<Url, ParseError>> {
    if input
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'\t' | b'\n' | b'\r'))
    {
        return None;
    }
    if input.is_empty() {
        let mut resolved = base.clone();
        if let Some(hash_start) = resolved.components.hash_start() {
            resolved.buffer.truncate(hash_start as usize);
            resolved.components = Components::new(
                resolved.components.protocol_end,
                resolved.components.username_end,
                resolved.components.host_start,
                resolved.components.host_end,
                resolved.components.port(),
                resolved.components.pathname_start,
                resolved.components.search_start(),
                None,
            );
        }
        return Some(Ok(resolved));
    }
    if let Some(fragment) = input.strip_prefix('#') {
        return Some((|| {
            let mut buffer = base.buffer.clone();
            buffer.truncate(
                base.components
                    .hash_start()
                    .map_or(buffer.len(), |offset| offset as usize),
            );
            let hash_start = to_u32(buffer.len())?;
            buffer.push('#');
            buffer.extend(utf8_percent_encode(fragment, FRAGMENT_ENCODE_SET));
            check_normalized_length(buffer.len())?;
            let components = Components::new(
                base.components.protocol_end,
                base.components.username_end,
                base.components.host_start,
                base.components.host_end,
                base.components.port(),
                base.components.pathname_start,
                base.components.search_start(),
                Some(hash_start),
            );
            Ok(Url {
                components,
                buffer,
                scheme_type: base.scheme_type,
                host_type: base.host_type,
                flags: base.flags,
            })
        })());
    }
    let query_and_fragment = input.strip_prefix('?')?;
    Some((|| {
        let (query, fragment) = query_and_fragment
            .split_once('#')
            .map_or((query_and_fragment, None), |(query, fragment)| {
                (query, Some(fragment))
            });
        let mut buffer = base.buffer.clone();
        let suffix_start = base
            .components
            .search_start()
            .or_else(|| base.components.hash_start())
            .map_or(buffer.len(), |offset| offset as usize);
        buffer.truncate(suffix_start);
        let search_start = to_u32(buffer.len())?;
        buffer.push('?');
        let encode_set = if base.is_special() {
            SPECIAL_QUERY_ENCODE_SET
        } else {
            QUERY_ENCODE_SET
        };
        buffer.extend(utf8_percent_encode(query, encode_set));
        let hash_start = fragment
            .map(|fragment| {
                let start = to_u32(buffer.len())?;
                buffer.push('#');
                buffer.extend(utf8_percent_encode(fragment, FRAGMENT_ENCODE_SET));
                Ok(start)
            })
            .transpose()?;
        check_normalized_length(buffer.len())?;
        let components = Components::new(
            base.components.protocol_end,
            base.components.username_end,
            base.components.host_start,
            base.components.host_end,
            base.components.port(),
            base.components.pathname_start,
            Some(search_start),
            hash_start,
        );
        Ok(Url {
            components,
            buffer,
            scheme_type: base.scheme_type,
            host_type: base.host_type,
            flags: base.flags,
        })
    })())
}

fn resolve_authority_reference(input: &str, base: &Url) -> Option<Result<Url, ParseError>> {
    if !base.has_authority() || base.scheme_type == SchemeType::File || has_url_scheme(input) {
        return None;
    }
    let special = base.is_special();
    let mut leading = 0_usize;
    for byte in input.bytes() {
        if byte == b'/' || (special && byte == b'\\') {
            leading += 1;
        } else {
            break;
        }
    }
    if leading < 2 {
        return None;
    }

    Some({
        let tail = &input[leading..];
        let suffix_end = memchr2(b'?', b'#', tail.as_bytes()).unwrap_or(tail.len());
        let mut absolute = String::with_capacity(base.scheme().len() + 3 + input.len());
        absolute.push_str(base.scheme());
        absolute.push_str("://");
        if special && tail[..suffix_end].contains('\\') {
            absolute.extend(
                tail[..suffix_end]
                    .chars()
                    .map(|character| if character == '\\' { '/' } else { character }),
            );
        } else {
            absolute.push_str(&tail[..suffix_end]);
        }
        absolute.push_str(&tail[suffix_end..]);
        Url::parse_with_url_base(&absolute, None)
    })
}

fn resolve_common_path_reference(input: &str, base: &Url) -> Option<Result<Url, ParseError>> {
    if base.scheme_type == SchemeType::File
        || !base.has_authority()
        || has_url_scheme(input)
        || input.starts_with("//")
        || (base.is_special() && input.as_bytes().starts_with(b"\\\\"))
        || input
            .as_bytes()
            .iter()
            .any(|byte| matches!(byte, b'\t' | b'\n' | b'\r'))
        || input
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

    Some((|| {
        let (path, query, fragment) = split_path_query_fragment(input);
        let rooted = path.starts_with('/') || (base.is_special() && path.starts_with('\\'));
        let mut resolved_path = if rooted {
            String::from("/")
        } else {
            let parent_end = base.pathname().rfind('/').map_or(0, |index| index + 1);
            String::from(&base.pathname()[..parent_end])
        };
        let path = if rooted { &path[1..] } else { path };
        let mut input_segments = path
            .split(if base.is_special() {
                &['/', '\\'][..]
            } else {
                &['/'][..]
            })
            .peekable();
        let mut needs_separator = false;
        while let Some(segment) = input_segments.next() {
            if is_single_dot_segment(segment) {
                if input_segments.peek().is_none() && !resolved_path.ends_with('/') {
                    resolved_path.push('/');
                }
                continue;
            }
            if is_double_dot_segment(segment) {
                shorten_resolved_path(&mut resolved_path);
                needs_separator = false;
                if input_segments.peek().is_none() && !resolved_path.ends_with('/') {
                    resolved_path.push('/');
                }
                continue;
            }
            if needs_separator || !resolved_path.ends_with('/') {
                resolved_path.push('/');
            }
            resolved_path.extend(utf8_percent_encode(segment, PATH_ENCODE_SET));
            needs_separator = true;
        }

        let pathname_start = base.components.pathname_start as usize;
        let mut buffer =
            String::with_capacity(pathname_start + resolved_path.len() + input.len() + 8);
        buffer.push_str(&base.buffer[..pathname_start]);
        buffer.push_str(&resolved_path);
        let search_start = query
            .map(|query| {
                let start = to_u32(buffer.len())?;
                buffer.push('?');
                let encode_set = if base.is_special() {
                    SPECIAL_QUERY_ENCODE_SET
                } else {
                    QUERY_ENCODE_SET
                };
                buffer.extend(utf8_percent_encode(query, encode_set));
                Ok(start)
            })
            .transpose()?;
        let hash_start = fragment
            .map(|fragment| {
                let start = to_u32(buffer.len())?;
                buffer.push('#');
                buffer.extend(utf8_percent_encode(fragment, FRAGMENT_ENCODE_SET));
                Ok(start)
            })
            .transpose()?;
        check_normalized_length(buffer.len())?;
        let components = Components::new(
            base.components.protocol_end,
            base.components.username_end,
            base.components.host_start,
            base.components.host_end,
            base.components.port(),
            base.components.pathname_start,
            search_start,
            hash_start,
        );
        Ok(Url {
            components,
            buffer,
            scheme_type: base.scheme_type,
            host_type: base.host_type,
            flags: base.flags,
        })
    })())
}

fn shorten_resolved_path(path: &mut String) {
    if path == "/" {
        return;
    }
    let end = path.len().saturating_sub(usize::from(path.ends_with('/')));
    let parent_end = path.as_bytes()[..end]
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or(0, |index| index + 1);
    path.truncate(parent_end.max(1));
}

fn parse_explicit_file(input: &str, base: Option<&Url>) -> Option<Result<Url, ParseError>> {
    let cleaned;
    let input = if input
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'\t' | b'\n' | b'\r'))
    {
        cleaned = input
            .chars()
            .filter(|character| !matches!(character, '\t' | '\n' | '\r'))
            .collect::<String>();
        cleaned.as_str()
    } else {
        input
    };
    let input = input.trim_matches(|character: char| character <= '\u{20}');
    let (scheme, rest) = input.split_at_checked(5)?;
    if !scheme.eq_ignore_ascii_case("file:") {
        return None;
    }

    Some((|| {
        let file_base = base.filter(|base| base.scheme_type == SchemeType::File);
        let bytes = rest.as_bytes();
        let two_slashes = bytes.len() >= 2
            && matches!(bytes[0], b'/' | b'\\')
            && matches!(bytes[1], b'/' | b'\\');

        let (host, host_type, path_input, initial_segments) = if two_slashes {
            let authority = &rest[2..];
            let host_end = authority
                .bytes()
                .position(|byte| matches!(byte, b'/' | b'\\' | b'?' | b'#'))
                .unwrap_or(authority.len());
            let raw_host = &authority[..host_end];
            if is_windows_drive_letter(raw_host.as_bytes()) {
                (String::new(), HostType::Domain, authority, Vec::new())
            } else {
                let (host, host_type) = if raw_host.is_empty() {
                    (String::new(), HostType::Domain)
                } else {
                    normalize_file_host(raw_host)?
                };
                let remaining = &authority[host_end..];
                let path_input = remaining
                    .strip_prefix('/')
                    .or_else(|| remaining.strip_prefix('\\'))
                    .unwrap_or(remaining);
                (host, host_type, path_input, Vec::new())
            }
        } else if bytes
            .first()
            .is_some_and(|byte| matches!(byte, b'/' | b'\\'))
        {
            let path_input = &rest[1..];
            let (host, host_type) = file_base.map_or_else(
                || (String::new(), HostType::Domain),
                |base| (String::from(base.hostname()), base.host_type),
            );
            let mut initial = Vec::new();
            if !starts_with_windows_drive_segment(path_only(path_input).as_bytes())
                && let Some(base) = file_base
                && starts_with_windows_drive_path(base.pathname().as_bytes())
            {
                initial.push(String::from(&base.pathname()[1..3]));
            }
            (host, host_type, path_input, initial)
        } else if let Some(base) = file_base {
            let path = path_only(rest);
            if path.is_empty() {
                let suffix = suffix_with_preserved_file_path(rest, base);
                return Url::from_file_parts(base.hostname(), base.host_type, &suffix);
            }

            let mut initial = file_path_segments(base.pathname());
            if starts_with_windows_drive_segment(path.as_bytes()) {
                initial.clear();
            } else {
                shorten_file_path(&mut initial);
            }
            (String::from(base.hostname()), base.host_type, rest, initial)
        } else {
            (String::new(), HostType::Domain, rest, Vec::new())
        };

        let suffix = parse_file_suffix(path_input, initial_segments);
        Url::from_file_parts(&host, host_type, &suffix)
    })())
}

#[inline]
fn might_be_explicit_file(input: &str) -> bool {
    input
        .as_bytes()
        .first()
        .is_some_and(|byte| matches!(byte, b'f' | b'F' | 0x00..=0x20 | 0x7f))
}

fn resolve_file_reference(input: &str, base: &Url) -> Option<Result<Url, ParseError>> {
    if base.scheme_type != SchemeType::File || has_url_scheme(input) {
        return None;
    }
    let mut explicit = String::with_capacity(5 + input.len());
    explicit.push_str("file:");
    explicit.push_str(input);
    parse_explicit_file(&explicit, Some(base))
}

fn resolve_normalized_file_relative(input: &str, base: &Url) -> Option<Result<Url, ParseError>> {
    if base.scheme_type != SchemeType::File
        || input.is_empty()
        || input.starts_with(['/', '\\', '?', '#'])
        || has_url_scheme(input)
        || input
            .as_bytes()
            .iter()
            .any(|byte| matches!(byte, b'\t' | b'\n' | b'\r'))
        || input
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
    let (path, query, fragment) = split_path_query_fragment(input);
    if starts_with_windows_drive_segment(path.as_bytes())
        || path
            .split('/')
            .any(|segment| is_single_dot_segment(segment) || is_double_dot_segment(segment))
    {
        return None;
    }

    Some((|| {
        let prefix_end = base.components.pathname_start as usize;
        let parent_end = base.pathname().rfind('/').map_or(0, |index| index + 1);
        let mut buffer = String::with_capacity(prefix_end + parent_end + input.len() + 8);
        buffer.push_str(&base.buffer[..prefix_end]);
        buffer.push_str(&base.pathname()[..parent_end]);
        buffer.extend(utf8_percent_encode(path, PATH_ENCODE_SET));
        let search_start = query
            .map(|query| {
                let start = to_u32(buffer.len())?;
                buffer.push('?');
                buffer.extend(utf8_percent_encode(query, SPECIAL_QUERY_ENCODE_SET));
                Ok(start)
            })
            .transpose()?;
        let hash_start = fragment
            .map(|fragment| {
                let start = to_u32(buffer.len())?;
                buffer.push('#');
                buffer.extend(utf8_percent_encode(fragment, FRAGMENT_ENCODE_SET));
                Ok(start)
            })
            .transpose()?;
        check_normalized_length(buffer.len())?;
        Ok(Url {
            components: Components::new(
                base.components.protocol_end,
                base.components.username_end,
                base.components.host_start,
                base.components.host_end,
                base.components.port(),
                base.components.pathname_start,
                search_start,
                hash_start,
            ),
            buffer,
            scheme_type: base.scheme_type,
            host_type: base.host_type,
            flags: base.flags,
        })
    })())
}

fn has_url_scheme(input: &str) -> bool {
    if starts_with_windows_drive_segment(path_only(input).as_bytes()) {
        return false;
    }
    let mut bytes = input.bytes();
    if !bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic()) {
        return false;
    }
    for byte in bytes {
        match byte {
            b':' => return true,
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'+' | b'-' | b'.' => {}
            _ => return false,
        }
    }
    false
}

fn parse_file_suffix(input: &str, mut segments: Vec<String>) -> String {
    let (path, query, fragment) = split_path_query_fragment(input);
    let mut suffix = String::with_capacity(
        segments
            .iter()
            .map(|segment| segment.len() + 1)
            .sum::<usize>()
            + input.len()
            + 1,
    );
    for segment in segments.drain(..) {
        suffix.push('/');
        suffix.push_str(&segment);
    }
    let mut input_segments = path.split(['/', '\\']).peekable();
    while let Some(segment) = input_segments.next() {
        if is_single_dot_segment(segment) {
            if input_segments.peek().is_none() && !suffix.ends_with('/') {
                suffix.push('/');
            }
            continue;
        }
        if is_double_dot_segment(segment) {
            shorten_file_suffix(&mut suffix);
            if input_segments.peek().is_none() && !suffix.ends_with('/') {
                suffix.push('/');
            }
            continue;
        }

        suffix.push('/');
        if suffix.len() == 1 && is_windows_drive_letter(segment.as_bytes()) {
            suffix.push(char::from(segment.as_bytes()[0]));
            suffix.push(':');
        } else {
            suffix.extend(utf8_percent_encode(segment, PATH_ENCODE_SET));
        }
    }
    if suffix.is_empty() {
        suffix.push('/');
    }
    append_query_and_fragment(&mut suffix, query, fragment);
    suffix
}

fn shorten_file_suffix(suffix: &mut String) {
    if suffix.is_empty() || suffix == "/" {
        return;
    }
    let end = suffix
        .len()
        .saturating_sub(usize::from(suffix.ends_with('/')));
    if end == 3
        && suffix.as_bytes()[0] == b'/'
        && suffix.as_bytes()[1].is_ascii_alphabetic()
        && suffix.as_bytes()[2] == b':'
    {
        return;
    }
    let parent_end = suffix.as_bytes()[..end]
        .iter()
        .rposition(|byte| *byte == b'/')
        .unwrap_or(0);
    suffix.truncate(parent_end);
}

fn parse_non_special_path(input: &str) -> String {
    let input = input.strip_prefix('/').unwrap_or(input);
    let mut segments = Vec::new();
    let mut input_segments = input.split('/').peekable();
    while let Some(segment) = input_segments.next() {
        if is_single_dot_segment(segment) {
            continue;
        }
        if is_double_dot_segment(segment) {
            segments.pop();
            if input_segments.peek().is_none() {
                segments.push(String::new());
            }
            continue;
        }
        segments.push(String::from(segment));
    }
    let mut pathname = String::new();
    for segment in segments {
        pathname.push('/');
        pathname.push_str(&segment);
    }
    if pathname.is_empty() {
        pathname.push('/');
    }
    pathname
}

fn suffix_with_preserved_file_path(input: &str, base: &Url) -> String {
    let (_, query, fragment) = split_path_query_fragment(input);
    let mut suffix = String::from(base.pathname());
    if input.is_empty() {
        if let Some(start) = base.components.search_start() {
            let end = base
                .components
                .hash_start()
                .map_or(base.buffer.len(), |offset| offset as usize);
            suffix.push_str(&base.buffer[start as usize..end]);
        }
    } else if input.starts_with('#') {
        if let Some(start) = base.components.search_start() {
            let end = base
                .components
                .hash_start()
                .map_or(base.buffer.len(), |offset| offset as usize);
            suffix.push_str(&base.buffer[start as usize..end]);
        }
    } else if let Some(query) = query {
        suffix.push('?');
        suffix.extend(utf8_percent_encode(query, SPECIAL_QUERY_ENCODE_SET));
    }
    if let Some(fragment) = fragment {
        suffix.push('#');
        suffix.extend(utf8_percent_encode(fragment, FRAGMENT_ENCODE_SET));
    }
    suffix
}

fn append_query_and_fragment(suffix: &mut String, query: Option<&str>, fragment: Option<&str>) {
    if let Some(query) = query {
        suffix.push('?');
        suffix.extend(utf8_percent_encode(query, SPECIAL_QUERY_ENCODE_SET));
    }
    if let Some(fragment) = fragment {
        suffix.push('#');
        suffix.extend(utf8_percent_encode(fragment, FRAGMENT_ENCODE_SET));
    }
}

fn split_path_query_fragment(input: &str) -> (&str, Option<&str>, Option<&str>) {
    let (before_fragment, fragment) = input.find('#').map_or((input, None), |index| {
        (&input[..index], Some(&input[index + 1..]))
    });
    let (path, query) = before_fragment
        .find('?')
        .map_or((before_fragment, None), |index| {
            (
                &before_fragment[..index],
                Some(&before_fragment[index + 1..]),
            )
        });
    (path, query, fragment)
}

fn path_only(input: &str) -> &str {
    split_path_query_fragment(input).0
}

fn file_path_segments(pathname: &str) -> Vec<String> {
    pathname
        .strip_prefix('/')
        .unwrap_or(pathname)
        .split('/')
        .map(String::from)
        .collect()
}

fn shorten_file_path(segments: &mut Vec<String>) {
    if segments.len() == 1 && is_windows_drive_letter(segments[0].as_bytes()) {
        return;
    }
    segments.pop();
}

fn starts_with_windows_drive_segment(input: &[u8]) -> bool {
    input.len() >= 2
        && input[0].is_ascii_alphabetic()
        && matches!(input[1], b':' | b'|')
        && (input.len() == 2 || matches!(input[2], b'/' | b'\\' | b'?' | b'#'))
}

fn is_single_dot_segment(segment: &str) -> bool {
    segment == "." || segment.eq_ignore_ascii_case("%2e")
}

fn is_double_dot_segment(segment: &str) -> bool {
    segment == ".."
        || segment.eq_ignore_ascii_case(".%2e")
        || segment.eq_ignore_ascii_case("%2e.")
        || segment.eq_ignore_ascii_case("%2e%2e")
}

fn parse_absolute_file_with_drive_host(input: &str) -> Option<Result<Url, ParseError>> {
    let input = input.trim_matches(|character: char| character <= '\u{20}');
    let (scheme, after_scheme) = input.split_at_checked(5)?;
    if !scheme.eq_ignore_ascii_case("file:") {
        return None;
    }
    let bytes = after_scheme.as_bytes();
    if bytes.len() < 3 || !matches!(bytes[0], b'/' | b'\\') || !matches!(bytes[1], b'/' | b'\\') {
        return None;
    }

    let authority = &after_scheme[2..];
    let host_end = authority
        .bytes()
        .position(|byte| matches!(byte, b'/' | b'\\' | b'?' | b'#'))
        .unwrap_or(authority.len());
    let raw_host = &authority[..host_end];
    let suffix = &authority[host_end..];
    let path = suffix.as_bytes();
    if raw_host.is_empty()
        || raw_host.eq_ignore_ascii_case("localhost")
        || is_windows_drive_letter(raw_host.as_bytes())
        || path.len() < 3
        || !matches!(path[0], b'/' | b'\\')
        || !path[1].is_ascii_alphabetic()
        || !matches!(path[2], b':' | b'|')
        || (path.len() > 3 && !matches!(path[3], b'/' | b'\\' | b'?' | b'#'))
    {
        return None;
    }

    Some((|| {
        let (host, host_type) = normalize_file_host(raw_host)?;
        let dummy = format!("file://{suffix}");
        let mut parsed =
            BackendUrl::parse(&dummy).map_err(|_| ParseError::new(ParseErrorKind::InvalidUrl))?;
        normalize_file_drive_letter(&mut parsed);
        Url::from_file_parts(
            &host,
            host_type,
            &parsed[Position::BeforePath..Position::AfterFragment],
        )
    })())
}

fn resolve_custom_file_base(input: &str, base: &Url) -> Option<Result<Url, ParseError>> {
    if base.scheme_type != SchemeType::File
        || base.hostname().is_empty()
        || !starts_with_windows_drive_path(base.pathname().as_bytes())
    {
        return None;
    }

    if let Some(absolute) = parse_absolute_file_with_drive_host(input) {
        return Some(absolute);
    }

    Some((|| {
        let dummy_base = BackendUrl::parse(&format!("file://{}", base.pathname_and_later()))
            .map_err(|_| ParseError::new(ParseErrorKind::InvalidBase))?;
        let mut parsed = BackendUrl::options()
            .base_url(Some(&dummy_base))
            .parse(input)
            .map_err(|_| ParseError::new(ParseErrorKind::InvalidUrl))?;
        normalize_file_drive_letter(&mut parsed);

        if parsed.scheme() != "file" || input_has_authority(input) {
            return Url::from_backend(parsed);
        }

        let suffix = if input_path(input) == "/" {
            let root = &base.pathname()[..4];
            format!(
                "{}{}",
                root,
                &parsed[Position::AfterPath..Position::AfterFragment]
            )
        } else {
            String::from(&parsed[Position::BeforePath..Position::AfterFragment])
        };
        Url::from_file_parts(base.hostname(), base.host_type, &suffix)
    })())
}

fn normalize_file_host(raw_host: &str) -> Result<(String, HostType), ParseError> {
    if raw_host.eq_ignore_ascii_case("localhost") {
        return Ok((String::new(), HostType::Domain));
    }
    if raw_host.eq_ignore_ascii_case("xn--") {
        return Ok((raw_host.to_ascii_lowercase(), HostType::Domain));
    }
    let (normalized, host_type) = fast_path::normalize_special_host(raw_host)
        .ok_or_else(|| ParseError::new(ParseErrorKind::InvalidUrl))?;
    if normalized.eq_ignore_ascii_case("localhost") {
        return Ok((String::new(), HostType::Domain));
    }
    Ok((normalized, host_type))
}

fn starts_with_windows_drive_path(path: &[u8]) -> bool {
    path.len() >= 3
        && path[0] == b'/'
        && path[1].is_ascii_alphabetic()
        && path[2] == b':'
        && (path.len() == 3 || path[3] == b'/')
}

fn is_windows_drive_letter(input: &[u8]) -> bool {
    input.len() == 2 && input[0].is_ascii_alphabetic() && matches!(input[1], b':' | b'|')
}

fn input_has_authority(input: &str) -> bool {
    let input = input.trim_start_matches(|character: char| character <= '\u{20}');
    let after_scheme = input
        .get(..5)
        .filter(|scheme| scheme.eq_ignore_ascii_case("file:"))
        .map_or(input, |_| &input[5..]);
    let bytes = after_scheme.as_bytes();
    bytes.len() >= 2 && matches!(bytes[0], b'/' | b'\\') && matches!(bytes[1], b'/' | b'\\')
}

fn input_path(input: &str) -> &str {
    let end = memchr2(b'?', b'#', input.as_bytes()).unwrap_or(input.len());
    &input[..end]
}

fn resolve_file_root(input: &str, base: &mut BackendUrl) -> Option<BackendUrl> {
    if input != "/" || base.scheme() != "file" {
        return None;
    }
    let path = base.path().as_bytes();
    let root = if path.len() >= 4
        && path[0] == b'/'
        && path[1].is_ascii_alphabetic()
        && path[2] == b':'
        && path[3] == b'/'
    {
        &base.path()[..4]
    } else {
        "/"
    };
    let root = String::from(root);
    base.set_path(&root);
    base.set_query(None);
    base.set_fragment(None);
    Some(base.clone())
}

#[cfg(test)]
mod tests {
    use super::{ParseErrorKind, Url, get_max_input_length, set_max_input_length};
    use crate::{HostType, SchemeType};
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn components_are_borrowed_from_one_buffer() {
        let _guard = TEST_LOCK.lock().unwrap();
        let url = Url::parse_with_url_base("https://user:pass@Example.com:8443/a/b?q=1#frag", None)
            .unwrap();
        assert_eq!(
            url.href(),
            "https://user:pass@example.com:8443/a/b?q=1#frag"
        );
        assert_eq!(url.protocol(), "https:");
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.username(), "user");
        assert_eq!(url.password(), "pass");
        assert_eq!(url.host(), "example.com:8443");
        assert_eq!(url.hostname(), "example.com");
        assert_eq!(url.port(), "8443");
        assert_eq!(url.pathname(), "/a/b");
        assert_eq!(url.search(), "?q=1");
        assert_eq!(url.hash(), "#frag");
        assert_eq!(url.scheme_type(), SchemeType::Https);
        assert_eq!(url.host_type(), HostType::Domain);
        assert!(url.validate());
    }

    #[test]
    fn resolves_relative_urls() {
        let _guard = TEST_LOCK.lock().unwrap();
        let base = Url::parse_with_url_base("https://example.com/a/b/", None).unwrap();
        let url = Url::parse_with_url_base("../c?q", Some(&base)).unwrap();
        assert_eq!(url.href(), "https://example.com/a/c?q");
    }

    #[test]
    fn handles_ip_and_opaque_urls() {
        let _guard = TEST_LOCK.lock().unwrap();
        let ipv4 = Url::parse_with_url_base("http://0x7f.1/", None).unwrap();
        assert_eq!(ipv4.hostname(), "127.0.0.1");
        assert_eq!(ipv4.host_type(), HostType::IPV4);

        let ipv6 = Url::parse_with_url_base("http://[2001:db8::1]/", None).unwrap();
        assert_eq!(ipv6.host_type(), HostType::IPV6);

        let opaque = Url::parse_with_url_base("mailto:user@example.com", None).unwrap();
        assert!(opaque.has_opaque_path());
        assert_eq!(opaque.pathname(), "user@example.com");
        assert_eq!(opaque.hostname(), "");
    }

    #[test]
    fn setters_are_transactional() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut url =
            Url::parse_with_url_base("https://user:pass@example.com/a?q#f", None).unwrap();
        url.set_host(Some("example.org:8080")).unwrap();
        assert_eq!(url.href(), "https://user:pass@example.org:8080/a?q#f");
        url.set_pathname(Some("/x y")).unwrap();
        url.set_search(Some("?a=b c"));
        url.set_hash(Some("#snow ☃"));
        assert_eq!(
            url.href(),
            "https://user:pass@example.org:8080/x%20y?a=b%20c#snow%20%E2%98%83"
        );

        let before = url.clone();
        assert!(url.set_port(Some("99999")).is_err());
        assert_eq!(url, before);
    }

    #[test]
    fn enforces_normalized_length() {
        let _guard = TEST_LOCK.lock().unwrap();
        let old = get_max_input_length();
        set_max_input_length(19);
        let error = Url::parse_with_url_base("https://example.com", None).unwrap_err();
        assert_eq!(error.kind(), ParseErrorKind::TooLong);
        assert!(!Url::can_parse("https://example.com", None));
        set_max_input_length(old);
    }
}
