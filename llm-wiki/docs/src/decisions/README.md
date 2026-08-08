# Decisions

Architectural decision records for llm-wiki as Como maintains it.

The upstream project's release-by-release decision history was retired
with the upstream break; the original repository remains the archive
for it. Suite-level decisions live in the Como portfolio ADR corpus.

| Decision | Status | Summary |
| -------- | ------ | ------- |
| [stable-page-identity](backlog/stable-page-identity.md) | shipped | Optional, tool-generated ULID `id` in frontmatter; slug-first, id-second resolution so links survive file moves. Full contract: [specifications/model/page-identity.md](../specifications/model/page-identity.md) |
