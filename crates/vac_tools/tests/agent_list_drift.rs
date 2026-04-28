//! Arc-audit C3 — drift guard for `AgentListTool`.
//!
//! The tool hardcodes the 5 built-in subagent kinds instead of
//! delegating to `vac_session_engine::BUILT_IN_SUBAGENTS` (the
//! wrapper lives in `vac_tools` which only depends on
//! `vac_session_primitives` in production; pulling session_engine
//! as a prod dep would re-introduce the cycle the arc broke).
//!
//! This test runs the tool and asserts that the kind set it
//! advertises to the LLM is a bijection with the canonical
//! `BUILT_IN_SUBAGENTS` constant. Adding a kind without touching
//! the tool now fails CI here.

use vac_session_engine::BUILT_IN_SUBAGENTS;
use vac_tools::builtin::agent_list::AgentListTool;
use vac_tools::registry::{ToolContext, VilTool};

#[tokio::test]
async fn agent_list_tool_matches_session_engine_built_ins() {
    let tool = AgentListTool::new();
    let ctx = ToolContext::new(std::env::temp_dir());
    let out = tool.execute(serde_json::json!({}), &ctx).await.unwrap();

    let emitted: Vec<String> = out["agents"]
        .as_array()
        .expect("agents is an array")
        .iter()
        .map(|a| {
            a["kind"]
                .as_str()
                .expect("each agent has a kind")
                .to_string()
        })
        .collect();

    let canonical: Vec<String> = BUILT_IN_SUBAGENTS
        .iter()
        .map(|spec| spec.kind.label().to_string())
        .collect();

    // Bijection — exact match under sort.
    let mut emitted_sorted = emitted.clone();
    let mut canonical_sorted = canonical.clone();
    emitted_sorted.sort();
    canonical_sorted.sort();
    assert_eq!(
        emitted_sorted, canonical_sorted,
        "AgentListTool drift: emitted {:?} vs canonical {:?}. \
         Update crates/vac_tools/src/builtin/agent_list.rs to track \
         the new kind.",
        emitted, canonical,
    );
}
