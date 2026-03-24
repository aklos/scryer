use rmcp::ErrorData as McpError;
use scryer_core::{C4Edge, C4Kind, C4ModelData, C4Node, C4Shape, Flow, Status};
use std::collections::HashMap;

/// Recursively collect all steps (flattened) from a step tree.
pub(crate) fn collect_all_steps(steps: &[scryer_core::FlowStep]) -> Vec<&scryer_core::FlowStep> {
    let mut result = Vec::new();
    for step in steps {
        result.push(step);
        for branch in &step.branches {
            result.extend(collect_all_steps(&branch.steps));
        }
    }
    result
}

/// Recursively migrate label → description on steps that have label but no description.
pub(crate) fn migrate_flow_labels(steps: &mut [scryer_core::FlowStep]) {
    for step in steps.iter_mut() {
        if step.description.is_none() {
            if let Some(lbl) = step.label.take() {
                step.description = Some(lbl);
            }
        }
        for branch in &mut step.branches {
            migrate_flow_labels(&mut branch.steps);
        }
    }
}

/// Recursively strip UI-only fields (position, type, refPositions) from a JSON value.
pub(crate) fn strip_ui_fields(val: &mut serde_json::Value) {
    strip_fields(val, false);
}

pub(crate) fn strip_fields_compact(val: &mut serde_json::Value) {
    strip_fields(val, true);
}

fn strip_fields(val: &mut serde_json::Value, compact: bool) {
    match val {
        serde_json::Value::Object(map) => {
            // Always strip UI-only fields
            map.remove("position");
            map.remove("type");
            map.remove("refPositions");

            if compact {
                // Strip notes (available via get_node/get_task)
                map.remove("notes");
                // Strip empty strings
                map.retain(|_, v| !matches!(v, serde_json::Value::String(s) if s.is_empty()));
                // Strip nulls
                map.retain(|_, v| !v.is_null());
                // Strip empty arrays
                map.retain(|_, v| !matches!(v, serde_json::Value::Array(a) if a.is_empty()));
                // Strip empty objects
                map.retain(|_, v| !matches!(v, serde_json::Value::Object(m) if m.is_empty()));
            }

            for (_, v) in map.iter_mut() {
                strip_fields(v, compact);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_fields(v, compact);
            }
        }
        _ => {}
    }
}

/// Externalize contract image base64 data to temp files so AI context isn't bloated.
/// Walks JSON looking for contract items with "image" objects, writes the data to
/// a temp file, and replaces "data" with "path".
pub(crate) fn externalize_attachments(val: &mut serde_json::Value, model_name: &str) {
    match val {
        serde_json::Value::Object(map) => {
            // Contract item with image: { text, image: { filename, mimeType, data } }
            if map.contains_key("text") {
                if let Some(serde_json::Value::Object(img)) = map.get_mut("image") {
                    let mime = img.get("mimeType").and_then(|v| v.as_str()).unwrap_or("image/png").to_string();
                    let filename = img.get("filename").and_then(|v| v.as_str()).unwrap_or("image").to_string();
                    let ext = match mime.as_str() {
                        "image/png" => "png",
                        "image/jpeg" | "image/jpg" => "jpg",
                        "image/gif" => "gif",
                        "image/webp" => "webp",
                        "image/svg+xml" => "svg",
                        _ => "bin",
                    };
                    if let Some(serde_json::Value::String(b64)) = img.get("data") {
                        if let Ok(bytes) = base64_decode(b64) {
                            let tmp_dir = std::env::temp_dir().join("scryer-attachments").join(model_name);
                            let _ = std::fs::create_dir_all(&tmp_dir);
                            let out_name = format!("{}.{}", filename, ext);
                            let out_path = tmp_dir.join(&out_name);
                            if std::fs::write(&out_path, &bytes).is_ok() {
                                img.remove("data");
                                img.insert("path".to_string(), serde_json::Value::String(out_path.to_string_lossy().to_string()));
                            }
                        }
                    }
                }
            }
            for (_, v) in map.iter_mut() {
                externalize_attachments(v, model_name);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                externalize_attachments(v, model_name);
            }
        }
        _ => {}
    }
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| e.to_string())
}

