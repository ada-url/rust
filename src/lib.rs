//! A memory-safe, high-performance WHATWG URL parser.
//!
//! The primary type is [`Url`]. It stores one normalized serialization and
//! compact byte offsets, so component access is allocation-free.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

mod components;
mod encoding;
mod error;
mod fast_path;
mod idna;
mod search_params;
mod url;
#[cfg(feature = "url-pattern")]
mod url_pattern;

pub use components::{Components, HostType, SchemeType, UrlComponents};
pub use encoding::{PercentEncodeSet, percent_decode, percent_encode};
pub use error::{ParseError, ParseErrorKind};
pub use idna::{Idna, domain_to_ascii, domain_to_unicode};
pub use search_params::{
    UrlSearchParams, UrlSearchParamsEntry, UrlSearchParamsEntryIterator,
    UrlSearchParamsKeyIterator, UrlSearchParamsValueIterator,
};
#[cfg(feature = "std")]
pub use url::href_from_file;
pub use url::{Url, can_parse, get_max_input_length, parse, set_max_input_length};
#[cfg(feature = "url-pattern")]
pub use url_pattern::{
    RegexSyntax, UrlPattern, UrlPatternComponentResult, UrlPatternError, UrlPatternInit,
    UrlPatternMatchInput, UrlPatternOptions, UrlPatternResult,
};

use core::fmt;

/// Error type returned by [`Url::parse`].
#[derive(Debug, PartialEq, Eq)]
pub struct ParseUrlError<Input> {
    /// The invalid input that could not be parsed.
    pub input: Input,
}

impl<Input: fmt::Debug> fmt::Display for ParseUrlError<Input> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Invalid url: {:?}", self.input)
    }
}

#[cfg(feature = "std")]
impl<Input: fmt::Debug> std::error::Error for ParseUrlError<Input> {}
