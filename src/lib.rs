//! # Ada URL
//!
//! Ada is a fast and spec-compliant URL parser written in C++.
//! - It's widely tested by both Web Platform Tests and Google OSS Fuzzer.
//! - It is extremely fast.
//! - It's the default URL parser of Node.js since Node 18.16.0.
//! - It supports Unicode Technical Standard.
//!
//! The Ada library passes the full range of tests from the specification, across a wide range
//! of platforms (e.g., Windows, Linux, macOS).
//!
//! ## Performance
//!
//! Ada is extremely fast.
//! For more information read our [benchmark page](https://ada-url.com/docs/performance).
//!
//! ```text
//!      ada  ▏  188 ns/URL ███▏
//! servo url ▏  664 ns/URL ███████████▎
//!     CURL  ▏ 1471 ns/URL █████████████████████████
//! ```
//!
//! # Feature: `serde`
//!
//! If you enable the `serde` feature, [`Url`](struct.Url.html) will implement
//! [`serde::Serialize`](https://docs.rs/serde/1/serde/trait.Serialize.html) and
//! [`serde::Deserialize`](https://docs.rs/serde/1/serde/trait.Deserialize.html).
//! See [serde documentation](https://serde.rs) for more information.
//!
//! ```toml
//! ada-url = { version = "1", features = ["serde"] }
//! ```

pub mod ffi;
mod idna;
pub use idna::Idna;

use std::{borrow, fmt, hash, ops};
use thiserror::Error;

#[cfg(feature = "serde")]
extern crate serde;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid url: \"{0}\"")]
    ParseUrl(String),
}

/// A parsed URL struct according to WHATWG URL specification.
#[derive(Eq, Clone)]
pub struct Url {
    url: *mut ffi::ada_url,
}

impl Drop for Url {
    fn drop(&mut self) {
        unsafe { ffi::ada_free(self.url) }
    }
}

impl Url {
    /// Parses the input with an optional base
    ///
    /// ```
    /// use ada_url::Url;
    /// let out = Url::parse("https://ada-url.github.io/ada", None)
    ///     .expect("This is a valid URL. Should have parsed it.");
    /// assert_eq!(out.protocol(), "https:");
    /// ```
    pub fn parse(input: &str, base: Option<&str>) -> Result<Url, Error> {
        let url_aggregator = match base {
            Some(base) => unsafe {
                ffi::ada_parse_with_base(
                    input.as_ptr().cast(),
                    input.len(),
                    base.as_ptr().cast(),
                    base.len(),
                )
            },
            None => unsafe { ffi::ada_parse(input.as_ptr().cast(), input.len()) },
        };

        if unsafe { ffi::ada_is_valid(url_aggregator) } {
            Ok(Url {
                url: url_aggregator,
            })
        } else {
            Err(Error::ParseUrl(input.to_owned()))
        }
    }

