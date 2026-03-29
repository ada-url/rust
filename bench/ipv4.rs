use ada_url::Url;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::num::Wrapping;

/// Non-decimal IPv4 URL examples — matches kIpv4NonDecimalUrls in bench_ipv4.cpp.
const IPV4_NON_DECIMAL_URLS: &[&str] = &[
    "http://0x7f.0x0.0x0.0x1",
    "http://0177.000.000.001",
    "http://0x7f.1.2.03",
    "http://0x7f.000.00.000",
    "http://000.000.000.000",
    "http://0x.0x.0x.0x",
    "http://0300.0250.0001.0001",
    "http://0xc0.0xa8.0x01.0x01",
    "http://3232235777",
    "http://0xc0a80101",
    "http://030052000401",
    "http://127.1",
    "http://127.0.1",
    "http://0x7f.1",
    "http://0177.1",
    "http://0300.0xa8.1.1",
    "http://192.168.0x1.01",
    "http://0x0.0x0.0x0.0x0",
    "http://0.0.0.0x0",
    "http://022.022.022.022",
    "http://0x12.0x12.0x12.0x12",
    "http://0xff.0xff.0xff.0xff",
    "http://0377.0377.0377.0377",
    "http://4294967295",
    "http://0xffffffff",
    "http://0x00.0x00.0x00.0x00",
    "http://00000.00000.00000.00000",
    "http://1.0x2.03.4",
    "http://0x1.2.0x3.4",
    "http://0.01.0x02.3",
];

/// DNS fallback URL examples — matches kDnsFallbackUrls in bench_ipv4.cpp.
const DNS_FALLBACK_URLS: &[&str] = &[
    "http://example.com",
    "http://www.google.com",
    "http://localhost",
    "http://foo.bar",
    "http://github.com",
    "http://microsoft.com",
    "http://aws.amazon.com",
    "http://adaparser.com",
    "http://www.wikipedia.org",
    "http://www.apple.com",
    "http://www.amazon.com",
    "http://www.facebook.com",
    "http://www.twitter.com",
    "http://www.instagram.com",
    "http://www.linkedin.com",
    "http://www.reddit.com",
    "http://www.netflix.com",
    "http://www.youtube.com",
    "http://www.bing.com",
    "http://www.yahoo.com",
];

/// Simple xorshift64 RNG for reproducible data generation.
/// Matches the sequence quality of std::mt19937 in bench_ipv4.cpp (seed 42).
struct Xorshift64 {
    state: Wrapping<u64>,
}

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        // Splitmix64 init to avoid zero state
        let mut s = Wrapping(seed);
        s += Wrapping(0x9e3779b97f4a7c15u64);
        s = (s ^ (s >> 30)) * Wrapping(0xbf58476d1ce4e5b9u64);
        s = (s ^ (s >> 27)) * Wrapping(0x94d049bb133111ebu64);
        s ^= s >> 31;
        Self { state: s }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x.0
    }

    fn next_octet(&mut self) -> u8 {
        (self.next_u64() & 0xff) as u8
    }
}

/// Generate 5000 random decimal IPv4 URLs — matches GetDecimalWorkload in bench_ipv4.cpp.
fn generate_decimal_ipv4(count: usize) -> Vec<String> {
    let mut rng = Xorshift64::new(42);
    (0..count)
        .map(|_| {
            format!(
                "http://{}.{}.{}.{}",
                rng.next_octet(),
                rng.next_octet(),
                rng.next_octet(),
                rng.next_octet()
            )
        })
        .collect()
}

/// Build permutation with Knuth shuffle, using a fixed seed — matches make_permutation.
fn make_permutation(count: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..count).collect();
    if count < 2 {
        return order;
    }
    let mut rng = Xorshift64::new(seed);
    for i in (1..count).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        order.swap(i, j);
    }
    order
}

