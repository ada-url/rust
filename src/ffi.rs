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

    // IDNA methods
    pub fn ada_idna_to_unicode(input: *const c_char, length: usize) -> ada_owned_string;
    pub fn ada_idna_to_ascii(input: *const c_char, length: usize) -> ada_owned_string;
}
