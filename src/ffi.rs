#![allow(non_camel_case_types)]
use core::ffi::{c_char, c_uint};

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
use std::fmt::Display;

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
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        unsafe {
            let slice = core::slice::from_raw_parts(self.data.cast(), self.length);
            core::str::from_utf8_unchecked(slice)
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
            let slice = core::slice::from_raw_parts(self.data.cast(), self.length);
            core::str::from_utf8_unchecked(slice)
        }
    }
}

#[cfg(feature = "std")]
impl Display for ada_owned_string {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_ref().to_owned())
    }
}

impl Drop for ada_owned_string {
    fn drop(&mut self) {
        // @note This is needed because ada_free_owned_string accepts by value
        let copy = ada_owned_string {
            data: self.data,
            length: self.length,
        };
        unsafe {
            ada_free_owned_string(copy);
        };
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
    pub fn ada_free_owned_string(url: ada_owned_string);
    pub fn ada_copy(url: *mut ada_url) -> *mut ada_url;
    pub fn ada_is_valid(url: *mut ada_url) -> bool;
    pub fn ada_can_parse(url: *const c_char, length: usize) -> bool;
    pub fn ada_can_parse_with_base(
        input: *const c_char,
        input_length: usize,
        base: *const c_char,
        base_length: usize,
    ) -> bool;
    pub fn ada_get_components(url: *mut ada_url) -> *mut ada_url_components;

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
    pub fn ada_get_host_type(url: *mut ada_url) -> c_uint;
    pub fn ada_get_scheme_type(url: *mut ada_url) -> c_uint;

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

    // Clear methods
    pub fn ada_clear_search(url: *mut ada_url);
    pub fn ada_clear_hash(url: *mut ada_url);
    pub fn ada_clear_port(url: *mut ada_url);

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

    // IDNA methods
    pub fn ada_idna_to_unicode(input: *const c_char, length: usize) -> ada_owned_string;
    pub fn ada_idna_to_ascii(input: *const c_char, length: usize) -> ada_owned_string;
}

#[cfg(test)]
mod tests {
    use crate::ffi;

    #[test]
    fn ada_free_owned_string_works() {
        let str = "meßagefactory.ca";
        let result = unsafe { ffi::ada_idna_to_ascii(str.as_ptr().cast(), str.len()) };
        assert_eq!(result.as_ref(), "xn--meagefactory-m9a.ca");
        unsafe { ffi::ada_free_owned_string(result) };
    }
}
