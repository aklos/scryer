// Throwaway: run the real validator against a project's model and print every
// warning, so we can see the actual categories instead of guessing.
fn main() {
    let path = std::env::args().nth(1).expect("usage: dump_validate <project>");
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(&path));
    let model = scryer_core::read_model_at(&model_ref).expect("read model");
    let mut warnings = scryer_core::validate::validate(&model);
    warnings.extend(scryer_core::validate::validate_coverage(
        &model,
        std::path::Path::new(&path),
    ));
    println!("total warnings: {}", warnings.len());
    // Bucket by a coarse prefix so categories are obvious.
    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<&str, usize> = BTreeMap::new();
    for w in &warnings {
        let cat = if w.contains("appear disconnected") || w.contains("disconnected") {
            "disconnected"
        } else if w.contains("rejected") || w.contains("same level") || w.contains("not visible") {
            "illegal-link"
        } else if w.contains("Source map") {
            "source-map"
        } else if w.contains("Boundary") {
            "boundary"
        } else {
            "other"
        };
        *buckets.entry(cat).or_default() += 1;
    }
    println!("by category: {buckets:?}");
    println!("---");
    for w in &warnings {
        println!("- {w}");
    }
}
