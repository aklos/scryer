//! Experiment: how good is DETERMINISTIC component clustering?
//!
//! Per container, clusters owned files three ways and prints the results
//! side by side so a human can judge them against "what components would I
//! actually draw":
//!   A. directory  — files grouped by immediate parent dir (the naive prior)
//!   B. louvain    — community detection on the symbol-edge call graph alone
//!   C. hybrid     — louvain on call graph + weak same-dir affinity edges
//!
//! Run: cargo run -p scryer-extract --example cluster -- /path/to/repo

use scryer_extract::{extract_context, slice_container};
use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    let repo = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let t0 = std::time::Instant::now();
    let ctx = extract_context(&repo).expect("extraction failed");
    eprintln!(
        "extracted {} containers, {} files, {} symbol-edges in {:?}\n",
        ctx.containers.len(),
        ctx.files.len(),
        ctx.symbol_edges.len(),
        t0.elapsed()
    );

    for container in &ctx.containers {
        let scope = slice_container(&ctx, &container.dir);
        if scope.files.len() < 2 {
            continue;
        }
        println!(
            "================================================================"
        );
        println!(
            "CONTAINER '{}' (dir='{}')  {} files",
            container.name,
            container.dir,
            scope.files.len()
        );

        // File index + symbol counts.
        let paths: Vec<&str> = scope.files.iter().map(|f| f.rel_path.as_str()).collect();
        let index: HashMap<&str, usize> =
            paths.iter().enumerate().map(|(i, p)| (*p, i)).collect();
        let sym_count: Vec<usize> = scope.files.iter().map(|f| f.symbols.len()).collect();

        // Weighted file-file graph. Cross-file dependencies live in
        // file_edges (symbol_edges are intra-file only).
        let mut call_w: HashMap<(usize, usize), f64> = HashMap::new();
        for e in &scope.internal_file_edges {
            let (Some(&a), Some(&b)) = (index.get(e.src.as_str()), index.get(e.dst.as_str()))
            else {
                continue;
            };
            if a == b {
                continue;
            }
            let key = if a < b { (a, b) } else { (b, a) };
            *call_w.entry(key).or_default() += 1.0;
        }

        // A. directory baseline
        let dir_clusters = cluster_by_dir(&paths, &container.dir);
        print_clusters("A. directory", &dir_clusters, &paths, &sym_count);

        // B. pure call-graph louvain
        let labels = louvain(paths.len(), &call_w);
        print_clusters(
            "B. louvain (call graph only)",
            &group(&labels),
            &paths,
            &sym_count,
        );

        // C. hybrid: call graph + weak same-dir affinity
        let mut hybrid_w = call_w.clone();
        let by_dir = files_by_dir(&paths);
        for members in by_dir.values() {
            for i in 0..members.len() {
                for j in (i + 1)..members.len() {
                    let key = (members[i].min(members[j]), members[i].max(members[j]));
                    *hybrid_w.entry(key).or_default() += 0.5;
                }
            }
        }
        let labels = louvain(paths.len(), &hybrid_w);
        print_clusters(
            "C. hybrid (call graph + dir prior)",
            &group(&labels),
            &paths,
            &sym_count,
        );
    }
    eprintln!("\ntotal wall time: {:?}", t0.elapsed());
}

fn file_of(key: &str) -> Option<&str> {
    key.split('#').next()
}

fn files_by_dir<'a>(paths: &[&'a str]) -> HashMap<&'a str, Vec<usize>> {
    let mut by_dir: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, p) in paths.iter().enumerate() {
        let dir = p.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        by_dir.entry(dir).or_default().push(i);
    }
    by_dir
}

fn cluster_by_dir(paths: &[&str], container_dir: &str) -> Vec<Vec<usize>> {
    // Group by first path segment below the container's source root, so
    // "src/tools/x.rs" and "src/tools/y.rs" land together while "src/a.rs"
    // joins the root bucket.
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, p) in paths.iter().enumerate() {
        let rel = p
            .strip_prefix(container_dir)
            .unwrap_or(p)
            .trim_start_matches('/');
        let rel = rel.strip_prefix("src/").unwrap_or(rel);
        let bucket = match rel.rsplit_once('/') {
            Some((dirs, _)) => dirs.to_string(),
            None => "(root)".to_string(),
        };
        groups.entry(bucket).or_default().push(i);
    }
    let mut out: Vec<Vec<usize>> = groups.into_values().collect();
    out.sort_by_key(|g| std::cmp::Reverse(g.len()));
    out
}

fn group(labels: &[usize]) -> Vec<Vec<usize>> {
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, &l) in labels.iter().enumerate() {
        groups.entry(l).or_default().push(i);
    }
    let mut out: Vec<Vec<usize>> = groups.into_values().collect();
    out.sort_by_key(|g| std::cmp::Reverse(g.len()));
    out
}

