//! A1 measurement: per-scope Wave 2 payload bytes with and without the
//! embedded `code` evidence. Usage: `cargo run -p scryer-extract --example
//! payload_bytes [repo]`.

fn strip_code(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("code");
            for v in map.values_mut() {
                strip_code(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_code(v);
            }
        }
        _ => {}
    }
}

fn main() {
    let repo = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let ctx = scryer_extract::extract_context(std::path::Path::new(&repo)).expect("extraction");
    let mut total_with = 0usize;
    let mut total_without = 0usize;
    println!("{:<28} {:>6} {:>10} {:>10} {:>7}", "scope", "files", "index", "+code", "growth");
    for c in &ctx.containers {
        let scope = scryer_extract::slice_container(&ctx, &c.dir);
        if scope.files.is_empty() {
            continue;
        }
        let compact = scryer_extract::compact_scope(&scope);
        let mut v = serde_json::to_value(&compact).unwrap();
        let with = serde_json::to_string(&v).unwrap().len();
        strip_code(&mut v);
        let without = serde_json::to_string(&v).unwrap().len();
        total_with += with;
        total_without += without;
        println!(
            "{:<28} {:>6} {:>10} {:>10} {:>6.1}x",
            if c.dir.is_empty() { "(root)" } else { &c.dir },
            scope.files.len(),
            without,
            with,
            with as f64 / without.max(1) as f64,
        );
    }
    println!(
        "{:<28} {:>6} {:>10} {:>10} {:>6.1}x  (~{}k -> ~{}k tokens)",
        "TOTAL",
        "",
        total_without,
        total_with,
        total_with as f64 / total_without.max(1) as f64,
        total_without / 4000,
        total_with / 4000,
    );
}
