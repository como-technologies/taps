//! Binary-side services: the rig agent loop and the tool definitions
//! it exposes. The storage / YAML / analysis primitives live in
//! `amaker_core::services`.

mod ai;
pub mod narrative;
mod tools;

pub use ai::{AiService, assessment_structure_summary, linkify_entities};
pub use tools::{
    AddDomainTool, AddPracticeTool, AddQuestionTool, AskClarifyingQuestionTool, DeleteDomainTool,
    DeletePracticeTool, DeleteQuestionTool, EditDomainTool, EditPracticeTool, EditQuestionTool,
    GenerateQuestionsTool, GenerateStructureTool, MovePracticeTool, OfferSuggestedRepliesTool,
    PublishAssessmentTool, RegenerateQuestionTool, ReorderDomainsTool, ReorderPracticesTool,
    RequestContext, ResetDraftFromVersionTool, SetQuestionBudgetTool, SharedContext,
    SwitchFocusTool, TailorVocabularyTool,
};
