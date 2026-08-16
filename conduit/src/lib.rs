//! conduit — forge-neutral agentic development harness (Adopt-stage engine).
//!
//! Library crate: all logic lives here; `main.rs` is clap marshalling +
//! human rendering only. Spec: docs/src/dev/spike-design.md.

pub mod approval;
pub mod cli;
pub mod config;
pub mod contract;
pub mod engine;
pub mod forge;
pub mod git;
pub mod hash;
pub mod item;
pub mod labels;
pub mod machine;
pub mod mcp;
pub mod payload;
pub mod proc;
pub mod repo;
pub mod router;
pub mod store;
pub mod surface;
pub mod task;
pub mod transcript;
pub mod work;
pub mod workitem;