/// Build coprime strides — matches make_strides in bench_ipv4.cpp.
fn make_strides(count: usize) -> Vec<usize> {
    let mut strides = Vec::new();
    if count > 1 {
        let limit = count.min(100);
        for s in 1..limit {
            if gcd(s, count) == 1 {
                strides.push(s);
            }
        }
    }
    if strides.is_empty() {
        strides.push(1);
    }
    strides
}

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Core benchmark runner — matches run_benchmark in bench_ipv4.cpp.
fn run_benchmark(c: &mut Criterion, group_name: &str, urls: &[String]) {
    if urls.is_empty() {
        return;
    }
    let bytes: u64 = urls.iter().map(|u| u.len() as u64).sum();
    let count = urls.len();

    let order = make_permutation(count, 0x12345678);
    let strides = make_strides(count);

    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Bytes(bytes));

    let mut iter = 0usize;
    group.bench_function("ada_url", |b| {
        b.iter(|| {
            let stride = strides[iter % strides.len()];
            let mut pos = iter % count;
            let mut success = 0usize;
            for _ in 0..count {
                let result = Url::parse(black_box(urls[order[pos]].as_str()), None);
                if result.is_ok() {
                    success += 1;
                }
                pos += stride;
                if pos >= count {
                    pos -= count;
                }
            }
            black_box(success);
            iter = iter.wrapping_add(1);
        })
    });
    group.finish();
}

fn run_benchmark_static(c: &mut Criterion, group_name: &str, urls: &[&str]) {
    if urls.is_empty() {
        return;
    }
    let bytes: u64 = urls.iter().map(|u| u.len() as u64).sum();
    let count = urls.len();

    let order = make_permutation(count, 0x12345678);
    let strides = make_strides(count);

    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Bytes(bytes));

    let mut iter = 0usize;
    group.bench_function("ada_url", |b| {
        b.iter(|| {
            let stride = strides[iter % strides.len()];
            let mut pos = iter % count;
            let mut success = 0usize;
            for _ in 0..count {
                let result = Url::parse(black_box(urls[order[pos]]), None);
                if result.is_ok() {
                    success += 1;
                }
                pos += stride;
                if pos >= count {
                    pos -= count;
                }
            }
            black_box(success);
            iter = iter.wrapping_add(1);
        })
    });
    group.finish();
}

/// Benchmark decimal IPv4 URL parsing — matches Bench_IPv4_Decimal_AdaURL in bench_ipv4.cpp.
pub fn bench_ipv4_decimal(c: &mut Criterion) {
    let urls = generate_decimal_ipv4(5000);
    run_benchmark(c, "Bench_IPv4_Decimal", &urls);
}

/// Benchmark non-decimal IPv4 URL parsing — matches Bench_IPv4_NonDecimal_AdaURL in bench_ipv4.cpp.
pub fn bench_ipv4_non_decimal(c: &mut Criterion) {
    // Repeat fixed non-decimal set to 5000 entries, matching GetNonDecimalWorkload
    let src_len = IPV4_NON_DECIMAL_URLS.len();
    let urls: Vec<&str> = (0..5000).map(|i| IPV4_NON_DECIMAL_URLS[i % src_len]).collect();
    run_benchmark_static(c, "Bench_IPv4_NonDecimal", &urls);
}

/// Benchmark DNS hostname URL parsing — matches Bench_DNS_AdaURL in bench_ipv4.cpp.
pub fn bench_dns(c: &mut Criterion) {
    // Repeat fixed DNS set to 2000 entries, matching GetDnsWorkload fallback
    let src_len = DNS_FALLBACK_URLS.len();
    let urls: Vec<&str> = (0..2000).map(|i| DNS_FALLBACK_URLS[i % src_len]).collect();
    run_benchmark_static(c, "Bench_DNS", &urls);
}

criterion_group!(benches, bench_ipv4_decimal, bench_ipv4_non_decimal, bench_dns);
criterion_main!(benches);
