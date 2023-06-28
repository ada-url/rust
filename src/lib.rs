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

use thiserror::Error;

pub mod ffi {
    use std::ffi::c_char;

    #[repr(C)]
    pub struct ada_url {
        _unused: [u8; 0],
        _marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
    }

    #[repr(C)]
    pub struct ada_string {
        pub data: *const c_char,
        pub length: usize,
    }

    impl ada_string {
        pub fn as_str(self) -> &'static str {
            unsafe {
                let slice = std::slice::from_raw_parts(self.data.cast(), self.length);
                std::str::from_utf8_unchecked(slice)
            }
        }
    }

    #[repr(C)]
    pub struct ada_owned_string {
        pub data: *const c_char,
        pub length: usize,
    }

    impl AsRef<str> for ada_owned_string {
        fn as_ref(&self) -> &str {
            unsafe {
                let slice = std::slice::from_raw_parts(self.data.cast(), self.length);
                std::str::from_utf8_unchecked(slice)
            }
        }
    }

    #[repr(C)]
    pub struct ada_url_components {
        pub protocol_end: u32,
        pub username_end: u32,
        pub host_start: u32,
        pub host_end: u32,
        pub port: u32,
        pub pathname_start: u32,
        pub search_start: u32,
        pub hash_start: u32,
    }

    extern "C" {
        pub fn ada_parse(input: *const c_char, length: usize) -> *mut ada_url;
        pub fn ada_parse_with_base(
            input: *const c_char,
            input_length: usize,
            base: *const c_char,
            base_length: usize,
        ) -> *mut ada_url;
        pub fn ada_free(url: *mut ada_url);
        pub fn ada_free_owned_string(url: *mut ada_owned_string);
        pub fn ada_is_valid(url: *mut ada_url) -> bool;
        pub fn ada_can_parse(url: *const c_char, length: usize) -> bool;
        pub fn ada_can_parse_with_base(
            input: *const c_char,
            input_length: usize,
            base: *const c_char,
            base_length: usize,
        ) -> bool;
        pub fn ada_get_url_components(url: *mut ada_url) -> ada_url_components;

        // Getters
        pub fn ada_get_origin(url: *mut ada_url) -> ada_owned_string;
        pub fn ada_get_href(url: *mut ada_url) -> ada_string;
        pub fn ada_get_username(url: *mut ada_url) -> ada_string;
        pub fn ada_get_password(url: *mut ada_url) -> ada_string;
        pub fn ada_get_port(url: *mut ada_url) -> ada_string;
        pub fn ada_get_hash(url: *mut ada_url) -> ada_string;
        pub fn ada_get_host(url: *mut ada_url) -> ada_string;
        pub fn ada_get_hostname(url: *mut ada_url) -> ada_string;
        pub fn ada_get_pathname(url: *mut ada_url) -> ada_string;
        pub fn ada_get_search(url: *mut ada_url) -> ada_string;
        pub fn ada_get_protocol(url: *mut ada_url) -> ada_string;

        // Setters
        pub fn ada_set_href(url: *mut ada_url, input: *const c_char, length: usize) -> bool;
        pub fn ada_set_username(url: *mut ada_url, input: *const c_char, length: usize) -> bool;
        pub fn ada_set_password(url: *mut ada_url, input: *const c_char, length: usize) -> bool;
        pub fn ada_set_port(url: *mut ada_url, input: *const c_char, length: usize) -> bool;
        pub fn ada_set_hash(url: *mut ada_url, input: *const c_char, length: usize);
        pub fn ada_set_host(url: *mut ada_url, input: *const c_char, length: usize) -> bool;
        pub fn ada_set_hostname(url: *mut ada_url, input: *const c_char, length: usize) -> bool;
        pub fn ada_set_pathname(url: *mut ada_url, input: *const c_char, length: usize) -> bool;
        pub fn ada_set_search(url: *mut ada_url, input: *const c_char, length: usize);
        pub fn ada_set_protocol(url: *mut ada_url, input: *const c_char, length: usize) -> bool;

        // Validators
        pub fn ada_has_credentials(url: *mut ada_url) -> bool;
        pub fn ada_has_empty_hostname(url: *mut ada_url) -> bool;
        pub fn ada_has_hostname(url: *mut ada_url) -> bool;
        pub fn ada_has_non_empty_username(url: *mut ada_url) -> bool;
        pub fn ada_has_non_empty_password(url: *mut ada_url) -> bool;
        pub fn ada_has_port(url: *mut ada_url) -> bool;
        pub fn ada_has_password(url: *mut ada_url) -> bool;
        pub fn ada_has_hash(url: *mut ada_url) -> bool;
        pub fn ada_has_search(url: *mut ada_url) -> bool;
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid url: \"{0}\"")]
    ParseUrl(String),
}

pub struct Url {
    url: *mut ffi::ada_url,
}

impl Drop for Url {
    fn drop(&mut self) {
        unsafe {
            ffi::ada_free(self.url);
        }
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

    pub fn has_credentials(&self) -> bool {
        unsafe { ffi::ada_has_credentials(self.url) }
    }

    pub fn has_empty_hostname(&self) -> bool {
        unsafe { ffi::ada_has_empty_hostname(self.url) }
    }

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

impl std::ops::Deref for Url {
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

impl std::fmt::Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
}
