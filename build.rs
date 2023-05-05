fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/binding.rs");
    println!("cargo:rerun-if-changed=deps/ada.h");

    cxx_build::bridge("src/binding.rs")
        .file("deps/ada.cpp")
        .flag_if_supported("-std=c++17")
        .compile("ada");
}
