//! Shared domain layer for the amaker binaries.
//!
//! Carries the metamodel (`Assessment`, `Domain`, `Practice`, `Question`),
//! project state (`Project`, `TopState`, `AuthoringSubstate`), the
//! blob-storage layer (`object_store`-backed, ETag-conditional writes),
//! YAML parsing, the draft-edit helper, and the analysis primitives
//! (scorecard / gaps / roadmap / narrative computation). Web-handler
//! chrome, the rig agent loop, and tool definitions live in the binary
//! crates that depend on this one.

pub mod config;
pub mod error;

pub mod models;
pub mod net;
pub mod observability;
pub mod services;
pub mod storage_backend;

pub use error::AppError;
pub use storage_backend::{StorageBackend, build_store, filesystem, load_from_env};
