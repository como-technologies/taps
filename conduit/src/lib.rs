//! conduit — harness-first execution store for the Adopt stage (portfolio
//! ADR-0015; the rebuild is taps issue 113).
//!
//! Work items (project/story/task) are conduit-owned KB classes behind the
//! llm-wiki appliance; humans gate intent (sign-off seals, [`approval`]),
//! the lifecycle rules are pure ([`workitem`]), and the doors ([`surface`],
//! served at the terminal and over [`mcp`]) are the only way work-item
//! state changes. Internal git ([`repo`]) provides the branches work lands
//! on through the mechanical merge door. `main.rs` is clap marshalling
//! only.

pub mod approval;
pub mod cli;
pub mod hash;
pub mod item;
pub mod mcp;
pub mod proc;
pub mod repo;
pub mod surface;
pub mod work;
pub mod workitem;
