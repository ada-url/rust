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

    pub fn set_origin<U: AsRef<str>>(&mut self, input: U) -> bool {
        let input_with_0_terminate = std::ffi::CString::new(input.as_ref()).unwrap();
        unsafe { ffi::ada_set_origin(self.url, input_with_0_terminate.as_ptr()) }
    }

    pub fn href(&self) -> &str {
        unsafe { ffi::ada_get_href(self.url) }.as_str()
    }

    pub fn set_href<U: AsRef<str>>(&mut self, input: U) -> bool {
        let input_with_0_terminate = std::ffi::CString::new(input.as_ref()).unwrap();
        unsafe { ffi::ada_set_href(self.url, input_with_0_terminate.as_ptr()) }
    }

    pub fn username(&self) -> &str {
        unsafe { ffi::ada_get_username(self.url) }.as_str()
    }

    pub fn set_username<U: AsRef<str>>(&mut self, input: U) -> bool {
        let input_with_0_terminate = std::ffi::CString::new(input.as_ref()).unwrap();
        unsafe { ffi::ada_set_username(self.url, input_with_0_terminate.as_ptr()) }
    }

    pub fn password(&self) -> &str {
        unsafe { ffi::ada_get_password(self.url) }.as_str()
    }

    pub fn set_password<U: AsRef<str>>(&mut self, input: U) -> bool {
        let input_with_0_terminate = std::ffi::CString::new(input.as_ref()).unwrap();
        unsafe { ffi::ada_set_password(self.url, input_with_0_terminate.as_ptr()) }
    }

    pub fn port(&self) -> &str {
        unsafe { ffi::ada_get_port(self.url) }.as_str()
    }

    pub fn set_port<U: AsRef<str>>(&mut self, input: U) -> bool {
        let input_with_0_terminate = std::ffi::CString::new(input.as_ref()).unwrap();
        unsafe { ffi::ada_set_port(self.url, input_with_0_terminate.as_ptr()) }
    }

    pub fn hash(&self) -> &str {
        unsafe { ffi::ada_get_hash(self.url) }.as_str()
    }

    pub fn set_hash<U: AsRef<str>>(&mut self, input: U) {
        let input_with_0_terminate = std::ffi::CString::new(input.as_ref()).unwrap();
        unsafe { ffi::ada_set_hash(self.url, input_with_0_terminate.as_ptr()) }
    }

    pub fn host(&self) -> &str {
        unsafe { ffi::ada_get_host(self.url) }.as_str()
    }

    pub fn set_host<U: AsRef<str>>(&mut self, input: U) -> bool {
        let input_with_0_terminate = std::ffi::CString::new(input.as_ref()).unwrap();
        unsafe { ffi::ada_set_host(self.url, input_with_0_terminate.as_ptr()) }
    }

    pub fn hostname(&self) -> &str {
        unsafe { ffi::ada_get_hostname(self.url) }.as_str()
    }

    pub fn set_hostname<U: AsRef<str>>(&mut self, input: U) -> bool {
        let input_with_0_terminate = std::ffi::CString::new(input.as_ref()).unwrap();
        unsafe { ffi::ada_set_hostname(self.url, input_with_0_terminate.as_ptr()) }
    }

    pub fn pathname(&self) -> &str {
        unsafe { ffi::ada_get_pathname(self.url) }.as_str()
    }

    pub fn set_pathname<U: AsRef<str>>(&mut self, input: U) -> bool {
        let input_with_0_terminate = std::ffi::CString::new(input.as_ref()).unwrap();
        unsafe { ffi::ada_set_pathname(self.url, input_with_0_terminate.as_ptr()) }
    }

    pub fn search(&self) -> &str {
        unsafe { ffi::ada_get_search(self.url) }.as_str()
    }

    pub fn set_search<U: AsRef<str>>(&mut self, input: U) {
        let input_with_0_terminate = std::ffi::CString::new(input.as_ref()).unwrap();
        unsafe { ffi::ada_set_search(self.url, input_with_0_terminate.as_ptr()) }
    }

    pub fn protocol(&self) -> &str {
        unsafe { ffi::ada_get_protocol(self.url) }.as_str()
    }

    pub fn set_protocol<U: AsRef<str>>(&mut self, input: U) -> bool {
        let input_with_0_terminate = std::ffi::CString::new(input.as_ref()).unwrap();
        unsafe { ffi::ada_set_protocol(self.url, input_with_0_terminate.as_ptr()) }
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
        let mut out = Url::parse("https://username:password@google.com:9090/search?query#hash")
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
