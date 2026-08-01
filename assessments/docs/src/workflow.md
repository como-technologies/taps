# Authoring Workflow

Authoring happens in the project workspace: chat on the left, live preview on
the right. The conversation is driven by a Claude **tool-use loop** — the
model decides when to move the project between phases and when to generate
content, by calling server-side tools (`src/services/tools.rs`).

## The five phases

Projects move through five phases (`src/models/project.rs`):

| # | Phase         | In the UI                  | What happens                                            |
| - | ------------- | -------------------------- | ------------------------------------------------------- |
| 1 | `Scoping`     | "What are we assessing?"   | Define the domain, scope, and goals                     |
| 2 | `Structuring` | "Building the structure"   | Generate assessment structure (domains and practices)   |
| 3 | `Questions`   | "Adding questions"         | Add questions to each practice                          |
| 4 | `Refining`    | "Making it better"         | Collaborate with the AI to improve the assessment       |
| 5 | `Complete`    | "Ready to go"              | The assessment is ready to export                       |

## Tools available to the model

Which tools Claude sees depends on the current phase
(`get_tools_for_phase`):

- **`advance_phase`** — available in every phase except `Complete`; moves to
  the next phase when the current phase's objectives are met.
- **`go_back_phase`** — available in every phase except `Scoping`; returns to
  an earlier phase to revisit work.
- **`ask_clarifying_question`** — `Scoping` only; presents the user a
  structured question with predefined options (optionally multi-select or
  free-text).
- **`generate_structure`** — `Structuring` only; generates the domains and
  practices (without questions) from the gathered context.
- **`generate_questions`** — `Questions` only; generates questions for one
  practice at a time, identified by its UUID.

`Refining` and `Complete` expose only the phase-navigation tools — refinement
happens through ordinary conversation against the live preview.

## Documents

The workspace accepts document uploads (`POST /api/projects/{id}/upload`).
The raw file is stored under the project's `uploads/` directory, and for
**text formats** (content type `text/*` or `application/json`, or a
`.md`/`.markdown`/`.txt`/`.json`/`.yaml`/`.yml` extension when the browser
sends a generic content type) the content is extracted at upload time —
UTF-8 only, capped at 64 KiB — into the document metadata
(`documents.json`).

Extracted text reaches **every chat and generation prompt** as background
context, framed exactly like the headless pipeline's `--context` files
(see [Headless Authoring](./authoring.md)): a `## Background Signal`
section with an explicit never-cite instruction. The same mechanical
quality gates apply to web-generated output (ADR-0007): a structure or
question set that cites an uploaded document's filename or its data-shape
JSON keys — or that echoes placeholders — is **never saved**; the failure
is returned to the model as the tool result, naming the offending tokens,
so it can regenerate (the web flow's corrective feedback). Deleting a
document removes its text from the prompt context.

Binary formats (PDF/DOCX) are stored but not extracted at this rung —
paste or convert the relevant text instead (extraction libraries are a
self-serve-scale concern).

## Storage layout

Each project is a directory under `DATA_DIR/projects/<uuid>/`:

```text
projects/<uuid>/
├── project.json      # project metadata, phase, model choice, document ids
├── documents.json    # uploaded-document metadata incl. extracted text
├── chat.json         # the conversation
├── assessment.yaml   # the assessment being built
└── uploads/          # uploaded files (raw bytes)
```
