# WHATWG URL parser for Rust

Fast, memory-safe [WHATWG URL Specification](https://url.spec.whatwg.org)
compliant URL parser for Rust. The parser is implemented in Rust and forbids
unsafe code in the library.

The crate runs Ada's current URL, setter, IDNA, percent-encoding, DNS-length,
and URLPattern conformance fixtures. It supports the relevant
[Unicode Technical Standard](https://www.unicode.org/reports/tr46/#ToUnicode)
through UTS #46 processing.

## Usage

See [here](examples/simple.rs) for a usage example.
You can run it locally with `cargo run --example simple`.
Feel free to adjust it for exploring this crate further.

### Features

**std:** Enables standard-library integrations. This feature is enabled by
default; set `default-features = false` for `no_std` plus `alloc`.

**serde:** Implements `Serialize` and `Deserialize` for `Url` and
`UrlSearchParams`. This feature is disabled by default and enables `std`.

**url-pattern:** Exposes the WHATWG `UrlPattern` API. This feature is disabled
by default.

The former `bundled` and `libcpp` feature names remain as no-ops so existing
downstream manifests continue to resolve after the move away from the C++ build.

### Performance

The parser uses a single normalized buffer with compact component offsets,
plus conservative fast paths for common normalized URLs. Run the included
Criterion comparisons against the `url` crate with:

```sh
cargo bench --bench parse
cargo bench --bench wpt
```

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for storage invariants,
fallback boundaries, and the performance policy.

### Implemented traits

`Url` implements the following traits.

| Trait(s)                                                                                                                                              | Description                                                                                                                                                                                                   |
|-------------------------------------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **[`Display`](https://doc.rust-lang.org/std/fmt/trait.Display.html)**                                                                                 | Provides `to_string` and allows for the value to be used in [format!](https://doc.rust-lang.org/std/fmt/fn.format.html) macros (e.g. `println!`).                                                             |
| **[`Debug`](https://doc.rust-lang.org/std/fmt/trait.Debug.html)**                                                                                     | Allows debugger output in format macros, (`{:?}` syntax)                                                                                                                                                      |
| **[`PartialEq`](https://doc.rust-lang.org/std/cmp/trait.PartialEq.html), [`Eq`](https://doc.rust-lang.org/std/cmp/trait.Eq.html)**                    | Allows for comparison, `url1 == url2`, `url1.eq(url2)`                                                                                                                                                        |
| **[`PartialOrd`](https://doc.rust-lang.org/std/cmp/trait.PartialOrd.html), [`Ord`](https://doc.rust-lang.org/std/cmp/trait.Ord.html)**                | Allows for ordering `url1 < url2`, done so alphabetically. This is also allows `Url` to be used as a key in a [`BTreeMap`](https://doc.rust-lang.org/std/collections/struct.BTreeMap.html)                    |
| **[`Hash`](https://doc.rust-lang.org/std/hash/trait.Hash.html)**                                                                                      | Makes it so that `Url` can be hashed based on the string representation. This is important so that `Url` can be used as a key in a [`HashMap`](https://doc.rust-lang.org/std/collections/struct.HashMap.html) |
| **[`FromStr`](https://doc.rust-lang.org/std/str/trait.FromStr.html)**                                                                                 | Allows for use with [`str`'s `parse` method](https://doc.rust-lang.org/std/primitive.str.html#method.parse)                                                                                                   |
| **[`TryFrom<String>`, `TryFrom<&str>`](https://doc.rust-lang.org/std/convert/trait.TryFrom.html)**                                                    | Provides `try_into` methods for `String` and `&str`                                                                                                                                                           |
| **[`Borrow<str>`](https://doc.rust-lang.org/std/borrow/trait.Borrow.html), [`Borrow<[u8]>`](https://doc.rust-lang.org/std/borrow/trait.Borrow.html)** | Used in some crates so that the `Url` can be used as a key.                                                                                                                                                   |
| **[`Deref<Target=str>`](https://doc.rust-lang.org/std/ops/trait.Deref.html)**                                                                         | Allows for `&Url` to dereference as a `&str`. Also provides a [number of string methods](https://doc.rust-lang.org/std/string/struct.String.html#deref-methods-str)                                           |
| **[`AsRef<[u8]>`](https://doc.rust-lang.org/std/convert/trait.AsRef.html), [`AsRef<str>`](https://doc.rust-lang.org/std/convert/trait.AsRef.html)**   | Used to do a cheap reference-to-reference conversion.                                                                                                                                                         |
| **[`Send`](https://doc.rust-lang.org/std/marker/trait.Send.html)**                                                                                    | Used to declare that the type can be transferred across thread boundaries.                                                                                                                                    |
| **[`Sync`](https://doc.rust-lang.org/stable/std/marker/trait.Sync.html)**                                                                             | Used to declare that the type is thread-safe.                                                                                                                                                                 |

## Development

### `justfile`

The [`justfile`](./justfile) contains commands (called "recipes") that can be executed by [just](https://github.com/casey/just) for convenience.

**Run all lints and tests:**

```sh
just all
```

**Skipping features:**

```sh
just all --skip=url-pattern
```

## License

This code is made available under the Apache License 2.0 as well as the MIT license.

Our tests include third-party code and data. The benchmarking code includes third-party code: it is provided for research purposes only and not part of the library.