    /// Returns whether or not the URL can be parsed or not.
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-canparse)
    ///
    /// ```
    /// use ada_url::Url;
    /// assert!(Url::can_parse("https://ada-url.github.io/ada", None));
    /// assert!(Url::can_parse("/pathname", Some("https://ada-url.github.io/ada")));
    /// ```
    pub fn can_parse(input: &str, base: Option<&str>) -> bool {
        unsafe {
            if let Some(base) = base {
                ffi::ada_can_parse_with_base(
                    input.as_ptr().cast(),
                    input.len(),
                    base.as_ptr().cast(),
                    base.len(),
                )
            } else {
                ffi::ada_can_parse(input.as_ptr().cast(), input.len())
            }
        }
    }

    /// Return the origin of this URL
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-origin)
    ///
    /// ```
    /// use ada_url::Url;
    ///
    /// let mut url = Url::parse("blob:https://example.com/foo", None).expect("Invalid URL");
    /// assert_eq!(url.origin(), "https://example.com");
    /// ```
    pub fn origin(&mut self) -> &str {
        unsafe {
            let out = ffi::ada_get_origin(self.url);
            let slice = std::slice::from_raw_parts(out.data.cast(), out.length);
            std::str::from_utf8_unchecked(slice)
        }
    }

    /// Return the parsed version of the URL with all components.
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-href)
    pub fn href(&self) -> &str {
        unsafe { ffi::ada_get_href(self.url) }.as_str()
    }

    pub fn set_href(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_href(self.url, input.as_ptr().cast(), input.len()) }
    }

    /// Return the username for this URL as a percent-encoded ASCII string.
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-username)
    ///
    /// ```
    /// use ada_url::Url;
    ///
    /// let url = Url::parse("ftp://rms:secret123@example.com", None).expect("Invalid URL");
    /// assert_eq!(url.username(), "rms");
    /// ```
    pub fn username(&self) -> &str {
        unsafe { ffi::ada_get_username(self.url) }.as_str()
    }

    pub fn set_username(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_username(self.url, input.as_ptr().cast(), input.len()) }
    }

    /// Return the password for this URL, if any, as a percent-encoded ASCII string.
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-password)
    ///
    /// ```
    /// use ada_url::Url;
    ///
    /// let url = Url::parse("ftp://rms:secret123@example.com", None).expect("Invalid URL");
    /// assert_eq!(url.password(), "secret123");
    /// ```
    pub fn password(&self) -> &str {
        unsafe { ffi::ada_get_password(self.url) }.as_str()
    }

    pub fn set_password(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_password(self.url, input.as_ptr().cast(), input.len()) }
    }

    /// Return the port number for this URL, or an empty string.
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-port)
    ///
    /// ```
    /// use ada_url::Url;
    ///
    /// let url = Url::parse("https://example.com", None).expect("Invalid URL");
    /// assert_eq!(url.port(), "");
    ///
    /// let url = Url::parse("https://example.com:8080", None).expect("Invalid URL");
    /// assert_eq!(url.port(), "8080");
    /// ```
    pub fn port(&self) -> &str {
        unsafe { ffi::ada_get_port(self.url) }.as_str()
    }

    pub fn set_port(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_port(self.url, input.as_ptr().cast(), input.len()) }
    }

    /// Return this URL’s fragment identifier, or an empty string.
    /// A fragment is the part of the URL with the # symbol.
    /// The fragment is optional and, if present, contains a fragment identifier that identifies
    /// a secondary resource, such as a section heading of a document.
    /// In HTML, the fragment identifier is usually the id attribute of a an element that is
    /// scrolled to on load. Browsers typically will not send the fragment portion of a URL to the
    /// server.
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-hash)
    ///
    /// ```
    /// use ada_url::Url;
    ///
    /// let url = Url::parse("https://example.com/data.csv#row=4", None).expect("Invalid URL");
    /// assert_eq!(url.hash(), "#row=4");
    /// assert!(url.has_hash());
    /// ```
    pub fn hash(&self) -> &str {
        unsafe { ffi::ada_get_hash(self.url) }.as_str()
    }

    pub fn set_hash(&mut self, input: &str) {
        unsafe { ffi::ada_set_hash(self.url, input.as_ptr().cast(), input.len()) }
    }

    /// Return the parsed representation of the host for this URL with an optional port number.
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-host)
    ///
    /// ```
    /// use ada_url::Url;
    ///
    /// let url = Url::parse("https://127.0.0.1:8080/index.html", None).expect("Invalid URL");
    /// assert_eq!(url.host(), "127.0.0.1:8080");
    /// ```
    pub fn host(&self) -> &str {
        unsafe { ffi::ada_get_host(self.url) }.as_str()
    }

    pub fn set_host(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_host(self.url, input.as_ptr().cast(), input.len()) }
    }

    /// Return the parsed representation of the host for this URL. Non-ASCII domain labels are
    /// punycode-encoded per IDNA if this is the host of a special URL, or percent encoded for
    /// non-special URLs.
    ///
    /// Hostname does not contain port number.
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-hostname)
    ///
    /// ```
    /// use ada_url::Url;
    ///
    /// let url = Url::parse("https://127.0.0.1:8080/index.html", None).expect("Invalid URL");
    /// assert_eq!(url.hostname(), "127.0.0.1");
    /// ```
    pub fn hostname(&self) -> &str {
        unsafe { ffi::ada_get_hostname(self.url) }.as_str()
    }

    pub fn set_hostname(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_hostname(self.url, input.as_ptr().cast(), input.len()) }
    }

    /// Return the path for this URL, as a percent-encoded ASCII string.
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-pathname)
    ///
    /// ```
    /// use ada_url::Url;
    ///
    /// let url = Url::parse("https://example.com/api/versions?page=2", None).expect("Invalid URL");
    /// assert_eq!(url.pathname(), "/api/versions");
    /// ```
    pub fn pathname(&self) -> &str {
        unsafe { ffi::ada_get_pathname(self.url) }.as_str()
    }

    pub fn set_pathname(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_pathname(self.url, input.as_ptr().cast(), input.len()) }
    }

    /// Return this URL’s query string, if any, as a percent-encoded ASCII string.
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-search)
    ///
    /// ```
    /// use ada_url::Url;
    ///
    /// let url = Url::parse("https://example.com/products?page=2", None).expect("Invalid URL");
    /// assert_eq!(url.search(), "?page=2");
    ///
    /// let url = Url::parse("https://example.com/products", None).expect("Invalid URL");
    /// assert_eq!(url.search(), "");
    /// ```
    pub fn search(&self) -> &str {
        unsafe { ffi::ada_get_search(self.url) }.as_str()
    }

    pub fn set_search(&mut self, input: &str) {
        unsafe { ffi::ada_set_search(self.url, input.as_ptr().cast(), input.len()) }
    }

    /// Return the scheme of this URL, lower-cased, as an ASCII string with the ‘:’ delimiter.
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-protocol)
    ///
    /// ```
    /// use ada_url::Url;
    ///
    /// let url = Url::parse("file:///tmp/foo", None).expect("Invalid URL");
    /// assert_eq!(url.protocol(), "file:");
    /// ```
    pub fn protocol(&self) -> &str {
        unsafe { ffi::ada_get_protocol(self.url) }.as_str()
    }

    pub fn set_protocol(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_protocol(self.url, input.as_ptr().cast(), input.len()) }
    }

    /// A URL includes credentials if its username or password is not the empty string.
    pub fn has_credentials(&self) -> bool {
        unsafe { ffi::ada_has_credentials(self.url) }
    }

    /// Returns true if it has an host but it is the empty string.
    pub fn has_empty_hostname(&self) -> bool {
        unsafe { ffi::ada_has_empty_hostname(self.url) }
    }

    /// Returns true if it has a host (included an empty host)
    pub fn has_hostname(&self) -> bool {
        unsafe { ffi::ada_has_hostname(self.url) }
    }

    pub fn has_non_empty_username(&self) -> bool {
        unsafe { ffi::ada_has_non_empty_username(self.url) }
    }

    pub fn has_non_empty_password(&self) -> bool {
        unsafe { ffi::ada_has_non_empty_password(self.url) }
    }

    pub fn has_port(&self) -> bool {
        unsafe { ffi::ada_has_port(self.url) }
    }

    pub fn has_password(&self) -> bool {
        unsafe { ffi::ada_has_password(self.url) }
    }

    pub fn has_hash(&self) -> bool {
        unsafe { ffi::ada_has_hash(self.url) }
    }

    pub fn has_search(&self) -> bool {
        unsafe { ffi::ada_has_search(self.url) }
    }
    /// Returns the parsed version of the URL with all components.
    ///
    /// For more information, read [WHATWG URL spec](https://url.spec.whatwg.org/#dom-url-href)
    pub fn as_str(&self) -> &str {
        self.href()
    }
}