pub(crate) fn parse_kind(s: &str) -> Result<C4Kind, McpError> {
    match s {
        "person" => Ok(C4Kind::Person),
        "system" => Ok(C4Kind::System),
        "container" => Ok(C4Kind::Container),
        "component" => Ok(C4Kind::Component),
        "operation" | "member" => Ok(C4Kind::Operation),
        "process" => Ok(C4Kind::Process),
        "model" => Ok(C4Kind::Model),
        _ => Err(McpError::invalid_params(
            format!(
                "Invalid kind '{}'. Must be: person, system, container, component, operation, process, model",
                s
            ),
            None,
        )),
    }
}

pub(crate) fn parse_status(s: &str) -> Option<Status> {
    match s {
        "proposed" => Some(Status::Proposed),
        "implemented" => Some(Status::Implemented),
        "verified" => Some(Status::Verified),
        "vagrant" => Some(Status::Vagrant),
        _ => None,
    }
}

pub(crate) fn parse_shape(s: &str) -> Option<C4Shape> {
    match s {
        "rectangle" => Some(C4Shape::Rectangle),
        "person" => Some(C4Shape::Person),
        "cylinder" => Some(C4Shape::Cylinder),
        "pipe" => Some(C4Shape::Pipe),
        "trapezoid" => Some(C4Shape::Trapezoid),
        "bucket" => Some(C4Shape::Bucket),
        "hexagon" => Some(C4Shape::Hexagon),
        _ => None,
    }
}

pub(crate) fn kind_str(k: &C4Kind) -> &'static str {
    match k {
        C4Kind::Person => "person",
        C4Kind::System => "system",
        C4Kind::Container => "container",
        C4Kind::Component => "component",
        C4Kind::Operation => "operation",
        C4Kind::Process => "process",
        C4Kind::Model => "model",
    }
}

pub(crate) fn status_str(s: &Option<Status>) -> &'static str {
    match s {
        Some(Status::Proposed) => "proposed",
        Some(Status::Implemented) => "implemented",
        Some(Status::Verified) => "verified",
        Some(Status::Vagrant) => "vagrant",
        None => "none",
    }
}

pub(crate) fn shape_str(s: &Option<C4Shape>) -> &'static str {
    match s {
        Some(C4Shape::Rectangle) => "rectangle",
        Some(C4Shape::Person) => "person",
        Some(C4Shape::Cylinder) => "cylinder",
        Some(C4Shape::Pipe) => "pipe",
        Some(C4Shape::Trapezoid) => "trapezoid",
        Some(C4Shape::Bucket) => "bucket",
        Some(C4Shape::Hexagon) => "hexagon",
        None => "default",
    }
}

pub(crate) fn opt_str(s: &Option<String>) -> &str {
    s.as_deref().unwrap_or("none")
}

pub(crate) fn format_contract_and_notes(
    name: &str,
    contract: &scryer_core::Contract,
    notes: &[String],
) -> String {
    let mut out = String::new();
    if !contract.is_empty() {
        out.push_str(&format!("\n{} — Contract (MUST follow):\n", name));
        if !contract.expect.is_empty() {
            out.push_str("  MUST:\n");
            for item in &contract.expect {
                out.push_str(&format!("    - {}\n", item));
            }
        }
        if !contract.ask.is_empty() {
            out.push_str("  ASK USER FIRST:\n");
            for item in &contract.ask {
                out.push_str(&format!("    - {}\n", item));
            }
        }
        if !contract.never.is_empty() {
            out.push_str("  NEVER:\n");
            for item in &contract.never {
                out.push_str(&format!("    - {}\n", item));
            }
        }
    }
    if !notes.is_empty() {
        out.push_str(&format!("\n{} — Notes:\n", name));
        for d in notes {
            out.push_str(&format!("  - {}\n", d));
        }
    }
    out
}

pub(crate) fn find_next_name<'a>(
    blocked: &[&'a scryer_core::C4Node],
    ready: &[&'a scryer_core::C4Node],
    current_unit: &[&scryer_core::C4Node],
) -> Option<&'a str> {
    // First check remaining ready nodes (not in current unit)
    let current_ids: std::collections::HashSet<&str> =
        current_unit.iter().map(|n| n.id.as_str()).collect();
    for n in ready {
        if !current_ids.contains(n.id.as_str()) {
            return Some(&n.data.name);
        }
    }
    // Then first blocked node
    blocked.first().map(|n| n.data.name.as_str())
}

