use std::ptr;

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
        pub fn ada_parse(url: *const c_char) -> *mut ada_url;
        pub fn ada_free(url: *mut ada_url);
        pub fn ada_free_owned_string(url: *mut ada_owned_string);
        pub fn ada_is_valid(url: *mut ada_url) -> bool;
        pub fn ada_can_parse(url: *const c_char, base: *const c_char) -> bool;
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
        pub fn ada_set_origin(url: *mut ada_url, input: *const c_char) -> bool;
        pub fn ada_set_href(url: *mut ada_url, input: *const c_char) -> bool;
        pub fn ada_set_username(url: *mut ada_url, input: *const c_char) -> bool;
        pub fn ada_set_password(url: *mut ada_url, input: *const c_char) -> bool;
        pub fn ada_set_port(url: *mut ada_url, input: *const c_char) -> bool;
        pub fn ada_set_hash(url: *mut ada_url, input: *const c_char);
        pub fn ada_set_host(url: *mut ada_url, input: *const c_char) -> bool;
        pub fn ada_set_hostname(url: *mut ada_url, input: *const c_char) -> bool;
        pub fn ada_set_pathname(url: *mut ada_url, input: *const c_char) -> bool;
        pub fn ada_set_search(url: *mut ada_url, input: *const c_char);
        pub fn ada_set_protocol(url: *mut ada_url, input: *const c_char) -> bool;

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
    pub fn parse<U: AsRef<str>>(url: U) -> Result<Url, Error> {
        let url_with_0_terminate = std::ffi::CString::new(url.as_ref()).unwrap();
        unsafe {
            let url_aggregator = ffi::ada_parse(url_with_0_terminate.as_ptr());

            if ffi::ada_is_valid(url_aggregator) {
                Ok(Url {
                    origin: None,
                    url: url_aggregator,
                })
            } else {
                Err(Error::ParseUrl(url.as_ref().to_owned()))
            }
        }
    }

    pub fn can_parse(input: &str, base: Option<&str>) -> bool {
        unsafe {
            ffi::ada_can_parse(
                input.as_ptr().cast(),
                base.map(|b| b.as_ptr()).unwrap_or(ptr::null_mut()).cast(),
            )
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

    pub fn username(&self) -> &str {
        unsafe { ffi::ada_get_username(self.url) }.as_str()
    }

    pub fn password(&self) -> &str {
        unsafe { ffi::ada_get_password(self.url) }.as_str()
    }

    pub fn port(&self) -> &str {
        unsafe { ffi::ada_get_port(self.url) }.as_str()
    }

    pub fn hash(&self) -> &str {
        unsafe { ffi::ada_get_hash(self.url) }.as_str()
    }

    pub fn host(&self) -> &str {
        unsafe { ffi::ada_get_host(self.url) }.as_str()
    }

    pub fn hostname(&self) -> &str {
        unsafe { ffi::ada_get_hostname(self.url) }.as_str()
    }

    pub fn pathname(&self) -> &str {
        unsafe { ffi::ada_get_pathname(self.url) }.as_str()
    }

    pub fn search(&self) -> &str {
        unsafe { ffi::ada_get_search(self.url) }.as_str()
    }

    pub fn protocol(&self) -> &str {
        unsafe { ffi::ada_get_protocol(self.url) }.as_str()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn should_parse_simple_url() {
        let mut out = Url::parse("https://username:password@google.com:9090/search?query#hash")
            .expect("Should have parsed a simple url");
        assert_eq!(out.origin(), "https://google.com:9090");
        assert_eq!(
            out.href(),
            "https://username:password@google.com:9090/search?query#hash"
        );
        assert_eq!(out.username(), "username");
        assert_eq!(out.password(), "password");
        assert_eq!(out.port(), "9090");
        assert_eq!(out.hash(), "#hash");
        assert_eq!(out.host(), "google.com:9090");
        assert_eq!(out.hostname(), "google.com");
        assert_eq!(out.pathname(), "/search");
        assert_eq!(out.search(), "?query");
        assert_eq!(out.protocol(), "https:");
    }

    #[test]
    fn can_parse_simple_url() {
        assert!(Url::can_parse("https://google.com", None));
        assert!(Url::can_parse("/helo", Some("https://www.google.com")));
    }
}