/// Serializes this URL into a `serde` stream.
///
/// This implementation is only available if the `serde` Cargo feature is enabled.
#[cfg(feature = "serde")]
impl serde::Serialize for Url {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

/// Deserializes this URL from a `serde` stream.
///
/// This implementation is only available if the `serde` Cargo feature is enabled.
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Url {
    fn deserialize<D>(deserializer: D) -> Result<Url, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, Unexpected, Visitor};

        struct UrlVisitor;

        impl<'de> Visitor<'de> for UrlVisitor {
            type Value = Url;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string representing an URL")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Url::parse(s, None).map_err(|err| {
                    let err_s = format!("{}", err);
                    Error::invalid_value(Unexpected::Str(s), &err_s.as_str())
                })
            }
        }

        deserializer.deserialize_str(UrlVisitor)
    }
}

unsafe impl Send for Url {}

/// URLs compare like their stringification.
impl PartialEq for Url {
    fn eq(&self, other: &Self) -> bool {
        self.href() == other.href()
    }
}

impl PartialOrd for Url {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.href().partial_cmp(other.href())
    }
}

impl Ord for Url {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.href().cmp(other.href())
    }
}

impl hash::Hash for Url {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.href().hash(state)
    }
}

impl borrow::Borrow<str> for Url {
    fn borrow(&self) -> &str {
        self.href()
    }
}

