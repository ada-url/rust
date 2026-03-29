use std::time::Instant;

fn main() {
    let raw = std::fs::read_to_string("tests/wpt/urltestdata.json").unwrap();
    let data: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let entries: Vec<_> = data.as_array().unwrap().iter()
        .filter_map(|e| e.as_object())
        .filter_map(|o| {
            let input = o.get("input")?.as_str()?;
            let base = o.get("base").and_then(|b| b.as_str()).unwrap_or("");
            Some((input.to_string(), base.to_string()))
        })
        .collect();

    let n = 2000u32;
    // warm up
    for _ in 0..200 {
        for (inp, base) in &entries {
            let b = if base.is_empty() { None } else { Some(base.as_str()) };
            let _ = ada_url::Url::parse(inp.as_str(), b);
        }
    }

    // Timed run
    let t = Instant::now();
    for _ in 0..n {
        for (inp, base) in &entries {
            let b = if base.is_empty() { None } else { Some(std::hint::black_box(base.as_str())) };
            let _ = std::hint::black_box(ada_url::Url::parse(std::hint::black_box(inp.as_str()), b));
        }
    }
    let elapsed = t.elapsed();
    let per_iter = elapsed.as_nanos() as f64 / n as f64;
    let per_url  = per_iter / entries.len() as f64;
    println!("Total per iteration: {:.1}µs  ({} entries, {:.1}ns/url)", per_iter/1000.0, entries.len(), per_url);

    // Now simulate the benchmark (with base re-parse like the bench does)
    let t2 = Instant::now();
    for _ in 0..n {
        let mut href_size = 0usize;
        for (input, base) in &entries {
            let parsed = if !base.is_empty() {
                match ada_url::Url::parse(std::hint::black_box(base.as_str()), None::<&str>) {
                    Ok(base_url) => {
                        let base_href = base_url.href().to_owned();
                        ada_url::Url::parse(std::hint::black_box(input.as_str()), Some(base_href.as_str()))
                    }
                    Err(_) => continue,
                }
            } else {
                ada_url::Url::parse(std::hint::black_box(input.as_str()), None::<&str>)
            };
            if let Ok(url) = parsed { href_size += url.href().len(); }
        }
        std::hint::black_box(href_size);
    }
    let e2 = t2.elapsed();
    let per_bench_iter = e2.as_nanos() as f64 / n as f64;
    println!("Benchmark simulation:  {:.1}µs per iteration", per_bench_iter/1000.0);
    println!("Overhead from double-parse: {:.1}µs ({:.1}%)", (per_bench_iter - per_iter)/1000.0, (per_bench_iter - per_iter) / per_iter * 100.0);
}
// This won't compile as-is, use a separate test
