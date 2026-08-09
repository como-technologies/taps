# Summary

[Documentation](./README.md)

# Guides

- [Guides](./guides/README.md)
  - [Getting Started](./guides/getting-started.md)
  - [Writing Content](./guides/writing-content.md)
  - [Como Authoring Contract](./guides/como-authoring.md)
  - [Custom Types](./guides/custom-types.md)
  - [Configuration](./guides/configuration.md)
  - [Multi-Wiki](./guides/multi-wiki.md)
  - [Search Ranking](./guides/search-ranking.md)
  - [Graph Guide](./guides/graph.md)
  - [Lint](./guides/lint.md)
  - [LLM-Optimized Output](./guides/llms-format.md)
  - [Privacy Redaction](./guides/redaction.md)

# Specifications

- [Specifications](./specifications/README.md)
  - [The Como KB specification](./specifications/como-kb-spec.md)
  - [Type System](./specifications/model/type-system.md)
    - [Base Type](./specifications/model/types/base.md)
    - [Concept Type](./specifications/model/types/concept.md)
    - [Doc Type](./specifications/model/types/doc.md)
    - [Section Type](./specifications/model/types/section.md)
    - [Skill Type](./specifications/model/types/skill.md)
    - [Source Types](./specifications/model/types/source.md)
  - [Page Identity](./specifications/model/page-identity.md)
  - [Page Content](./specifications/model/page-content.md)
  - [Epistemic Model](./specifications/model/epistemic-model.md)
  - [Wiki Repository Layout](./specifications/model/wiki-repository-layout.md)
  - [wiki.toml](./specifications/model/wiki-toml.md)
  - [config.toml](./specifications/model/global-config.md)
  - [Engine State](./specifications/engine/engine-state.md)
    - [Index Management](./specifications/engine/index-management.md)
    - [Ingest Pipeline](./specifications/engine/ingest-pipeline.md)
    - [Graph](./specifications/engine/graph.md)
    - [Server](./specifications/engine/server.md)
    - [Watch](./specifications/engine/watch.md)
  - [Tool Surface Overview](./specifications/tools/overview.md)
    - [Search](./specifications/tools/search.md)
    - [List](./specifications/tools/list.md)
    - [Content Operations](./specifications/tools/content-operations.md)
    - [Ingest](./specifications/tools/ingest.md)
    - [Lint](./specifications/tools/lint.md)
    - [Suggest](./specifications/tools/suggest.md)
    - [Graph](./specifications/tools/graph.md)
    - [Stats](./specifications/tools/stats.md)
    - [History](./specifications/tools/history.md)
    - [Index](./specifications/tools/index.md)
    - [Export](./specifications/tools/export.md)
    - [Wiki Administration](./specifications/tools/wiki-administration.md)
    - [Schema Management](./specifications/tools/schema-management.md)
    - [Config Management](./specifications/tools/config-management.md)
  - [MCP Clients](./specifications/integrations/mcp-clients.md)
  - [ACP Transport](./specifications/integrations/acp-transport.md)

# Implementation

- [Implementation](./implementation/README.md)
  - [Rust Implementation Guide](./implementation/rust.md)
  - [Engine Implementation](./implementation/engine.md)
  - [Manager Pattern](./implementation/manager-pattern.md)
  - [MCP Tool Implementation](./implementation/mcp-tool-pattern.md)
  - [Index Manager Implementation](./implementation/index-manager.md)
  - [Tantivy Implementation Notes](./implementation/tantivy.md)
  - [Type Registry Implementation](./implementation/type-registry.md)
  - [Schema Change Detection](./implementation/schema-change-detection.md)
  - [Graph Cache Implementation](./implementation/graph-cache.md)
  - [petgraph-live Integration Guide](./implementation/petgraph-live.md)
  - [Lock Patterns](./implementation/lock-patterns.md)

# Decisions

- [Decisions](./decisions/README.md)
  - [Backlog: Stable Page Identity](./decisions/backlog/stable-page-identity.md)

---

[Diagrams](./diagrams.md)
[Roadmap](./roadmap.md)
