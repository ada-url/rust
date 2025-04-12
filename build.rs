use regex::Regex;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::{env, fmt};

#[derive(Clone, Debug)]
pub struct Target {
    pub architecture: String,
    pub vendor: String,
    pub system: Option<String>,
    pub abi: Option<String>,
}

impl Target {
    pub fn as_strs(&self) -> (&str, &str, Option<&str>, Option<&str>) {
        (
            self.architecture.as_str(),
            self.vendor.as_str(),
            self.system.as_deref(),
            self.abi.as_deref(),
        )
    }
}

impl Display for Target {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}-{}", &self.architecture, &self.vendor)?;

        if let Some(ref system) = self.system {
            write!(f, "-{}", system)
        } else {
            Ok(())
        }?;

        if let Some(ref abi) = self.abi {
            write!(f, "-{}", abi)
        } else {
            Ok(())
        }
    }
}

pub fn ndk() -> String {
    env::var("ANDROID_NDK").expect("ANDROID_NDK variable not set")
}

pub fn target_arch(arch: &str) -> &str {
    match arch {
        "armv7" => "arm",
        "aarch64" => "arm64",
        "i686" => "x86",
        arch => arch,
    }
}

fn host_tag() -> &'static str {
    // Because this is part of build.rs, the target_os is actually the host system
    if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else if cfg!(target_os = "linux") {
        "linux-x86_64"
    } else if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else {
        panic!("host os is not supported")
    }
}

/// Get NDK major version from source.properties
fn ndk_major_version(ndk_dir: &Path) -> u32 {
    // Capture version from the line with Pkg.Revision
    let re = Regex::new(r"Pkg.Revision = (\d+)\.(\d+)\.(\d+)").unwrap();
    // There's a source.properties file in the ndk directory, which contains
    let mut source_properties =
        File::open(ndk_dir.join("source.properties")).expect("Couldn't open source.properties");
    let mut buf = String::new();
    source_properties
        .read_to_string(&mut buf)
        .expect("Could not read source.properties");
    // Capture version info
    let captures = re
        .captures(&buf)
        .expect("source.properties did not match the regex");
    // Capture 0 is the whole line of text
    captures[1].parse().expect("could not parse major version")
}

fn main() {
    let target_str = env::var("TARGET").unwrap();
    let target: Vec<String> = target_str.split('-').map(|s| s.into()).collect();
    assert!(target.len() >= 2, "Failed to parse TARGET {}", target_str);

    let abi = if target.len() > 3 {
        Some(target[3].clone())
    } else {
        None
    };

    let system = if target.len() > 2 {
        Some(target[2].clone())
    } else {
        None
    };

    let target = Target {
        architecture: target[0].clone(),
        vendor: target[1].clone(),
        system,
        abi,
    };

    let mut build = cc::Build::new();
    build
        .file("./deps/ada.cpp")
        .include("./deps")
        .cpp(true)
        .std("c++20");

    let compile_target_arch = env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH");
    let compile_target_os = env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS");
    let compile_target_feature = env::var("CARGO_CFG_TARGET_FEATURE");
    // Except for Emscripten target (which emulates POSIX environment), compile to Wasm via WASI SDK
    // which is currently the only standalone provider of stdlib for compilation of C/C++ libraries.

    match target.system.as_deref() {
        Some("android" | "androideabi") => {
            let ndk = ndk();
            let major = ndk_major_version(Path::new(&ndk));
            if major < 22 {
                build
                    .flag(format!("--sysroot={}/sysroot", ndk))
                    .flag(format!(
                        "-isystem{}/sources/cxx-stl/llvm-libc++/include",
                        ndk
                    ));
            } else {
                // NDK versions >= 22 have the sysroot in the llvm prebuilt by
                let host_toolchain = format!("{}/toolchains/llvm/prebuilt/{}", ndk, host_tag());
                // sysroot is stored in the prebuilt llvm, under the host
                build.flag(format!("--sysroot={}/sysroot", host_toolchain));
            }
        }
        _ => {
            if compile_target_arch.starts_with("wasm") && compile_target_os != "emscripten" {
                let wasi_sdk = env::var("WASI_SDK").unwrap_or_else(|_| "/opt/wasi-sdk".to_owned());
                assert!(
                    Path::new(&wasi_sdk).exists(),
                    "WASI SDK not found at {wasi_sdk}"
                );
                build.compiler(format!("{wasi_sdk}/bin/clang++"));
                let wasi_sysroot_lib = match compile_target_feature {
                    Ok(compile_target_feature) if compile_target_feature.contains("atomics") => {
                        "wasm32-wasip1-threads"
                    }
                    _ => "wasm32-wasip1",
                };
                println!(
                    "cargo:rustc-link-search={wasi_sdk}/share/wasi-sysroot/lib/{wasi_sysroot_lib}"
                );
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
                    build.target("wasm32-wasip1");
                    println!("cargo:rustc-link-lib=c");
                    build.file("./deps/wasi_to_unknown.cpp");
                }
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
                }
            } else if compiler.is_like_clang() && cfg!(feature = "libcpp") {
                build.cpp_set_stdlib("c++");
            }
        }
    }

    build.compile("ada");
}
