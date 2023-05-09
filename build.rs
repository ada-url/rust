use std::env;

fn main() {
    println!("cargo:rerun-if-changed=deps/ada.cpp");
    println!("cargo:rerun-if-changed=deps/ada.h");
    println!("cargo:rerun-if-changed=deps/ada_c.h");

    let mut build = cc::Build::new();
    build
        .file("./deps/ada.cpp")
        .include("./deps/ada.h")
        .include("./deps/ada_c.h")
        .cpp(true);

    let compile_target_os = env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS");
    let compile_target_env = env::var("CARGO_CFG_TARGET_ENV").expect("CARGO_CFG_TARGET_ENV");
    if !(compile_target_os == "windows" && compile_target_env == "msvc") {
        build.compiler("clang++");
        build.cpp_set_stdlib("c++").flag("-std=c++17");
        println!("cargo:rustc-link-lib=c++");
    } else {
        build.flag("/std:c++17").static_crt(true);
    }

    build.compile("ada");
}
