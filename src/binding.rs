#[cxx::bridge(namespace = "ada")]
mod ffi {
    #[namespace = "url_components"]
    struct url_components {
        protocol_end: u32,
        username_end: u32,
        host_start: u32,
        host_end: u32,
        port: u32,
        pathname_start: u32,
        search_start: u32,
        hash_start: u32,
    }

    unsafe extern "C++" {
        include!("ada-url/deps/ada.h");
    }
}
