//! Chat models for conversation history.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ids::{
    ChatMessageId, ClarifyingQuestionId, DocumentId, PracticeId, ProjectId, ToolUseId,
};

/// A conversation associated with a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub project_id: ProjectId,
    pub messages: Vec<ChatMessage>,
}

impl Conversation {
    /// Create a new empty conversation.
    pub fn new(project_id: ProjectId) -> Self {
        Self {
            project_id,
            messages: Vec::new(),
        }
    }

    /// Add a message to the conversation.
    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: ChatMessageId,
    pub role: ChatRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,

    /// If this message triggered an assessment update
    #[serde(default)]
    pub triggered_update: bool,

    /// Document attachments (by ID)
    #[serde(default)]
    pub attachments: Vec<DocumentId>,

    /// Tool uses in this message (for assistant messages).
    #[serde(default)]
    pub tool_uses: Vec<ToolUse>,

    /// A clarifying question (for assistant messages).
    #[serde(default)]
    pub clarifying_question: Option<ClarifyingQuestion>,

    /// Suggested quick-reply labels (rendered as pill buttons; latest message only).
    #[serde(default)]
    pub suggestions: Vec<String>,

    /// Marker that questions were generated for a practice in this turn.
    /// The chat renders a compact banner instead of relisting questions
    /// already visible in the assessment preview pane.
    #[serde(default)]
    pub questions_generated: Option<GeneratedQuestionsInfo>,
}

/// Summary of a `generate_questions` tool invocation — attached to the
/// corresponding assistant `ChatMessage` so the UI can render a compact
/// banner instead of the model echoing the full question list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedQuestionsInfo {
    pub practice_id: PracticeId,
    pub practice_name: String,
    pub count: usize,
}

/// A tool use from Claude's response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUse {
    pub id: ToolUseId,
    pub name: String,
    pub input: Value,
}

impl ChatMessage {
    /// Create a user message.
    pub fn user(content: String) -> Self {
        Self {
            id: ChatMessageId::new(),
            role: ChatRole::User,
            content,
            timestamp: Utc::now(),
            triggered_update: false,
            attachments: Vec::new(),
            tool_uses: Vec::new(),
            clarifying_question: None,
            suggestions: Vec::new(),
            questions_generated: None,
        }
    }

    /// Create an assistant message.
    pub fn assistant(content: String) -> Self {
        Self {
            id: ChatMessageId::new(),
            role: ChatRole::Assistant,
            content,
            timestamp: Utc::now(),
            triggered_update: false,
            attachments: Vec::new(),
            tool_uses: Vec::new(),
            clarifying_question: None,
            suggestions: Vec::new(),
            questions_generated: None,
        }
    }

    /// Attach quick-reply suggestions to an assistant message.
    pub fn with_suggestions(mut self, suggestions: Vec<String>) -> Self {
        self.suggestions = suggestions;
        self
    }

    /// Attach a "questions generated" marker to an assistant message.
    pub fn with_questions_generated(mut self, info: GeneratedQuestionsInfo) -> Self {
        self.questions_generated = Some(info);
        self
    }

    /// Create an assistant message with a clarifying question.
    pub fn with_question(content: String, question: ClarifyingQuestion) -> Self {
        Self {
            id: ChatMessageId::new(),
            role: ChatRole::Assistant,
            content,
            timestamp: Utc::now(),
            triggered_update: false,
            attachments: Vec::new(),
            tool_uses: Vec::new(),
            clarifying_question: Some(question),
            suggestions: Vec::new(),
            questions_generated: None,
        }
    }

    /// Render this message's content as HTML (markdown → HTML for templates).
    pub fn content_html(&self) -> String {
        use pulldown_cmark::{Options, Parser, html};
        let parser = Parser::new_ext(&self.content, Options::all());
        let mut out = String::new();
        html::push_html(&mut out, parser);
        out
    }
}

/// Role in a chat conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

/// A structured clarifying question with options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarifyingQuestion {
    pub id: ClarifyingQuestionId,
    pub question: String,
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub allow_custom: bool,
    #[serde(default)]
    pub multi_select: bool,
}

/// An option for a clarifying question.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}

impl std::fmt::Display for ChatRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatRole::User => write!(f, "User"),
            ChatRole::Assistant => write!(f, "Assistant"),
            ChatRole::System => write!(f, "System"),
        }
    }
}
