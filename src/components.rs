//! Compact URL component metadata.

/// Sentinel stored in a component offset when that component is absent.
pub(crate) const OMITTED: u32 = u32::MAX;

/// Byte offsets into a URL's normalized serialization.
///
/// Offsets point at delimiters where useful: `protocol_end` is one byte after
/// `:`, while `search_start` and `hash_start` point at `?` and `#`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(C)]
pub struct Components {
    /// End of the protocol, including its trailing colon.
    pub protocol_end: u32,
    /// End of the username.
    pub username_end: u32,
    /// Start of the host.
    pub host_start: u32,
    /// End of the host.
    pub host_end: u32,
    /// Parsed numeric port, or the internal omitted sentinel.
    port: u32,
    /// Start of the pathname.
    pub pathname_start: u32,
    /// Start of the query delimiter, or the internal omitted sentinel.
    search_start: u32,
    /// Start of the fragment delimiter, or the internal omitted sentinel.
    hash_start: u32,
}

impl Components {
    /// Creates component metadata.
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        protocol_end: u32,
        username_end: u32,
        host_start: u32,
        host_end: u32,
        port: Option<u16>,
        pathname_start: u32,
        search_start: Option<u32>,
        hash_start: Option<u32>,
    ) -> Self {
        Self {
            protocol_end,
            username_end,
            host_start,
            host_end,
            port: match port {
                Some(value) => value as u32,
                None => OMITTED,
            },
            pathname_start,
            search_start: match search_start {
                Some(value) => value,
                None => OMITTED,
            },
            hash_start: match hash_start {
                Some(value) => value,
                None => OMITTED,
            },
        }
    }

    /// Returns the parsed non-default port.
    #[inline]
    #[must_use]
    pub const fn port(self) -> Option<u16> {
        if self.port == OMITTED {
            None
        } else {
            Some(self.port as u16)
        }
    }

    /// Returns the query delimiter offset.
    #[inline]
    #[must_use]
    pub const fn search_start(self) -> Option<u32> {
        if self.search_start == OMITTED {
            None
        } else {
            Some(self.search_start)
        }
    }

    /// Returns the fragment delimiter offset.
    #[inline]
    #[must_use]
    pub const fn hash_start(self) -> Option<u32> {
        if self.hash_start == OMITTED {
            None
        } else {
            Some(self.hash_start)
        }
    }

    pub(crate) fn validate(self, length: usize) -> bool {
        let Ok(length) = u32::try_from(length) else {
            return false;
        };
        if !(self.protocol_end <= self.username_end
            && self.username_end <= self.host_start
            && self.host_start <= self.host_end
            && self.host_end <= self.pathname_start
            && self.pathname_start <= length)
        {
            return false;
        }

        let hash = self.hash_start().unwrap_or(length);
        let search = self.search_start().unwrap_or(hash);
        self.pathname_start <= search && search <= hash && hash <= length
    }
}

/// Public URL component offsets, compatible with the historical `ada-url`
/// crate API.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UrlComponents {
    /// End of the protocol, including its trailing colon.
    pub protocol_end: u32,
    /// End of the username.
    pub username_end: u32,
    /// Start of the host, including the `@` delimiter when credentials exist.
    pub host_start: u32,
    /// End of the hostname, before an optional port.
    pub host_end: u32,
    /// Parsed non-default port.
    pub port: Option<u32>,
    /// Start of the pathname.
    pub pathname_start: Option<u32>,
    /// Start of the query delimiter.
    pub search_start: Option<u32>,
    /// Start of the fragment delimiter.
    pub hash_start: Option<u32>,
}

/// The normalized host representation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum HostType {
    /// A domain or opaque host.
    #[default]
    Domain,
    /// An IPv4 address.
    IPV4,
    /// An IPv6 address.
    IPV6,
}

/// A compact classification of known schemes.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum SchemeType {
    /// `http`
    Http,
    /// A non-special scheme.
    #[default]
    NotSpecial,
    /// `https`
    Https,
    /// `ws`
    Ws,
    /// `ftp`
    Ftp,
    /// `wss`
    Wss,
    /// `file`
    File,
}

impl SchemeType {
    #[inline]
    pub(crate) fn from_scheme(scheme: &str) -> Self {
        match scheme {
            "http" => Self::Http,
            "https" => Self::Https,
            "ws" => Self::Ws,
            "ftp" => Self::Ftp,
            "wss" => Self::Wss,
            "file" => Self::File,
            _ => Self::NotSpecial,
        }
    }

    /// Returns whether this is a WHATWG special scheme.
    #[inline]
    #[must_use]
    pub const fn is_special(self) -> bool {
        !matches!(self, Self::NotSpecial)
    }

    /// Returns the scheme's default port.
    #[inline]
    #[must_use]
    pub const fn default_port(self) -> Option<u16> {
        match self {
            Self::Ftp => Some(21),
            Self::Http | Self::Ws => Some(80),
            Self::Https | Self::Wss => Some(443),
            Self::File | Self::NotSpecial => None,
        }
    }
}
