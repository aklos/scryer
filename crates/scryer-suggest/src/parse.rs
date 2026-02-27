use scryer_core::C4ModelData;

use crate::{Hint, HintSeverity};


#[derive(serde::Deserialize)]
struct LlmHint {
    node: String,
    msg: String,
    sev: Option<String>,
}

/// Parse raw LLM output into Hint structs, matching node names back to IDs.
/// Returns empty vec on total parse failure (graceful degradation).
pub fn parse_llm_output(raw: &str, model: &C4ModelData) -> Vec<Hint> {
    let json_str = match extract_json_array(raw) {
        Some(s) => s,
        None => return vec![],
    };

    // Try full array parse first
    let llm_hints: Vec<LlmHint> = match serde_json::from_str(&json_str) {
        Ok(h) => h,
        Err(_) => {
            // Fall back to line-by-line extraction
            parse_line_by_line(&json_str)
        }
    };

    llm_hints
        .into_iter()
        .filter_map(|lh| {
            let node_id = resolve_node_id(&lh.node, model)?;
            Some(Hint {
                node_id,
                message: lh.msg,
                severity: map_severity(lh.sev.as_deref()),
            })
        })
        .collect()
}

/// Extract the JSON array substring from raw LLM output.
fn extract_json_array(raw: &str) -> Option<String> {
    let start = raw.find('[')?;
    let end = raw.rfind(']')?;
    if end <= start {
        return None;
    }
    Some(raw[start..=end].to_string())
}

/// Try to parse individual objects from a malformed JSON array.
fn parse_line_by_line(json_str: &str) -> Vec<LlmHint> {
    let inner = json_str
        .trim()
        .strip_prefix('[')
        .unwrap_or(json_str)
        .strip_suffix(']')
        .unwrap_or(json_str);

    let mut hints = Vec::new();
    let mut depth = 0;
    let mut start = None;

    for (i, ch) in inner.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        let obj_str = &inner[s..=i];
                        if let Ok(hint) = serde_json::from_str::<LlmHint>(obj_str) {
                            hints.push(hint);
                        }
                    }
                    start = None;
                }
            }
            _ => {}
        }
    }

    hints
}

/// Match a node or step identifier from LLM output to an ID in the model.
/// Tries ID match first (e.g. "node-3", "step-1"), then falls back to name matching.
fn resolve_node_id(name: &str, model: &C4ModelData) -> Option<String> {
    // Direct node ID match
    if model.nodes.iter().any(|n| n.id == name) {
        return Some(name.to_string());
    }

    // Direct step ID match
    for flow in &model.flows {
        if flow.steps.iter().any(|s| s.id == name) {
            return Some(name.to_string());
        }
    }

    let name_lower = name.to_lowercase();

    // Exact name match
    if let Some(n) = model.nodes.iter().find(|n| n.data.name == name) {
        return Some(n.id.clone());
    }

    // Case-insensitive name match
    if let Some(n) = model
        .nodes
        .iter()
        .find(|n| n.data.name.to_lowercase() == name_lower)
    {
        return Some(n.id.clone());
    }

    // Substring match (name contains the LLM's string or vice versa)
    if let Some(n) = model.nodes.iter().find(|n| {
        let n_lower = n.data.name.to_lowercase();
        n_lower.contains(&name_lower) || name_lower.contains(&n_lower)
    }) {
        return Some(n.id.clone());
    }

    None
}

fn map_severity(s: Option<&str>) -> HintSeverity {
    match s {
        Some("w") => HintSeverity::Warning,
        _ => HintSeverity::Info,
    }
}