impl borrow::Borrow<[u8]> for Url {
    fn borrow(&self) -> &[u8] {
        self.href().as_bytes()
    }
}

impl AsRef<[u8]> for Url {
    fn as_ref(&self) -> &[u8] {
        self.href().as_bytes()
    }
}

impl From<Url> for String {
    fn from(val: Url) -> Self {
        val.href().to_owned()
    }
}

impl fmt::Debug for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe {
            let components = ffi::ada_get_components(self.url).as_ref().unwrap();
            let mut debug = f.debug_struct("Url");
            debug
                .field("href", &self.href())
                .field("protocol_end", &components.protocol_end)
                .field("host_start", &components.host_start)
                .field("host_end", &components.host_end);
            let port = if components.port == u32::MAX {
                None
            } else {
                Some(components.port)
            };
            let username_end = if components.username_end == u32::MAX {
                None
            } else {
                Some(components.username_end)
            };
            let search_start = if components.search_start == u32::MAX {
                None
            } else {
                Some(components.search_start)
            };
            let hash_start = if components.hash_start == u32::MAX {
                None
            } else {
                Some(components.hash_start)
            };
            let pathname_start = if components.pathname_start == u32::MAX {
                None
            } else {
                Some(components.pathname_start)
            };

            debug
                .field("port", &port)
                .field("username_end", &username_end)
                .field("search_start", &search_start)
                .field("hash_start", &hash_start)
                .field("pathname_start", &pathname_start)
                .finish()
        }
    }
}

impl TryFrom<&str> for Url {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value, None)
    }
}

impl TryFrom<String> for Url {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value, None)
    }
}

impl TryFrom<&String> for Url {
    type Error = Error;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        Self::parse(value, None)
    }
}

impl ops::Deref for Url {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.href()
    }
}

impl AsRef<str> for Url {
    fn as_ref(&self) -> &str {
        self.href()
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.href())
    }
}

