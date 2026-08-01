//! End-to-end tests for the `llm-wiki` binary, ported from the retired
//! Python suite (`tests-integration/`). Each module spawns the real binary
//! and drives it the way a client would:
//!
//! - `engine` — CLI commands and their text/JSON output
//! - `mcp`    — the MCP server over JSON-RPC 2.0 on stdio
//! - `acp`    — the ACP server over NDJSON JSON-RPC on stdio

mod helpers;

mod acp;
mod engine;
mod mcp;
