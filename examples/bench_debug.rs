use std::hint::black_box;
use std::time::Instant;
const URLS: &[&str] = &[
    "https://www.google.com/webhp?hl=en&amp;ictx=2",
    "https://en.wikipedia.org/wiki/Dog#Roles_with_humans",
    "https://www.tiktok.com/@aguyandagolden/video/7133277734310038830",
    "https://business.twitter.com/en/help/troubleshooting/how-twitter-ads-work.html?ref=web",
    "https://images-na.ssl-images-amazon.com/images/I/41Gc3C8UysL.css?AUI",
    "https://www.reddit.com/?after=t3_zvz1ze",
    "https://www.reddit.com/login/?dest=https%3A%2F%2Fwww.reddit.com%2F",
    "postgresql://other:pass@localhost:5432/db",
    "http://192.168.1.1",
    "http://[2606:4700::1111]",
];
fn bench(label: &str, n: u32, f: impl Fn()) {
    for _ in 0..10000 { f(); }
    let t = Instant::now();
    for _ in 0..n { f(); }
    let ns = t.elapsed().as_nanos() as f64 / n as f64;
    println!("{label}: {ns:.0} ns/iter  ({:.0} ns/url)", ns / URLS.len() as f64);
}
fn main() {
    let n = 500_000u32;
    bench("can_parse all",    n, || { for u in URLS { black_box(ada_url::Url::can_parse(black_box(u), None)); } });
    bench("parse    all",     n, || { for u in URLS { black_box(ada_url::Url::parse(black_box(u), None)); } });
    bench("can_parse fast7",  n, || { for u in &URLS[..7] { black_box(ada_url::Url::can_parse(black_box(u), None)); } });
    bench("can_parse slow3",  n, || { for u in &URLS[7..] { black_box(ada_url::Url::can_parse(black_box(u), None)); } });
}