impl std::str::FromStr for Url {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s, None)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn should_display_serialization() {
        let tests = [
            ("http://example.com/", "http://example.com/"),
            ("HTTP://EXAMPLE.COM", "http://example.com/"),
            ("http://user:pwd@domain.com", "http://user:pwd@domain.com/"),
            (
                "HTTP://EXAMPLE.COM/FOO/BAR?K1=V1&K2=V2",
                "http://example.com/FOO/BAR?K1=V1&K2=V2",
            ),
            (
                "http://example.com/🦀/❤️/",
                "http://example.com/%F0%9F%A6%80/%E2%9D%A4%EF%B8%8F/",
            ),
            (
                "https://example.org/hello world.html",
                "https://example.org/hello%20world.html",
            ),
            (
                "https://三十六計.org/走為上策/",
                "https://xn--ehq95fdxbx86i.org/%E8%B5%B0%E7%82%BA%E4%B8%8A%E7%AD%96/", // weird, but this PR cannot change the implementation
            ),
        ];
        for (value, expected) in tests {
            eprintln!("{value} -> {expected}");
            let url = Url::parse(value, None).expect("Should have parsed url");
            assert_eq!(url.to_string(), expected);
        }
    }

    #[test]
    fn try_from_ok() {
        let url = Url::try_from("http://example.com/foo/bar?k1=v1&k2=v2");
        dbg!(&url);
        let url = url.unwrap();
        assert_eq!(url.href(), "http://example.com/foo/bar?k1=v1&k2=v2");
        assert_eq!(
            url,
            Url::parse("http://example.com/foo/bar?k1=v1&k2=v2", None).unwrap(),
        );
    }

    #[test]
    fn try_from_err() {
        let url = Url::try_from("this is not a url");
        dbg!(&url);
        let error = url.unwrap_err();
        assert_eq!(error.to_string(), r#"Invalid url: "this is not a url""#);
        assert!(matches!(error, Error::ParseUrl(_)));
    }

    #[test]
    fn should_compare_urls() {
        let tests = [
            ("http://example.com/", "http://example.com/", true),
            ("http://example.com/", "https://example.com/", false),
            ("http://example.com#", "https://example.com/#", false),
            ("http://example.com", "https://example.com#", false),
            (
                "https://user:pwd@example.com",
                "https://user:pwd@example.com",
                true,
            ),
        ];
        for (left, right, expected) in tests {
            let left_url = Url::parse(left, None).expect("Should have parsed url");
            let right_url = Url::parse(right, None).expect("Should have parsed url");
            assert_eq!(
                left_url == right_url,
                expected,
                "left: {left}, right: {right}, expected: {expected}",
            );
        }
    }
    #[test]
    fn should_order_alphabetically() {
        let left = Url::parse("https://example.com/", None).expect("Should have parsed url");
        let right = Url::parse("https://zoo.tld/", None).expect("Should have parsed url");
        assert!(left < right);
        let left = Url::parse("https://c.tld/", None).expect("Should have parsed url");
        let right = Url::parse("https://a.tld/", None).expect("Should have parsed url");
        assert!(right < left);
    }

    #[test]
    fn should_parse_simple_url() {
        let mut out = Url::parse(
            "https://username:password@google.com:9090/search?query#hash",
            None,
        )
        .expect("Should have parsed a simple url");
        assert_eq!(out.origin(), "https://google.com:9090");
        assert_eq!(
            out.href(),
            "https://username:password@google.com:9090/search?query#hash"
        );

        assert!(out.set_username("new-username"));
        assert_eq!(out.username(), "new-username");

        assert!(out.set_password("new-password"));
        assert_eq!(out.password(), "new-password");

        assert!(out.set_port("4242"));
        assert_eq!(out.port(), "4242");

        out.set_hash("#new-hash");
        assert_eq!(out.hash(), "#new-hash");

        assert!(out.set_host("yagiz.co:9999"));
        assert_eq!(out.host(), "yagiz.co:9999");

        assert!(out.set_hostname("domain.com"));
        assert_eq!(out.hostname(), "domain.com");

        assert!(out.set_pathname("/new-search"));
        assert_eq!(out.pathname(), "/new-search");

        out.set_search("updated-query");
        assert_eq!(out.search(), "?updated-query");

        out.set_protocol("wss");
        assert_eq!(out.protocol(), "wss:");

        assert!(out.has_credentials());
        assert!(out.has_non_empty_username());
        assert!(out.has_non_empty_password());
        assert!(out.has_search());
        assert!(out.has_hash());
        assert!(out.has_password());
    }

    #[test]
    fn can_parse_simple_url() {
        assert!(Url::can_parse("https://google.com", None));
        assert!(Url::can_parse("/helo", Some("https://www.google.com")));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_serde_serialize_deserialize() {
        let input = "https://www.google.com";
        let output = "\"https://www.google.com/\"";
        let url = Url::parse(&input, None).unwrap();
        assert_eq!(serde_json::to_string(&url).unwrap(), output.to_string());

        let deserialized: Url = serde_json::from_str(&output).unwrap();
        assert_eq!(deserialized.href(), input.to_string() + "/");
    }
}
