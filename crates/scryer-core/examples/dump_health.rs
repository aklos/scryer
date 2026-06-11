//! Ad-hoc: dump the health report for a real project's model.
//! `cargo run -p scryer-core --example dump_health -- /path/to/project`

fn main() {
    let project = std::env::args().nth(1).expect("usage: dump_health <project>");
    let r = scryer_core::ModelRef::ProjectLocal(project.clone().into());
    let model = scryer_core::read_model_at(&r).expect("read model");

    let health = scryer_core::health::compute_health(&model, None);
    let t = &health.totals;
    println!(
        "totals: {} resps, {} props | proposed {} impl {} verified {} changed {} | vagrant {} | anchorable {} anchored {} unmapped {}",
        t.responsibilities, t.properties,
        t.statuses.proposed, t.statuses.implemented, t.statuses.verified, t.statuses.changed,
        t.vagrant, t.anchorable, t.anchored, t.unmapped
    );


    if let Some(edges) = scryer_core::build_edges::read_build_edges(&r.build_edges_path()) {
        let g = scryer_core::build_edges::derive_graph(&model, &edges);
        let backed = g.link_audit.iter().filter(|a| a.edge_count > 0).count();
        println!(
            "links: {} declared, {} code-backed, {} asserted-only | unmodeled {}",
            g.link_audit.len(), backed, g.link_audit.len() - backed,
            g.unmodeled.len()
        );
        for e in g.unmodeled.iter().take(8) {
            let name = |id: &str| model.nodes.iter().find(|n| n.id == id).map(|n| n.name.clone()).unwrap_or_default();
            println!("  unmodeled: {} -> {} ({} edges)", name(&e.src), name(&e.dst), e.count);
        }
    } else {
        println!("no .build_edges.json cache");
    }
}
