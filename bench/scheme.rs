/// Benchmark for scheme type detection methods.
/// Matches bench_protocol.cpp — benchmarks different approaches to detect URL scheme types.
///
/// Reference:
/// Daniel Lemire, "Quickly checking that a string belongs to a small set,"
/// https://lemire.me/blog/2022/12/30/quickly-checking-that-a-string-belongs-to-a-small-set/
use ada_url::{SchemeType, Url};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::collections::{BTreeMap, HashMap};
use std::num::Wrapping;

/// Matches the scheme options and weights from bench_protocol.cpp:
/// std::discrete_distribution<> d({20, 10, 10, 5, 5, 5}) over {"http","https","ftp","ws","wss","file"}.
const SCHEME_OPTIONS: &[&str] = &["http", "https", "ftp", "ws", "wss", "file"];
/// Weights sum = 55; cumulative: 20, 30, 40, 45, 50, 55
const WEIGHTS_CUMULATIVE: &[u32] = &[20, 30, 40, 45, 50, 55];
const WEIGHT_TOTAL: u32 = 55;

/// Simple xorshift64 for reproducible random generation.
struct Xorshift64 {
    state: Wrapping<u64>,
}

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        // Splitmix64 initialization
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

    /// Pick a scheme string according to the weighted distribution.
    fn next_scheme(&mut self) -> &'static str {
        let val = (self.next_u64() % WEIGHT_TOTAL as u64) as u32;
        for (i, &cum) in WEIGHTS_CUMULATIVE.iter().enumerate() {
            if val < cum {
                return SCHEME_OPTIONS[i];
            }
        }
        SCHEME_OPTIONS[0]
    }
}

/// Populate a vector of scheme strings with the given count, using a weighted distribution.
/// Matches the populate() function in bench_protocol.cpp.
fn populate(count: usize) -> Vec<&'static str> {
    let mut rng = Xorshift64::new(12345); // non-deterministic in C++, we use fixed seed
    (0..count).map(|_| rng.next_scheme()).collect()
}

/// Naive sequential if/else chain — matches get_scheme_type_naive in bench_protocol.cpp.
#[inline]
fn get_scheme_type_naive(input: &str) -> Option<SchemeType> {
    if input == "http" {
        Some(SchemeType::Http)
    } else if input == "https" {
        Some(SchemeType::Https)
    } else if input == "ftp" {
        Some(SchemeType::Ftp)
    } else if input == "ws" {
        Some(SchemeType::Ws)
    } else if input == "wss" {
        Some(SchemeType::Wss)
    } else if input == "file" {
        Some(SchemeType::File)
    } else {
        None
    }
}

/// Scheme type detection via URL parsing — matches the "ada" method in bench_protocol.cpp.
/// This actually parses a full URL to get the scheme type.
#[inline]
fn get_scheme_type_ada(scheme: &str) -> Option<SchemeType> {
    // Build a minimal URL for the given scheme
    let url_str = format!("{}://example.com/", scheme);
    Url::parse(&url_str, None).ok().map(|u| u.scheme_type())
}

const NUM_STRINGS: usize = 200_000;

/// Benchmark naive if/else chain — matches "naive" in bench_protocol.cpp.
pub fn bench_scheme_naive(c: &mut Criterion) {
    let strings = populate(NUM_STRINGS);
    let bytes: u64 = strings.iter().map(|s| s.len() as u64).sum();

    let mut group = c.benchmark_group("scheme_detection");
    group.throughput(Throughput::Bytes(bytes));

    group.bench_function("naive", |b| {
        b.iter(|| {
            let mut count = 0usize;
            for &s in &strings {
                if let Some(_t) = get_scheme_type_naive(black_box(s)) {
                    count += 1;
                }
            }
            black_box(count)
        })
    });

    let hash_map: HashMap<&str, SchemeType> = [
        ("http", SchemeType::Http),
        ("https", SchemeType::Https),
        ("ftp", SchemeType::Ftp),
        ("ws", SchemeType::Ws),
        ("wss", SchemeType::Wss),
        ("file", SchemeType::File),
    ]
    .into_iter()
    .collect();

    // Matches std::unordered_map in bench_protocol.cpp.
    group.bench_function("hash_map", |b| {
        b.iter(|| {
            let mut count = 0usize;
            for &s in &strings {
                if hash_map.get(black_box(s)).is_some() {
                    count += 1;
                }
            }
            black_box(count)
        })
    });

    let btree_map: BTreeMap<&str, SchemeType> = [
        ("http", SchemeType::Http),
        ("https", SchemeType::Https),
        ("ftp", SchemeType::Ftp),
        ("ws", SchemeType::Ws),
        ("wss", SchemeType::Wss),
        ("file", SchemeType::File),
    ]
    .into_iter()
    .collect();

    // Matches std::map in bench_protocol.cpp.
    group.bench_function("btree_map", |b| {
        b.iter(|| {
            let mut count = 0usize;
            for &s in &strings {
                if btree_map.get(black_box(s)).is_some() {
                    count += 1;
                }
            }
            black_box(count)
        })
    });

    // Matches "ada" (get_scheme_type from ada::scheme) in bench_protocol.cpp.
    // We use URL parsing to exercise the same code path.
    group.bench_function("ada_url_parse", |b| {
        b.iter(|| {
            let mut count = 0usize;
            for &s in &strings {
                if get_scheme_type_ada(black_box(s)).is_some() {
                    count += 1;
                }
            }
            black_box(count)
        })
    });

    group.finish();
}

criterion_group!(benches, bench_scheme_naive);
criterion_main!(benches);
