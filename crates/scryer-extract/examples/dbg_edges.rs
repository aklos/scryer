use scryer_extract::{extract_context, slice_container};
fn main() {
    let ctx = extract_context(std::path::Path::new(".")).unwrap();
    for c in &ctx.containers {
        let scope = slice_container(&ctx, &c.dir);
        let total = scope.internal_symbol_edges.len();
        let cross = scope
            .internal_symbol_edges
            .iter()
            .filter(|e| {
                e.src.split('#').next() != e.dst.split('#').next()
            })
            .count();
        println!("container '{}': {} internal symbol edges, {} cross-file", c.dir, total, cross);
        for e in scope.internal_symbol_edges.iter().take(3) {
            println!("    sample: {} -> {}", e.src, e.dst);
        }
        println!("    internal_file_edges: {}", scope.internal_file_edges.len());
    }
}