pub(crate) fn format_done_message(model: &C4ModelData) -> String {
    let mut output = String::from("All tasks complete.");

    // Check for member nodes (operations/processes/models) that are still proposed
    let mut pending_members: Vec<(&C4Node, &str)> = Vec::new();
    for node in &model.nodes {
        if node.data.kind != C4Kind::Component {
            continue;
        }
        for member in model.nodes.iter().filter(|n| {
            n.parent_id.as_deref() == Some(&node.id)
                && matches!(n.data.kind, C4Kind::Operation | C4Kind::Process | C4Kind::Model)
                && matches!(n.data.status, Some(Status::Proposed))
        }) {
            pending_members.push((member, &node.data.name));
        }
    }
    if !pending_members.is_empty() {
        output.push_str("\n\nThese member nodes are still proposed — mark as `implemented` with a reason explaining what was built:\n");
        for (member, parent_name) in &pending_members {
            output.push_str(&format!(
                "  - {} [{}] ({}, {}) in {}\n",
                member.data.name,
                member.id,
                kind_str(&member.data.kind),
                status_str(&member.data.status),
                parent_name
            ));
        }
    }

    if model.flows.is_empty() {
        if pending_members.is_empty() {
            output.push_str(" Nothing to build.");
        }
        return output;
    }

    output.push_str("\n\nPlease validate that the model's flows still accurately describe the system behavior. Flows are integration test specs — update step descriptions as needed using `set_flows`. Use @[Name] mentions in step descriptions to reference architecture nodes. When a test exists for a flow, use `update_source_map` to link the flow to the test file.\n");

    for flow in &model.flows {
        let all_steps = collect_all_steps(&flow.steps);
        output.push_str(&format!("\n**{}** — {} steps\n", flow.name, all_steps.len()));
    }

    output
}

