use ada_url::Url;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

/// Realistic URL examples collected from the BBC homepage.
/// Matches url_examples in bbc_bench.cpp.
const BBC_URLS: &[&str] = &[
    "https://static.files.bbci.co.uk/orbit/737a4ee2bed596eb65afc4d2ce9af568/js/polyfills.js",
    "https://static.files.bbci.co.uk/orbit/737a4ee2bed596eb65afc4d2ce9af568/css/orbit-v5-ltr.min.css",
    "https://static.files.bbci.co.uk/orbit/737a4ee2bed596eb65afc4d2ce9af568/js/require.min.js",
    "https://static.files.bbci.co.uk/fonts/reith/2.512/BBCReithSans_W_Rg.woff2",
    "https://nav.files.bbci.co.uk/searchbox/c8bfe8595e453f2b9483fda4074e9d15/css/box.css",
    "https://static.files.bbci.co.uk/cookies/d3bb303e79f041fec95388e04f84e716/cookie-banner/cookie-library.bundle.js",
    "https://static.files.bbci.co.uk/account/id-cta/597/style/id-cta.css",
    "https://gn-web-assets.api.bbc.com/wwhp/20220908-1153-091014d07889c842a7bdc06e00fa711c9e04f049/responsive/css/old-ie.min.css",
    "https://gn-web-assets.api.bbc.com/wwhp/20220908-1153-091014d07889c842a7bdc06e00fa711c9e04f049/modules/vendor/bower/modernizr/modernizr.js",
];

fn total_bytes() -> u64 {
    BBC_URLS.iter().map(|u| u.len() as u64).sum()
}

/// Parse BBC URLs and get href — matches BBC_BasicBench_AdaURL_href in bbc_bench.cpp.
pub fn bbc_basic_bench_ada_url_href(c: &mut Criterion) {
    let mut group = c.benchmark_group("BBC_BasicBench_AdaURL_href");
    group.throughput(Throughput::Bytes(total_bytes()));
    group.bench_function("ada_url", |b| {
        b.iter(|| {
            let mut href_size = 0usize;
            for &url in BBC_URLS {
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
            for &url in BBC_URLS {
                if let Ok(parsed) = black_box(url).parse::<url::Url>() {
                    href_size += parsed.as_str().len();
                }
            }
            black_box(href_size)
        })
    });
    group.finish();
}

/// Check if BBC URLs can be parsed — matches BBC_BasicBench_AdaURL_CanParse in bbc_bench.cpp.
pub fn bbc_basic_bench_ada_url_can_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("BBC_BasicBench_AdaURL_CanParse");
    group.throughput(Throughput::Bytes(total_bytes()));
    group.bench_function("ada_url", |b| {
        b.iter(|| {
            let mut success = 0usize;
            for &url in BBC_URLS {
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
    bbc_basic_bench_ada_url_href,
    bbc_basic_bench_ada_url_can_parse
);
criterion_main!(benches);