fn print_clusters(title: &str, clusters: &[Vec<usize>], paths: &[&str], syms: &[usize]) {
    println!("\n--- {title}: {} clusters ---", clusters.len());
    let mut singles: Vec<usize> = Vec::new();
    for cluster in clusters {
        if cluster.len() == 1 {
            singles.push(cluster[0]);
            continue;
        }
        let total: usize = cluster.iter().map(|&i| syms[i]).sum();
        println!("  [{} files, {} symbols]", cluster.len(), total);
        let mut sorted = cluster.clone();
        sorted.sort_by_key(|&i| paths[i]);
        for i in sorted {
            println!("      {} ({})", paths[i], syms[i]);
        }
    }
    if !singles.is_empty() {
        singles.sort_by_key(|&i| paths[i]);
        let names: Vec<&str> = singles.iter().map(|&i| paths[i]).collect();
        println!("  [singletons: {}]", names.join(", "));
    }
}

/// Plain Louvain: greedy modularity local-moving + aggregation, deterministic
/// node order. Weighted undirected graph given as upper-triangle pair weights.
fn louvain(n: usize, weights: &HashMap<(usize, usize), f64>) -> Vec<usize> {
    // current community label per ORIGINAL node
    let mut node_label: Vec<usize> = (0..n).collect();
    // current aggregated graph: adjacency of supernodes
    let mut adj: Vec<HashMap<usize, f64>> = vec![HashMap::new(); n];
    for (&(a, b), &w) in weights {
        *adj[a].entry(b).or_default() += w;
        *adj[b].entry(a).or_default() += w;
    }
    // self-loop weights of supernodes (internal weight after aggregation)
    let mut self_w: Vec<f64> = vec![0.0; n];
    // map original node -> current supernode
    let mut assign: Vec<usize> = (0..n).collect();

    loop {
        let m2: f64 = adj
            .iter()
            .map(|nb| nb.values().sum::<f64>())
            .sum::<f64>()
            + self_w.iter().sum::<f64>() * 2.0;
        if m2 <= 0.0 {
            break;
        }
        let sn = adj.len();
        let k: Vec<f64> = (0..sn)
            .map(|i| adj[i].values().sum::<f64>() + 2.0 * self_w[i])
            .collect();
        let mut community: Vec<usize> = (0..sn).collect();
        let mut comm_tot: Vec<f64> = k.clone();
        let mut moved_any = false;

        let mut improved = true;
        while improved {
            improved = false;
            for i in 0..sn {
                let ci = community[i];
                comm_tot[ci] -= k[i];
                // weight from i to each neighboring community
                let mut to_comm: HashMap<usize, f64> = HashMap::new();
                for (&j, &w) in &adj[i] {
                    if j != i {
                        *to_comm.entry(community[j]).or_default() += w;
                    }
                }
                let base = to_comm.get(&ci).copied().unwrap_or(0.0)
                    - comm_tot[ci] * k[i] / m2;
                let mut best = (ci, 0.0f64);
                let mut cands: Vec<(usize, f64)> = to_comm.into_iter().collect();
                cands.sort_by_key(|&(c, _)| c); // deterministic
                for (c, w) in cands {
                    if c == ci {
                        continue;
                    }
                    let gain = (w - comm_tot[c] * k[i] / m2) - base;
                    if gain > best.1 + 1e-12 {
                        best = (c, gain);
                    }
                }
                community[i] = best.0;
                comm_tot[best.0] += k[i];
                if best.0 != ci {
                    improved = true;
                    moved_any = true;
                }
            }
        }
        if !moved_any {
            break;
        }

        // renumber communities densely
        let mut renum: HashMap<usize, usize> = HashMap::new();
        for &c in &community {
            let next = renum.len();
            renum.entry(c).or_insert(next);
        }
        let nc = renum.len();

        // propagate labels back to original nodes
        for orig in 0..n {
            let sn_id = assign[orig];
            assign[orig] = renum[&community[sn_id]];
        }
        node_label.clone_from(&assign);

        if nc == sn {
            break;
        }
        // aggregate graph
        let mut new_adj: Vec<HashMap<usize, f64>> = vec![HashMap::new(); nc];
        let mut new_self: Vec<f64> = vec![0.0; nc];
        for i in 0..sn {
            let ci = renum[&community[i]];
            new_self[ci] += self_w[i];
            for (&j, &w) in &adj[i] {
                let cj = renum[&community[j]];
                if ci == cj {
                    if i < j {
                        new_self[ci] += w;
                    }
                } else {
                    *new_adj[ci].entry(cj).or_default() += w;
                }
            }
        }
        adj = new_adj;
        self_w = new_self;
    }
    node_label
}
