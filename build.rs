use std::env;

// Taken from https://github.com/Brooooooklyn/ada-url/blob/main/ada/build.rs
fn main() {
    println!("cargo:rerun-if-changed=deps/ada.cpp");
    println!("cargo:rerun-if-changed=deps/ada.h");
    println!("cargo:rerun-if-changed=deps/ada_c.h");

    let mut build = cc::Build::new();
    build
        .file("./deps/ada.cpp")
        .include("./deps/ada.h")
        .include("./deps/ada_c.h")
        .cpp(true)
        .std("c++17");

    let compile_target_arch = env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH");
    let compile_target_os = env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS");
    let compile_target_env = env::var("CARGO_CFG_TARGET_ENV").expect("CARGO_CFG_TARGET_ENV");
    let compile_target_feature = env::var("CARGO_CFG_TARGET_FEATURE");
    // Except for Emscripten target (which emulates POSIX environment), compile to Wasm via WASI SDK
    // which is currently the only standalone provider of stdlib for compilation of C/C++ libraries.
    if compile_target_arch.starts_with("wasm") && compile_target_os != "emscripten" {
        let wasi_sdk = env::var("WASI_SDK").unwrap_or_else(|_| "/opt/wasi-sdk".to_owned());
        assert!(
            std::path::Path::new(&wasi_sdk).exists(),
            "WASI SDK not found at {wasi_sdk}"
        );
        build.compiler(format!("{wasi_sdk}/bin/clang++"));
        let wasi_sysroot_lib = match compile_target_feature {
            Ok(compile_target_feature) if compile_target_feature.contains("atomics") => {
                "wasm32-wasi-threads"
            }
            _ => "wasm32-wasi",
        };
        println!("cargo:rustc-link-search={wasi_sdk}/share/wasi-sysroot/lib/{wasi_sysroot_lib}");
        // Wasm exceptions are new and not yet supported by WASI SDK.
        build.flag("-fno-exceptions");
        // WASI SDK only has libc++ available.
        build.cpp_set_stdlib("c++");
        // Explicitly link C++ ABI to avoid linking errors (it takes care of C++ -> C "lowering").
        println!("cargo:rustc-link-lib=c++abi");
        // Because Ada is a pure parsing library that doesn't need any OS syscalls,
        // it's also possible to compile it to wasm32-unknown-unknown.
        // This still requires WASI SDK for libc & libc++, but then we need a few hacks / overrides to get a pure Wasm w/o imports instead.
        if compile_target_os == "unknown" {
            build.target("wasm32-wasi");
            println!("cargo:rustc-link-lib=c");
            build.file("./deps/wasi_to_unknown.cpp");
        }
    } else if !(compile_target_os == "windows" && compile_target_env == "msvc") {
        build.compiler("clang++");
    }

    let compiler = build.get_compiler();
    // Note: it's possible to use Clang++ explicitly on Windows as well, so this check
    // should be specifically for "is target compiler MSVC" and not "is target OS Windows".
    if compiler.is_like_msvc() {
        build.static_crt(true);
        link_args::windows! {
            unsafe {
                no_default_lib(
                    "libcmt.lib",
                );
            }
        };
    } else if compiler.is_like_clang() && cfg!(feature = "libcpp") {
        build.cpp_set_stdlib("c++");
    }

    build.compile("ada");
}
