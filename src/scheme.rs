//! URL scheme type definitions and utilities.

/// Scheme type matching the WHATWG URL standard special schemes.
/// Values match the Ada C++ implementation for ABI compatibility with tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SchemeType {
    Http = 0,
    NotSpecial = 1,
    Https = 2,
    Ws = 3,
    Ftp = 4,
    Wss = 5,
    File = 6,
}

impl SchemeType {
    /// Returns true if this is a "special" scheme per the WHATWG URL spec.
    #[inline]
    pub const fn is_special(self) -> bool {
        !matches!(self, SchemeType::NotSpecial)
    }

    /// Returns the default port for special schemes, or 0 for non-special.
    #[inline]
    pub const fn default_port(self) -> u16 {
        match self {
            SchemeType::Http | SchemeType::Ws => 80,
            SchemeType::Https | SchemeType::Wss => 443,
            SchemeType::Ftp => 21,
            SchemeType::File | SchemeType::NotSpecial => 0,
        }
    }
}

/// Dispatch table keyed by (first_byte, length) — avoids a full string
/// comparison for every scheme on the hot parse path.
///
/// Input must already be lowercased ASCII.
#[inline]
pub fn get_scheme_type(scheme: &str) -> SchemeType {
    let b = scheme.as_bytes();
    // Dispatch on (first_char, length) — two cheap ops vs 6 string comparisons.
    match (b.first().copied(), b.len()) {
        (Some(b'h'), 4) if b == b"http"  => SchemeType::Http,
        (Some(b'h'), 5) if b == b"https" => SchemeType::Https,
        (Some(b'w'), 2) if b == b"ws"    => SchemeType::Ws,
        (Some(b'w'), 3) if b == b"wss"   => SchemeType::Wss,
        (Some(b'f'), 3) if b == b"ftp"   => SchemeType::Ftp,
        (Some(b'f'), 4) if b == b"file"  => SchemeType::File,
        _                                 => SchemeType::NotSpecial,
    }
}

/// Returns true if `scheme` (without ':') is a special scheme (case-insensitive).
#[allow(dead_code)]
pub fn is_special(scheme: &str) -> bool {
    matches!(
        scheme,
        "http"
            | "https"
            | "ws"
            | "wss"
            | "ftp"
            | "file"
            | "HTTP"
            | "HTTPS"
            | "WS"
            | "WSS"
            | "FTP"
            | "FILE"
    ) || scheme.eq_ignore_ascii_case("http")
        || scheme.eq_ignore_ascii_case("https")
        || scheme.eq_ignore_ascii_case("ws")
        || scheme.eq_ignore_ascii_case("wss")
        || scheme.eq_ignore_ascii_case("ftp")
        || scheme.eq_ignore_ascii_case("file")
}
