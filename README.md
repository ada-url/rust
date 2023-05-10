## Rust bindings for Ada

Fast [WHATWG specification](https://url.spec.whatwg.org) compliant URL parser for Rust.

### Usage

Add the following as a dependency to your project (`Cargo.toml`):

```
[dependencies]
ada-url = { git = "https://github.com/ada-url/rust" }
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