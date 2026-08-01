# The knowledge base

Every stage of the loop produces artifacts the other stages consume — and
all of them live in one place: a knowledge base of typed, schema-validated,
machine-readable pages. The assessment's findings, the decisions and their
plans, the guides, the monthly measurements — one substrate, so the thread
from *where are we?* to *is it working?* never gets dropped between tools.

## llm-wiki

The substrate is [llm-wiki](https://github.com/como-technologies/llm-wiki),
Como's knowledge base product: one headless, git-backed binary with typed
pages, strict validation at admission, full-text search, a concept graph,
and a rich tool surface for AI harnesses — and **no LLM inside**. Every
model call happens in the tools and harness around it, behind a
deterministic gate, which is what keeps the AI-assisted steps auditable: a
page either validates and lands, or it doesn't.

## Your harness is the interface

You work with the knowledge base from your own AI harness — Claude Code,
Claude Desktop, or any MCP-capable client — configured from the shipped
**authoring kit**: instructions, skills, reference configs, and
[starter content](./starter-content.md). Ask questions over the corpus,
draft a decision in conversation, file guidance as you learn it — every
write goes through the same validation gates regardless of who (or what)
authored it. The CLIs remain first-class doors for people and pipelines
that prefer them.

The full specification — page types, validation rules, the admission
model — lives with the product, in the
[llm-wiki repo](https://github.com/como-technologies/llm-wiki/blob/main/docs/specifications/como-kb-spec.md).
