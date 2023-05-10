use crate::ffi::{ada_parse, ada_parse_with_base};
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
        pub fn ada_get_origin(url: *mut ada_url) -> *mut ada_owned_string;
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
    origin: Option<*mut ffi::ada_owned_string>,
    url: *mut ffi::ada_url,
}

impl Drop for Url {
    fn drop(&mut self) {
        if let Some(origin) = self.origin {
            unsafe {
                ffi::ada_free_owned_string(origin);
            }
        }
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
                ada_parse_with_base(
                    input.as_ptr().cast(),
                    input.len(),
                    base.as_ptr().cast(),
                    base.len(),
                )
            },
            None => unsafe { ada_parse(input.as_ptr().cast(), input.len()) },
        };

        if unsafe { ffi::ada_is_valid(url_aggregator) } {
            Ok(Url {
                origin: None,
                url: url_aggregator,
            })
        } else {
            Err(Error::ParseUrl(input.to_owned()))
        }
    }

    /// Returns whether or not the URL can be parsed or not.
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

    pub fn origin(&mut self) -> &str {
        unsafe {
            self.origin = Some(ffi::ada_get_origin(self.url));
            self.origin.map(|o| (*o).as_ref()).unwrap_or("")
        }
    }

    pub fn href(&self) -> &str {
        unsafe { ffi::ada_get_href(self.url) }.as_str()
    }

    pub fn set_href(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_href(self.url, input.as_ptr().cast(), input.len()) }
    }

    pub fn username(&self) -> &str {
        unsafe { ffi::ada_get_username(self.url) }.as_str()
    }

    pub fn set_username(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_username(self.url, input.as_ptr().cast(), input.len()) }
    }

    pub fn password(&self) -> &str {
        unsafe { ffi::ada_get_password(self.url) }.as_str()
    }

    pub fn set_password(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_password(self.url, input.as_ptr().cast(), input.len()) }
    }

    pub fn port(&self) -> &str {
        unsafe { ffi::ada_get_port(self.url) }.as_str()
    }

    pub fn set_port(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_port(self.url, input.as_ptr().cast(), input.len()) }
    }

    pub fn hash(&self) -> &str {
        unsafe { ffi::ada_get_hash(self.url) }.as_str()
    }

    pub fn set_hash(&mut self, input: &str) {
        unsafe { ffi::ada_set_hash(self.url, input.as_ptr().cast(), input.len()) }
    }

    pub fn host(&self) -> &str {
        unsafe { ffi::ada_get_host(self.url) }.as_str()
    }

    pub fn set_host(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_host(self.url, input.as_ptr().cast(), input.len()) }
    }

    pub fn hostname(&self) -> &str {
        unsafe { ffi::ada_get_hostname(self.url) }.as_str()
    }

    pub fn set_hostname(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_hostname(self.url, input.as_ptr().cast(), input.len()) }
    }

    pub fn pathname(&self) -> &str {
        unsafe { ffi::ada_get_pathname(self.url) }.as_str()
    }

    pub fn set_pathname(&mut self, input: &str) -> bool {
        unsafe { ffi::ada_set_pathname(self.url, input.as_ptr().cast(), input.len()) }
    }

    pub fn search(&self) -> &str {
        unsafe { ffi::ada_get_search(self.url) }.as_str()
    }

    pub fn set_search(&mut self, input: &str) {
        unsafe { ffi::ada_set_search(self.url, input.as_ptr().cast(), input.len()) }
    }

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
