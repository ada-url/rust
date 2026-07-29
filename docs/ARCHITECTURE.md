# Architecture

## Upstream pin and scope

The native Rust implementation is mapped against `ada-url/ada` commit
`30f3f3020c5a979b62f90dc9c37fd45de3cc84d7`. Full scope means:

1. URL parsing, serialization, base resolution, file URLs, opaque URLs, IPv4,
   IPv6, IDNA, origin calculation, setters, and configurable length limits.
2. URLSearchParams, including stable UTF-16 sorting semantics.
3. URLPattern behind an optional feature and a denial-of-service-resistant regex
   provider.
4. The upstream WPT, Ada-extra, UTS #46, setter, and URLPattern fixtures.

The C ABI and the multi-string `ada::url` representation are not ported. Rust's
public `Url` corresponds to `ada::url_aggregator`.

## Data layout

`Url` owns exactly one normalized UTF-8 `String`. `Components` stores eight
32-bit fields: scheme end, username end, host start/end, numeric port, pathname
start, query start, and fragment start. `u32::MAX` denotes an absent component.
Small enums and boolean properties are packed separately.

Every public component getter is a checked slice of that buffer and allocates
nothing. Parser construction and every mutation validate the component ordering
before publishing the value.

The standard `String` is the baseline. Inline storage will be adopted only if
allocation and end-to-end benchmarks show that its larger object and extra
branches are a net win on the corpus.

## Parser pipeline

1. Enforce the raw-input length limit.
2. Try conservative native parsers for normalized HTTP(S), normalized file
   URLs, common authority URLs, opaque URLs, WHATWG IPv4/IPv6 hosts, and direct
   references. Inputs that can be copied directly avoid intermediate
   allocations.
3. Delegate uncommon states to the memory-safe Rust `url` parser.
4. Apply Ada-compatible normalization for file, opaque, host, and setter edge
   cases that differ from the fallback parser.
5. Enforce normalized-output length, derive compact offsets, validate them,
   and publish the immutable `Url`.

Uncommon states not yet covered natively use the memory-safe Rust `url` parser.
The fallback is a correctness bridge; native coverage is expanded only with
fixture and differential proof.

## Scanning and SIMD

Character properties are compile-time byte lookup tables. `memchr`,
`memchr2`, and `memchr3` provide maintained runtime-dispatched SIMD for
delimiter scans on x86-64, AArch64, and WebAssembly.

The crate currently forbids unsafe code, so handwritten intrinsics are not part
of the library. A future intrinsic implementation is admitted only as a
separate, audited dependency when a reproducible benchmark beats the safe
baseline. Each such kernel must:

- live in an architecture-specific module with a safe public wrapper;
- use runtime feature detection unless enabled at compile time;
- never read outside the source allocation;
- have a byte-for-byte scalar oracle and randomized differential tests;
- document every unsafe operation and compile independently with
  `unsafe_op_in_unsafe_fn` denied while preserving `#![forbid(unsafe_code)]` in
  `ada-url`.

## Correctness gates

- 100% of pinned `urltestdata.json`, Ada extras, long-input tests, setters,
  percent-encoding, `IdnaTestV2`, `toascii`, URLSearchParams, and URLPattern
  fixtures.
- Mutation failures are transactional.
- All offsets validate after every parse and setter in debug/test builds.
- `cargo test`, Clippy, rustfmt, docs, Miri for safe targets, and fuzz smoke
  tests pass.

## Performance protocol

Rust, C++ Ada, and comparison parsers receive the same preloaded byte strings.
The benchmark consumes the normalized serialization so validation-only work
cannot masquerade as parsing. Build settings, target features, CPU model,
compiler versions, warmup, sample count, and dataset revision are recorded.

Track:

- nanoseconds per URL and GiB/s;
- p50/p95/p99 by input family;
- allocations and allocated bytes per URL;
- `parse`, `can_parse`, component access, setters, IDNA, IPv4, percent
  encoding, SearchParams, and URLPattern separately;
- portable binaries and `target-cpu=native` binaries.

A performance result is accepted only when normalized outputs match and the
median regression is no worse than 3% on any primary corpus.