pub(crate) fn compute_diff(baseline: &C4ModelData, current: &C4ModelData) -> String {
    let base_nodes: HashMap<&str, &C4Node> =
        baseline.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let curr_nodes: HashMap<&str, &C4Node> =
        current.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let base_edges: HashMap<&str, &C4Edge> =
        baseline.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let curr_edges: HashMap<&str, &C4Edge> =
        current.edges.iter().map(|e| (e.id.as_str(), e)).collect();

    let mut sections: Vec<String> = Vec::new();

    // --- Nodes added ---
    let added: Vec<_> = current
        .nodes
        .iter()
        .filter(|n| !base_nodes.contains_key(n.id.as_str()))
        .collect();
    if !added.is_empty() {
        let mut lines = vec![format!("Nodes added ({}):", added.len())];
        for n in &added {
            let mut detail = format!(
                "  - {} \"{}\" ({})",
                n.id,
                n.data.name,
                kind_str(&n.data.kind)
            );
            if let Some(pid) = &n.parent_id {
                detail.push_str(&format!(", parent={}", pid));
            }
            if let Some(tech) = &n.data.technology {
                detail.push_str(&format!(", technology={}", tech));
            }
            if n.data.shape.is_some() {
                detail.push_str(&format!(", shape={}", shape_str(&n.data.shape)));
            }
            if n.data.status.is_some() {
                detail.push_str(&format!(", status={}", status_str(&n.data.status)));
            }
            if !n.data.description.is_empty() {
                let desc = if n.data.description.len() > 80 {
                    format!("{}...", &n.data.description[..77])
                } else {
                    n.data.description.clone()
                };
                detail.push_str(&format!(", description=\"{}\"", desc));
            }
            lines.push(detail);
        }
        sections.push(lines.join("\n"));
    }

    // --- Nodes removed ---
    let removed: Vec<_> = baseline
        .nodes
        .iter()
        .filter(|n| !curr_nodes.contains_key(n.id.as_str()))
        .collect();
    if !removed.is_empty() {
        let mut lines = vec![format!("Nodes removed ({}):", removed.len())];
        for n in &removed {
            lines.push(format!(
                "  - {} \"{}\" ({})",
                n.id,
                n.data.name,
                kind_str(&n.data.kind)
            ));
        }
        sections.push(lines.join("\n"));
    }

    // --- Nodes modified ---
    let mut mod_lines: Vec<String> = Vec::new();
    for (id, curr) in &curr_nodes {
        if let Some(base) = base_nodes.get(id) {
            let mut changes: Vec<String> = Vec::new();
            if base.data.name != curr.data.name {
                changes.push(format!(
                    "name \"{}\" -> \"{}\"",
                    base.data.name, curr.data.name
                ));
            }
            if base.data.description != curr.data.description {
                changes.push("description changed".to_string());
            }
            if base.data.kind != curr.data.kind {
                changes.push(format!(
                    "kind {} -> {}",
                    kind_str(&base.data.kind),
                    kind_str(&curr.data.kind)
                ));
            }
            if base.data.technology != curr.data.technology {
                changes.push(format!(
                    "technology {} -> {}",
                    opt_str(&base.data.technology),
                    opt_str(&curr.data.technology)
                ));
            }
            if base.data.external != curr.data.external {
                changes.push(format!(
                    "external {:?} -> {:?}",
                    base.data.external, curr.data.external
                ));
            }
            if base.data.shape != curr.data.shape {
                changes.push(format!(
                    "shape {} -> {}",
                    shape_str(&base.data.shape),
                    shape_str(&curr.data.shape)
                ));
            }
            if base.data.status != curr.data.status {
                changes.push(format!(
                    "status {} -> {}",
                    status_str(&base.data.status),
                    status_str(&curr.data.status)
                ));
            }
            if base.parent_id != curr.parent_id {
                changes.push(format!(
                    "parentId {} -> {}",
                    base.parent_id.as_deref().unwrap_or("none"),
                    curr.parent_id.as_deref().unwrap_or("none")
                ));
            }
            // Sources: compare by serialization
            if base.data.sources.len() != curr.data.sources.len()
                || base
                    .data
                    .sources
                    .iter()
                    .zip(curr.data.sources.iter())
                    .any(|(a, b)| a.pattern != b.pattern || a.comment != b.comment)
            {
                changes.push(format!(
                    "sources changed ({} -> {} entries)",
                    base.data.sources.len(),
                    curr.data.sources.len()
                ));
            }
            if base.data.contract != curr.data.contract {
                changes.push("contract changed".to_string());
            }
            if base.data.notes != curr.data.notes {
                changes.push("notes changed".to_string());
            }
            if base.data.properties != curr.data.properties {
                changes.push(format!(
                    "properties changed ({} -> {} entries)",
                    base.data.properties.len(),
                    curr.data.properties.len()
                ));
            }
            if !changes.is_empty() {
                mod_lines.push(format!(
                    "  - {} (\"{}\"): {}",
                    id,
                    curr.data.name,
                    changes.join(", ")
                ));
            }
        }
    }
    if !mod_lines.is_empty() {
        sections.push(format!(
            "Nodes modified ({}):\n{}",
            mod_lines.len(),
            mod_lines.join("\n")
        ));
    }

    // --- Edges added ---
    let edges_added: Vec<_> = current
        .edges
        .iter()
        .filter(|e| !base_edges.contains_key(e.id.as_str()))
        .collect();
    if !edges_added.is_empty() {
        let mut lines = vec![format!("Edges added ({}):", edges_added.len())];
        for e in &edges_added {
            let label = e.data.as_ref().map(|d| d.label.as_str()).unwrap_or("");
            lines.push(format!(
                "  - {}: {} -> {} \"{}\"",
                e.id, e.source, e.target, label
            ));
        }
        sections.push(lines.join("\n"));
    }

    // --- Edges removed ---
    let edges_removed: Vec<_> = baseline
        .edges
        .iter()
        .filter(|e| !curr_edges.contains_key(e.id.as_str()))
        .collect();
    if !edges_removed.is_empty() {
        let mut lines = vec![format!("Edges removed ({}):", edges_removed.len())];
        for e in &edges_removed {
            let label = e.data.as_ref().map(|d| d.label.as_str()).unwrap_or("");
            lines.push(format!(
                "  - {}: {} -> {} \"{}\"",
                e.id, e.source, e.target, label
            ));
        }
        sections.push(lines.join("\n"));
    }

    // --- Edges modified ---
    let mut edge_mod_lines: Vec<String> = Vec::new();
    for (id, curr) in &curr_edges {
        if let Some(base) = base_edges.get(id) {
            let mut changes: Vec<String> = Vec::new();
            let base_data = base.data.as_ref();
            let curr_data = curr.data.as_ref();
            let base_label = base_data.map(|d| d.label.as_str()).unwrap_or("");
            let curr_label = curr_data.map(|d| d.label.as_str()).unwrap_or("");
            if base_label != curr_label {
                changes.push(format!("label \"{}\" -> \"{}\"", base_label, curr_label));
            }
            let base_method = base_data.and_then(|d| d.method.as_deref());
            let curr_method = curr_data.and_then(|d| d.method.as_deref());
            if base_method != curr_method {
                changes.push(format!(
                    "method {} -> {}",
                    base_method.unwrap_or("none"),
                    curr_method.unwrap_or("none")
                ));
            }
            if !changes.is_empty() {
                edge_mod_lines.push(format!("  - {}: {}", id, changes.join(", ")));
            }
        }
    }
    if !edge_mod_lines.is_empty() {
        sections.push(format!(
            "Edges modified ({}):\n{}",
            edge_mod_lines.len(),
            edge_mod_lines.join("\n")
        ));
    }


    // --- Flows ---
    let base_flows: HashMap<&str, &Flow> =
        baseline.flows.iter().map(|s| (s.id.as_str(), s)).collect();
    let curr_flows: HashMap<&str, &Flow> =
        current.flows.iter().map(|s| (s.id.as_str(), s)).collect();

    let flows_added: Vec<_> = current
        .flows
        .iter()
        .filter(|s| !base_flows.contains_key(s.id.as_str()))
        .collect();
    if !flows_added.is_empty() {
        let mut lines = vec![format!("Flows added ({}):", flows_added.len())];
        for s in &flows_added {
            lines.push(format!(
                "  - {} \"{}\" ({} steps)",
                s.id,
                s.name,
                scryer_core::collect_step_ids(&s.steps).len(),
            ));
        }
        sections.push(lines.join("\n"));
    }

    let flows_removed: Vec<_> = baseline
        .flows
        .iter()
        .filter(|s| !curr_flows.contains_key(s.id.as_str()))
        .collect();
    if !flows_removed.is_empty() {
        let mut lines = vec![format!("Flows removed ({}):", flows_removed.len())];
        for s in &flows_removed {
            lines.push(format!("  - {} \"{}\"", s.id, s.name));
        }
        sections.push(lines.join("\n"));
    }

    let mut flow_mod_lines: Vec<String> = Vec::new();
    for (id, curr) in &curr_flows {
        if let Some(base) = base_flows.get(id) {
            if base != curr {
                let mut changes: Vec<String> = Vec::new();
                if base.name != curr.name {
                    changes.push(format!("name \"{}\" -> \"{}\"", base.name, curr.name));
                }
                let base_count = scryer_core::collect_step_ids(&base.steps).len();
                let curr_count = scryer_core::collect_step_ids(&curr.steps).len();
                if base_count != curr_count {
                    changes.push(format!(
                        "steps {} -> {}",
                        base_count,
                        curr_count
                    ));
                }
                if base.description != curr.description {
                    changes.push("description changed".to_string());
                }
                if !changes.is_empty() {
                    flow_mod_lines.push(format!(
                        "  - {} (\"{}\"): {}",
                        id,
                        curr.name,
                        changes.join(", ")
                    ));
                }
            }
        }
    }
    if !flow_mod_lines.is_empty() {
        sections.push(format!(
            "Flows modified ({}):\n{}",
            flow_mod_lines.len(),
            flow_mod_lines.join("\n")
        ));
    }

    if sections.is_empty() {
        "No changes since last seen.".to_string()
    } else {
        sections.join("\n\n")
    }
}
