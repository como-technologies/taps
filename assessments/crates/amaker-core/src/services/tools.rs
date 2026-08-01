//! Tool definition for the LLM provider seam.
//!
//! The loop line carried a full interactive tool-calling agent here; that
//! agent loop is redundant with the web apps' own (rig-based) authoring
//! engine, so only the `ToolDef` shape the `LlmProvider` trait and its
//! impls reference is kept.

use serde::Serialize;
use serde_json::Value;

/// Tool definition passed to an `LlmProvider` for tool use.
#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}
