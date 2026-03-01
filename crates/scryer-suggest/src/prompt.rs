use scryer_core::{C4Kind, C4ModelData, Flow, Status};

fn name_of<'a>(id: &'a str, model: &'a C4ModelData) -> &'a str {
    model
        .nodes
        .iter()
        .find(|n| n.id == id)
        .map(|n| n.data.name.as_str())
        .unwrap_or(id)
}

/// Convert a C4 model to a compact text representation for LLM consumption.
pub fn serialize_diagram(model: &C4ModelData) -> String {
    let mut out = String::with_capacity(2048);

    out.push_str("NODES:\n");
    for node in &model.nodes {
        let d = &node.data;
        let prefix = match d.kind {
            C4Kind::Person => "[P]",
            C4Kind::System if d.external.unwrap_or(false) => "[S!]",
            C4Kind::System => "[S]",
            C4Kind::Container => "[C]",
            C4Kind::Component => "[K]",
            C4Kind::Operation => "[M]",
            C4Kind::Process => "[Pr]",
            C4Kind::Model => "[Md]",
        };

        out.push_str(prefix);
        out.push(' ');
        out.push_str(&node.id);
        out.push_str(" \"");
        out.push_str(&d.name);
        out.push_str("\" (");
        out.push_str(kind_str(&d.kind));
        if d.external.unwrap_or(false) {
            out.push_str(",external");
        }
        if let Some(pid) = &node.parent_id {
            out.push_str(",parent=");
            out.push_str(name_of(pid, model));
        }
        out.push(')');
        if let Some(tech) = &d.technology {
            if !tech.is_empty() {
                out.push_str(" tech=");
                out.push_str(tech);
            }
        }
        if let Some(ref status) = d.status {
            out.push_str(" status=");
            out.push_str(match status {
                Status::Implemented => "implemented",
                Status::Proposed => "proposed",
                Status::Changed => "changed",
                Status::Deprecated => "deprecated",
            });
        }
        if !d.description.is_empty() {
            out.push_str(" | \"");
            // Truncate long descriptions
            if d.description.len() > 80 {
                out.push_str(&d.description[..80]);
                out.push_str("...");
            } else {
                out.push_str(&d.description);
            }
            out.push('"');
        }
        out.push('\n');
    }

    out.push_str("EDGES:\n");
    for edge in &model.edges {
        let label = edge
            .data
            .as_ref()
            .map(|d| d.label.as_str())
            .unwrap_or("uses");
        let tech = edge.data.as_ref().and_then(|d| d.method.as_deref());

        out.push_str(&edge.source);
        out.push_str(" \"");
        out.push_str(name_of(&edge.source, model));
        out.push_str("\" --[");
        out.push_str(label);
        if let Some(t) = tech {
            out.push('/');
            out.push_str(t);
        }
        out.push_str("]--> ");
        out.push_str(&edge.target);
        out.push_str(" \"");
        out.push_str(name_of(&edge.target, model));
        out.push('"');
        out.push('\n');
    }

    if !model.flows.is_empty() {
        out.push_str("FLOWS:\n");
        for flow in &model.flows {
            serialize_flow(&mut out, flow);
        }
    }

    out
}

fn serialize_flow(out: &mut String, flow: &Flow) {
    out.push_str("  flow \"");
    out.push_str(&flow.name);
    out.push_str("\":\n");
    serialize_steps(out, &flow.steps, 4);
}

fn serialize_steps(out: &mut String, steps: &[scryer_core::FlowStep], indent: usize) {
    let pad: String = " ".repeat(indent);
    for step in steps {
        out.push_str(&pad);
        out.push('[');
        out.push_str(&step.id);
        out.push_str("] ");
        out.push_str(step.description.as_deref().unwrap_or("(empty)"));
        out.push('\n');
        for branch in &step.branches {
            out.push_str(&pad);
            out.push_str("  branch");
            if !branch.condition.is_empty() {
                out.push_str(" \"");
                out.push_str(&branch.condition);
                out.push('"');
            }
            out.push_str(":\n");
            serialize_steps(out, &branch.steps, indent + 4);
        }
    }
}

fn kind_str(kind: &C4Kind) -> &'static str {
    match kind {
        C4Kind::Person => "person",
        C4Kind::System => "system",
        C4Kind::Container => "container",
        C4Kind::Component => "component",
        C4Kind::Operation => "operation",
        C4Kind::Process => "process",
        C4Kind::Model => "model",
    }
}

pub fn system_prompt() -> String {
    format!(
        "You are a C4 architecture modeling advisor. Review diagrams for architectural quality — \
naming, relationships, structural problems.\n\n\
Focus on:\n\
- Vague or technology-stuffed names — suggest clearer role-based alternatives \
(e.g. \"Data Service\" → \"Order Fulfillment Service\", \"React + Express\" → \"Web App\")\n\
- Missing or misleading relationships — flag nodes that likely need connections, \
or edges pointing in the wrong direction\n\
- Structural issues — frontend talking directly to a database, missing queues/buses \
between async services, components that are too abstract to map to code\n\
- Authority hierarchy violations — a component whose responsibility doesn't fit within its \
parent container's stated role, cross-cutting concerns that suggest a container boundary \
needs rethinking, or lower-level structure that implicitly redefines higher-level decisions\n\
- Suggest specific, better names when you can — don't just say \"rename this\"\n\
- Flow step granularity — flag steps that describe UI gestures instead of system \
interactions (e.g. \"clicks button\", \"scrolls down\", \"fills in field\", \"hovers over\", \
\"types in\", \"selects option\"). Each step should be a meaningful interaction like \
\"System validates credentials\" or \"API returns search results\"\n\
- Missing production infrastructure — flag systems that accept user input but have no \
auth mechanism modeled, APIs with no validation or error handling component, databases \
with no migration strategy, user-facing services with no rate limiting. Be specific: \
\"This API has no authentication — add a Session Auth or JWT component\" not \"consider security\"\n\
- Placeholder nodes — flag nodes named like \"Auth (TODO)\", \"TBD\", or with vague descriptions \
like \"handles security\" that don't name a concrete mechanism\n\n\
Do NOT:\n\
- Flag empty descriptions, missing technology fields, or unlabeled edges — \
the UI already tracks completeness separately\n\
- Suggest reorganizing the author's decomposition — container boundaries are intentional\n\
- Make assumptions about how the underlying code is structured\n\
- Give generic architecture advice (\"consider scaling\", \"think about caching\") — \
only flag concrete missing pieces that should be explicit nodes in the model\n\
- Suggest adding edges that the C4 rules say are wrong\n\n\
Output ONLY a JSON array. \
Each item: {{\"node\":\"<node-id or step-id>\",\"msg\":\"<suggestion>\",\"sev\":\"i\"|\"w\"}}. \
Use the node ID for architecture hints, step ID for flow hints. \
In \"msg\", use display names so the text is human-readable. \
Use \"w\" only for clear C4 violations. Use \"i\" for constructive suggestions. \
If nothing to suggest, output [].\n\n\
## C4 Rules\n{}\n\n\
Output ONLY the JSON array, nothing else.",
        scryer_core::rules::RULES
    )
}

pub fn user_message(model: &C4ModelData) -> String {
    serialize_diagram(model)
}
