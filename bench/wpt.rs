/// WPT (Web Platform Tests) URL benchmark.
/// Matches wpt_bench.cpp — parses URLs with base URLs from the WPT urltestdata.json.
///
/// Usage: cargo bench --bench wpt
/// The benchmark reads tests/wpt/urltestdata.json by default.
/// If the file is not found, the benchmark is skipped.
use ada_url::Url;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::path::Path;

/// Load WPT URL test data from the given JSON file.
/// Returns pairs of (input, base) strings, matching the structure used in wpt_bench.cpp.
fn load_wpt_data(path: &Path) -> Vec<(String, String)> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let data: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut result = Vec::new();
    if let Some(arr) = data.as_array() {
        for item in arr {
            if let Some(obj) = item.as_object() {
                let input = match obj.get("input").and_then(|v| v.as_str()) {
                    Some(s) => s.to_owned(),
                    None => continue,
                };
                let base = obj
                    .get("base")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                result.push((input, base));
            }
        }
    }
    result
}

/// Parse URLs with base — matches WptBench_BasicBench_AdaURL_url in wpt_bench.cpp.
pub fn wpt_bench_ada_url(c: &mut Criterion) {
    // Try standard locations, matching how wpt_bench.cpp is invoked in the ada project.
    let search_paths = [
        "tests/wpt/urltestdata.json",
        "../ada/tests/wpt/urltestdata.json",
    ];

    let mut url_examples = Vec::new();
    for path_str in &search_paths {
        let path = Path::new(path_str);
        let data = load_wpt_data(path);
        if !data.is_empty() {
            url_examples = data;
            eprintln!("Loaded {} WPT URL entries from {}", url_examples.len(), path_str);
            break;
        }
    }

    if url_examples.is_empty() {
        eprintln!("WPT benchmark: no test data found, skipping.");
        eprintln!("  Expected: tests/wpt/urltestdata.json");
        return;
    }

    let bytes: u64 = url_examples
        .iter()
        .map(|(i, b)| (i.len() + b.len()) as u64)
        .sum();

    let mut group = c.benchmark_group("WptBench_BasicBench_AdaURL");
    group.throughput(Throughput::Bytes(bytes));

    group.bench_function("ada_url", |b| {
        b.iter(|| {
            let mut href_size = 0usize;
            for (input, base) in &url_examples {
                // Mirror the C++ logic: if base is non-empty, pre-parse it and use its href.
                // If base parse fails, skip this entry (matching the C++ `continue`).
                let parsed = if !base.is_empty() {
                    match Url::parse(black_box(base.as_str()), None) {
                        Ok(base_url) => {
                            // Pass the base URL's href string as the base for input parsing.
                            // This is equivalent to using a pre-parsed base pointer in C++.
                            let base_href = base_url.href().to_owned();
                            Url::parse(black_box(input.as_str()), Some(&base_href))
                        }
                        Err(_) => continue,
                    }
                } else {
                    Url::parse(black_box(input.as_str()), None)
                };
                if let Ok(url) = parsed {
                    href_size += url.href().len();
                }
            }
            black_box(href_size)
        })
    });

    group.bench_function("url", |b| {
        b.iter(|| {
            let mut href_size = 0usize;
            for (input, base) in &url_examples {
                let base_url = if !base.is_empty() {
                    match black_box(base.as_str()).parse::<url::Url>() {
                        Ok(u) => Some(u),
                        Err(_) => continue,
                    }
                } else {
                    None
                };
                let parsed = if let Some(ref b) = base_url {
                    b.join(black_box(input.as_str()))
                } else {
                    black_box(input.as_str()).parse::<url::Url>()
                };
                if let Ok(url) = parsed {
                    href_size += url.as_str().len();
                }
            }
            black_box(href_size)
        })
    });

    group.finish();
}

criterion_group!(benches, wpt_bench_ada_url);
criterion_main!(benches);
