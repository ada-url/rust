# WHATWG URL parser for Rust

Fast [WHATWG URL Specification](https://url.spec.whatwg.org) compliant URL parser for Rust.
Well-tested and widely used by Node.js since [Node 18](https://nodejs.org/en/blog/release/v18.17.0).

The Ada library passes the full range of tests from the specification, across a wide range of platforms (e.g., Windows, Linux, macOS).
It fully supports the relevant [Unicode Technical Standard](https://www.unicode.org/reports/tr46/#ToUnicode).

## Usage

See [here](examples/simple.rs) for a usage example.
You can run it locally with `cargo run --example simple`.
Feel free to adjust it for exploring this crate further.

### Features

**std:** Functionalities that require `std`.
This feature is enabled by default, set `no-default-features` to `true` if you want `no-std`.

**serde:** Allow `Url` to work with `serde`. This feature is disabled by default. Enabling this feature without `std` would provide you only `Serialize`.
Enabling this feature and `std` would provide you both `Serialize` and `Deserialize`.

**libcpp:** Build `ada-url` with `libc++`. This feature is disabled by default.
Enabling this feature without `libc++` installed would cause compile error.

### Performance

Ada is fast. The benchmark below shows **3.49 times** faster URL parsing compared to `url`

```text
can_parse/ada_url       time:   [1.2109 µs 1.2121 µs 1.2133 µs]
                        thrpt:  [635.09 MiB/s 635.75 MiB/s 636.38 MiB/s]

parse/ada_url           time:   [2.0124 µs 2.0157 µs 2.0190 µs]
                        thrpt:  [381.67 MiB/s 382.28 MiB/s 382.91 MiB/s]

parse/url               time:   [7.0530 µs 7.0597 µs 7.0666 µs]
                        thrpt:  [109.04 MiB/s 109.15 MiB/s 109.25 MiB/s]
```

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
just all --skip=libcpp,serde
```

## License

This code is made available under the Apache License 2.0 as well as the MIT license.

Our tests include third-party code and data. The benchmarking code includes third-party code: it is provided for research purposes only and not part of the library.
