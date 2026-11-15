use ada_url::Idna;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

/// International domain examples for IDNA benchmarking.
const DOMAINS: &[&str] = &[
    "meßagefactory.ca",
    "xn--meagefactory-m9a.ca",
    "bücher.example",
    "example.com",
    "xn--zca.xn--zca",
    "xn--zca.ß",
    "ab--c.ß",
    "xn--10cl1a0b660p",
    "xn--mgba3gch31f060k",
    "مثال.إختبار",
    "a†--",
    "xn--a---kp0a",
    "xn--a-yoc",
    "xn--1ug.example",
    "www.google.com",
    "日本語.jp",
];

fn total_bytes() -> u64 {
    DOMAINS.iter().map(|d| d.len() as u64).sum()
}

/// Convert domains to ASCII — compares `ada_url::Idna` with the `idna` crate.
pub fn idna_bench_to_ascii(c: &mut Criterion) {
    let mut group = c.benchmark_group("IdnaBench_ToASCII");
    group.throughput(Throughput::Bytes(total_bytes()));
    group.bench_function("ada_url", |b| {
        b.iter(|| {
            let mut out_len = 0usize;
            for &domain in DOMAINS {
                out_len += Idna::ascii(black_box(domain)).len();
            }
            black_box(out_len)
        })
    });
    group.bench_function("idna", |b| {
        b.iter(|| {
            let mut out_len = 0usize;
            for &domain in DOMAINS {
                let converted = idna::domain_to_ascii(black_box(domain)).unwrap_or_default();
                out_len += converted.len();
            }
            black_box(out_len)
        })
    });
    group.finish();
}

/// Convert domains to Unicode — compares `ada_url::Idna` with the `idna` crate.
pub fn idna_bench_to_unicode(c: &mut Criterion) {
    let mut group = c.benchmark_group("IdnaBench_ToUnicode");
    group.throughput(Throughput::Bytes(total_bytes()));
    group.bench_function("ada_url", |b| {
        b.iter(|| {
            let mut out_len = 0usize;
            for &domain in DOMAINS {
                out_len += Idna::unicode(black_box(domain)).len();
            }
            black_box(out_len)
        })
    });
    group.bench_function("idna", |b| {
        b.iter(|| {
            let mut out_len = 0usize;
            for &domain in DOMAINS {
                let (converted, _) = idna::domain_to_unicode(black_box(domain));
                out_len += converted.len();
            }
            black_box(out_len)
        })
    });
    group.finish();
}

criterion_group!(benches, idna_bench_to_ascii, idna_bench_to_unicode);
criterion_main!(benches);
