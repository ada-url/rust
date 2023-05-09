use std::{ptr, mem};
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

    impl AsRef<str> for ada_string {
        fn as_ref(&self) -> &str {
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
    pub struct url_components {
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
        pub fn ada_parse(url: *const c_char) -> ada_url;
        pub fn ada_free(url: *mut ada_url);
        pub fn ada_free_owned_string(url: *mut ada_owned_string);
        pub fn ada_is_valid(url: *mut ada_url) -> bool;
        pub fn ada_can_parse(url: *const c_char, base: *const c_char) -> bool;
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
        unsafe {
            if let Some(origin) = self.origin {
                ffi::ada_free_owned_string(origin);
            }
            ffi::ada_free(self.url);
        }
    }
}

impl Url {
    pub fn can_parse(input: &str, base: Option<&str>) -> bool {
        unsafe {
            ffi::ada_can_parse(
                input.as_ptr().cast(),
                base.unwrap_or_else(ptr::null()).as_ptr().cast(),
            )
        }
    }

    pub fn origin(&mut self) -> &str {
        unsafe {
            self.origin = ffi::ada_get_origin(self.url);
            return self.origin.as_ref();
        }
    }

    pub fn href(&self) -> &str {
        unsafe { ffi::ada_get_href(self.url).as_ref() }
    }

    pub fn username(&self) -> &str {
        unsafe { ffi::ada_get_username(self.url).as_ref() }
    }

    pub fn password(&self) -> &str {
        unsafe { ffi::ada_get_password(self.url).as_ref() }
    }

    pub fn port(&self) -> &str {
        unsafe { ffi::ada_get_port(self.url).as_ref() }
    }

    pub fn hash(&self) -> &str {
        unsafe { ffi::ada_get_hash(self.url).as_ref() }
    }

    pub fn host(&self) -> &str {
        unsafe { ffi::ada_get_host(self.url).as_ref() }
    }

    pub fn hostname(&self) -> &str {
        unsafe { ffi::ada_get_hostname(self.url).as_ref() }
    }

    pub fn pathname(&self) -> &str {
        unsafe { ffi::ada_get_pathname(self.url).as_ref() }
    }

    pub fn search(&self) -> &str {
        unsafe { ffi::ada_get_search(self.url).as_ref() }
    }

    pub fn protocol(&self) -> &str {
        unsafe { ffi::ada_get_protocol(self.url).as_ref() }
    }
}

pub fn parse<U: AsRef<str>>(url: U) -> Result<Url, Error> {
    unsafe {
        let url_aggregator = ffi::ada_parse(url.as_ref().as_ptr().cast());

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
