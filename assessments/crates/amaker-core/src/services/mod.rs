//! Core services — storage, parsing, draft mutation, and analysis.
//!
//! The web-handler / agent-loop side lives in the binary crates that
//! depend on this one.

pub mod analysis;
mod anthropic;
pub mod author;
pub mod draft;
mod export;
mod generation;
mod markdown;
mod models;
mod ollama;
pub mod provider;
pub mod quality;
mod responses;
mod storage;
mod tools;
mod yaml;

pub use author::{
    AuthorContext, MAX_GENERATION_ATTEMPTS, MAX_JOBS, Progress, author_assessment, project_context,
};
pub use export::{DataFormat, ExportService};
pub use generation::{assessment_structure_summary, system_prompt_for_phase};
pub use markdown::markdown_to_html;
pub use models::{available_models, default_model_for, effective_model};
pub use provider::{DEFAULT_MAX_TOKENS, LlmProvider, build_provider};
pub use responses::{AnswerProgress, ResponseService};
pub use storage::StorageService;
pub use yaml::YamlService;
