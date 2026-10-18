//! Parse errors.

use core::fmt;

/// The reason URL construction failed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ParseErrorKind {
    /// The input is not a valid WHATWG URL.
    InvalidUrl,
    /// The supplied base URL is invalid.
    InvalidBase,
    /// The raw or normalized URL exceeds the configured limit.
    TooLong,
    /// A setter is not permitted for this URL.
    InvalidSetter,
}

/// An error returned while parsing or mutating a URL.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ParseError {
    kind: ParseErrorKind,
}

impl ParseError {
    pub(crate) const fn new(kind: ParseErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the error category.
    #[must_use]
    pub const fn kind(self) -> ParseErrorKind {
        self.kind
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self.kind {
            ParseErrorKind::InvalidUrl => "invalid URL",
            ParseErrorKind::InvalidBase => "invalid base URL",
            ParseErrorKind::TooLong => "URL exceeds the configured length limit",
            ParseErrorKind::InvalidSetter => "URL component cannot be changed",
        };
        formatter.write_str(message)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ParseError {}
