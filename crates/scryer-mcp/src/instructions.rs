//! States the model-first workflow loop and steers every rule lookup to the fetched rule text by slug rather than inlining it

/// Connect-time instructions: always loaded, so one paragraph — the claim
/// grammar and the test levels an agent needs before it writes a stub, and the
/// loop. Tool descriptions carry the rest where it is used. The README's
/// suggested CLAUDE.md text is this paragraph.
pub(crate) const INSTRUCTIONS: &str = "\
Scryer reads this project's architecture from its code: containers from packaging, components from \
modules, claims from behaviour tests. A behaviour test's description is an EARS claim about the code \
it tests, subject dropped, condition first: `<verb> <object>` when always true; `When <trigger>, \
<response>`; `While <state>, <response>`; `If <unwanted condition>, then <response>`. One pattern \
and one response per test; a cross-cutting concern is a trailing `[tag]`. The test's level is the \
claim's level: an e2e test claims for the whole system, an integration test at a deployable's \
boundary for its container, a unit test for its component — write a claim at the level it speaks \
for. Before building, `orient {task, files}` and write each claim you will satisfy as a todo test \
in the file orient names; a todo test is work to do. Run only what `get_test_radius` names, then \
`ingest_test_report`. A surviving probe (`open_probe`) is a finding: strengthen the test. Every \
tool takes an optional absolute `project`, defaulting to the working directory.";
