use ada_url::Url;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

/// Realistic URL examples collected on the actual web.
/// Matches the url_examples_default array in bench.cpp.
const URLS: &[&str] = &[
    "https://www.google.com/webhp?hl=en&amp;ictx=2&amp;sa=X&amp;ved=0ahUKEwil_oSxzJj8AhVtEFkFHTHnCGQQPQgI",
    "https://support.google.com/websearch/?p=ws_results_help&amp;hl=en-CA&amp;fg=1",
    "https://en.wikipedia.org/wiki/Dog#Roles_with_humans",
    "https://www.tiktok.com/@aguyandagolden/video/7133277734310038830",
    "https://business.twitter.com/en/help/troubleshooting/how-twitter-ads-work.html?ref=web-twc-ao-gbl-adsinfo&utm_source=twc&utm_medium=web&utm_campaign=ao&utm_content=adsinfo",
    "https://images-na.ssl-images-amazon.com/images/I/41Gc3C8UysL.css?AUIClients/AmazonGatewayAuiAssets",
    "https://www.reddit.com/?after=t3_zvz1ze",
    "https://www.reddit.com/login/?dest=https%3A%2F%2Fwww.reddit.com%2F",
    "postgresql://other:9818274x1!!@localhost:5432/otherdb?connect_timeout=10&application_name=myapp",
    "http://192.168.1.1",            // ipv4
    "http://[2606:4700:4700::1111]", // ipv6
];

fn total_bytes() -> u64 {
    URLS.iter().map(|u| u.len() as u64).sum()
}

/// Parse URLs and get href — matches BasicBench_AdaURL_href in bench.cpp.
pub fn basic_bench_ada_url_href(c: &mut Criterion) {
    let mut group = c.benchmark_group("BasicBench_AdaURL_href");
    group.throughput(Throughput::Bytes(total_bytes()));
    group.bench_function("ada_url", |b| {
        b.iter(|| {
            let mut href_size = 0usize;
            for &url in URLS {
                if let Ok(parsed) = Url::parse(black_box(url), None) {
                    href_size += parsed.href().len();
                }
            }
            black_box(href_size)
        })
    });
    group.bench_function("url", |b| {
        b.iter(|| {
            let mut href_size = 0usize;
            for &url in URLS {
                if let Ok(parsed) = black_box(url).parse::<url::Url>() {
                    href_size += parsed.as_str().len();
                }
            }
            black_box(href_size)
        })
    });
    group.finish();
}

/// Check if URLs can be parsed — matches BasicBench_AdaURL_CanParse in bench.cpp.
pub fn basic_bench_ada_url_can_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("BasicBench_AdaURL_CanParse");
    group.throughput(Throughput::Bytes(total_bytes()));
    group.bench_function("ada_url", |b| {
        b.iter(|| {
            let mut success = 0usize;
            for &url in URLS {
                if Url::can_parse(black_box(url), None) {
                    success += 1;
                }
            }
            black_box(success)
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    basic_bench_ada_url_href,
    basic_bench_ada_url_can_parse
);
criterion_main!(benches);
