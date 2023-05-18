## Rust bindings for Ada

Fast [WHATWG specification](https://url.spec.whatwg.org) compliant URL parser for Rust.

### Usage

Add the following as a dependency to your project (`Cargo.toml`):

```toml
[dependencies]
ada-url = "1"
```

Here is an example illustrating a common usage:

```Rust
use ada_url::Url;
fn main() {
    let mut u = Url::parse("http://www.google:8080/love#drug", None).expect("bad url");
    println!("port: {:?}", u.port());
    println!("hash: {:?}", u.hash());
    println!("pathname: {:?}", u.pathname());
    println!("href: {:?}", u.href());
    u.set_port("9999");
    println!("href: {:?}", u.href());
}
```

### Performance

Ada is fast. The benchmark below shows **2 times** faster URL parsing compared to `url`

```
     Running bench/parse.rs (target/release/deps/parse-dff65469468a2cec)
url_parse/ada_parse     time:   [2.5853 µs 2.5982 µs 2.6115 µs]
                        change: [-3.8745% -2.9874% -2.0620%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 2 outliers among 100 measurements (2.00%)
  1 (1.00%) low mild
  1 (1.00%) high severe
url_parse/servo_parse   time:   [5.5127 µs 5.6287 µs 5.8046 µs]
                        change: [+0.7618% +3.0977% +6.5694%] (p = 0.01 < 0.05)
                        Change within noise threshold.
Found 2 outliers among 100 measurements (2.00%)
  2 (2.00%) high severe
```