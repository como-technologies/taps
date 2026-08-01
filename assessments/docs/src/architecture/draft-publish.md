# Storage, versioning & the draft/publish model

This page is the contract for how assessment content is stored, versioned,
and served. It's the storage and publish layer beneath the
authoring → responding → analysis arc described in
[The lifecycle](../vision/lifecycle.md).

Three ideas hold the model together:

- **The draft** is the live, freely-editable working copy. Every edit is a
  surgical tool call that preserves entity UUIDs.
- **Published versions** are immutable snapshots of the draft.
- **Responses** bind to a published version, so editing the draft can
  never break a response in progress.

## Storage backend

The storage layer is a thin facade over the
[`object_store`](https://docs.rs/object_store) crate's `ObjectStore`
trait. `object_store` (an Apache project, from the Arrow / DataFusion /
Delta-rs group) ships several backends behind feature flags:

| Backend | `object_store` type | When to pick |
|---------|---------------------|--------------|
| `Filesystem` | `LocalFileSystem` | Single-VM dev, single-binary install. The default. |
| `S3Compatible` | `AmazonS3` (S3-protocol) | rustfs (local dev), Cloudflare R2, Backblaze B2, Hetzner Object Storage, GCS-via-S3-interop |
| `Gcs` | `GoogleCloudStorage` | Native GCS API — used in the Cloud Run deployment |
| `InMemory` | `InMemory` | Tests; quick experiments |

`S3Compatible` is the S3 *wire protocol*, not AWS-the-service — it's
pointed at whichever provider speaks S3.

Backend selection is per-binary, via env var:

- `<PREFIX>_STORAGE_BACKEND` — one of `filesystem` (default),
  `s3_compatible`, `gcs`, `in_memory`. `<PREFIX>` is `AUTHOR` / `ASSESS`
  / `ANALYZE`.
- When `filesystem`: `<PREFIX>_DATA_DIR` is the root path (default `./data`).
- When `s3_compatible`: `<PREFIX>_S3_ENDPOINT`, `<PREFIX>_S3_BUCKET`,
  `<PREFIX>_S3_REGION`, `<PREFIX>_S3_ACCESS_KEY_ID`,
  `<PREFIX>_S3_SECRET_ACCESS_KEY`.
- When `gcs`: `<PREFIX>_GCS_BUCKET`, `<PREFIX>_GCS_SERVICE_ACCOUNT_PATH`
  (optional; falls back to Application Default Credentials).

All three binaries point at the same backend (same path, same bucket) so
they read each other's writes. Because the storage layer holds no local
state, each binary is a clean stateless container.

## Storage layout

Each project is a key prefix at `projects/{project_id}/`:

```
projects/{ProjectId}/
├── project.json                # mutable; the project envelope
├── draft.yaml                  # mutable; the live working copy
├── versions/
│   ├── v1.yaml                 # immutable; published snapshot of draft.yaml
│   ├── v1.meta.json            # immutable; { published_at, notes? }
│   ├── v2.yaml
│   └── v2.meta.json
├── responses/
│   └── {respondent}.yaml       # mutable; per-respondent answers + version
├── chat.json                   # mutable; conversation transcript
├── uploads/{doc_id}_{name}     # opaque uploaded blobs
└── analysis/
    ├── report.md               # cached; regenerable
    ├── scorecard.json          # cached; regenerable
    └── gaps.json               # cached; regenerable
```

`analysis/*` entries are regenerable caches, so loss is acceptable. Logs
are never in the blob store — each binary writes structured logs to
stdout (see [Logging & debugging](../operations/logging.md)).

## Concurrency

Concurrency is the storage layer's job, handled with conditional writes —
there are no in-process locks, so multiple instances of any binary are
safe to run at once.

The compare-and-swap pattern for the live draft:

1. `(yaml, version) = storage.load_assessment_yaml_versioned(project_id)`
   — the content plus a *version handle*.
2. mutate `yaml` in memory.
3. `storage.save_assessment_yaml_if_match(project_id, new_yaml, Some(version))`.
4. on `AppError::Conflict` → propagate to the caller (a tool returning to
   the agent loop), which can retry.

`draft::edit_yaml` takes a single `FnOnce` mutator and propagates a
`Conflict` rather than retrying internally — the orchestrator sees a
clear error instead of a silent overwrite.

The version handle is `object_store`'s `UpdateVersion` — it carries
*both* an ETag and a store-native version (the GCS generation, the S3
version-id). Backends condition on different fields: S3 and the local
filesystem use the ETag, GCS uses the generation. The storage layer
captures the whole handle from a read and passes it back unchanged on the
write, so the same code is correct on every backend.

Mapping to `object_store` primitives:

| Operation | `object_store` |
|-----------|----------------|
| Unconditional write | `PutMode::Overwrite` |
| Create-if-not-exists | `PutMode::Create` |
| Compare-and-swap | `PutMode::Update(UpdateVersion { e_tag, version })` |

Publishing a version is two writes — `versions/{name}.yaml` +
`versions/{name}.meta.json` — done **content-first, both with
`PutMode::Create`**:

1. Write `versions/{name}.yaml` with `Create`. A taken name fails here,
   atomically, and the publish aborts.
2. Write `versions/{name}.meta.json` with `Create`.

A crash between the two writes leaks a content blob with no meta entry.
`list_versions` skips entries lacking a `.meta.json`, so the orphan is
invisible and harmless; a future cleanup pass can reclaim it.

## The draft

`draft.yaml` is the full serialization of the
`models::assessment::Assessment` struct — every domain, practice,
question, evidence type, and blocker type, with UUIDs inline. The YAML
*is* the graph; there's no separate graph storage, and the graph is small
(tens of KB at the largest plausible assessment).

Example excerpt:

```yaml
id: 0195a1cc-0000-7000-8000-000000000001
name: Cloud Security Readiness
description: …
goal: …
evidence_types:
  - id: audit
    label: Audit report
blocker_types:
  - id: time
    label: Time
domains:
  - id: 0195a1cc-…-d001
    name: Identity & Access
    context: …
    value: …
    risk: …
    practices:
      - id: 0195a1cc-…-p001
        name: MFA
        questions:
          - id: 0195a1cc-…-q001
            text: Do all human admin accounts require MFA?
            polarity: positive
            roles: [security-engineer]
updated_at: 2026-04-24T…
```

UUIDs are generated in Rust (the `models/ids.rs` newtype macros) and
never rewritten. Every surgical edit mutates fields or adds/removes
entities; it never re-stamps an existing ID. Transcript links to UUIDs
stay live across arbitrary draft churn.

## The Assessment YAML shape (the contract)

The wire shape is the serialized `Assessment` struct — the same shape the
in-memory authoring loop uses, and the same shape stored under
`versions/`. There is no separate "snapshot" type.

- Field names match the Rust `Assessment` / `Domain` / `Practice` /
  `Question` struct fields, via `serde`.
- UUIDs are inline at every level.
- `evidence_types` and `blocker_types` are arrays of
  `{ id, label, description? }` on the root.
- Binary `polarity` is a string (`"positive"` | `"negative"`).
- Empty optional fields serialize as absent (not null), via
  `skip_serializing_if = "Option::is_none"`.

An `insta` YAML snapshot test pins the serialized shape so any change to
it is visible in review.

## Publishing a version

`publish_assessment` (a Rig tool) snapshots the current `draft.yaml` as a
new immutable version:

1. Validate the version name (no `/`, no `\`, no `..`).
2. Load `draft.yaml`. Missing draft → `BadRequest`.
3. `PutMode::Create` the content at `versions/{name}.yaml`. A duplicate
   name → `BadRequest`.
4. `PutMode::Create` the meta at `versions/{name}.meta.json` —
   `{ published_at, notes? }`.
5. Return `Published as '<name>'.`

Inputs: `name` (optional; defaults to `v{N+1}` from the current version
count) and `notes` (optional free text, stored in the meta blob). The
publish snapshots the draft exactly as it stands.

## Responses & answers

```yaml
# responses/00000000-0000-0000-0000-000000000001.yaml
version: v1
respondent_id: 00000000-0000-0000-0000-000000000001
answers:
  0195a1cc-…-q001:
    value: yes
    evidence_ids: [audit]
    blocker_ids: []
    planned: null
    notes: null
    answered_at: 2026-04-24T…
submitted_at: 2026-04-24T…
```

The `version` field is the published-version name the response is bound
to. A response is created bound to the latest published version; the
draft can change freely afterward without affecting it.

V1 has a single primary respondent at a well-known UUID; multi-respondent
lands as additive sibling files under `responses/`, schema-compatible.

## Response-loading path

To resolve a response end-to-end:

1. Read `projects/{id}/responses/{respondent}.yaml`.
2. Read `projects/{id}/versions/{response.version}.yaml` and parse it as
   `Assessment`. A missing version is a hard error.
3. Confirm every answer key in the response is a question UUID present in
   that assessment tree. A mismatch is a hard error.
4. Hand the hydrated `(Assessment, Response)` pair to the analysis and
   collection flows.

`ResponseService::load_primary_context` performs steps 1–3. Because the
response reads its *bound version*, not the live draft, transcript and
markdown UUID links resolve correctly even after the draft has moved on.

## Editing a published version

There is always exactly **one draft per project**. "Edit a published
version" means overwriting `draft.yaml` with the content of a version,
then authoring forward from there.

`reset_draft_from_version`:

1. Reads `versions/{version}.yaml`; a missing version → `NotFound`.
2. Overwrites `draft.yaml` (unconditional — last-writer-wins is fine for
   an explicit reset; a surgical edit in flight will fail its next
   conditional save with a `Conflict`).

Responses against the old version are untouched — they bind to
`versions/{name}`, not the draft. A later publish creates a new version;
all versions remain resolvable and their bound responses keep loading.

## Surgical CRUD tool surface

All structural edits are surgical. Wholesale regeneration
(`generate_structure`, full-practice `generate_questions`) is **restricted
to first-time generation**: on a non-empty draft the tool refuses and
points the orchestrator at the surgical tools.

| Entity | Tools |
|--------|-------|
| Domain | `add_domain`, `edit_domain`, `delete_domain` (with disposition), `reorder_domains` |
| Practice | `add_practice`, `edit_practice`, `delete_practice` (with disposition), `move_practice`, `reorder_practices` |
| Question | `add_question`, `edit_question`, `delete_question`, `regenerate_question` |

Each tool runs through `services::draft::edit_yaml`: load `draft.yaml`
with its version handle, parse → `Assessment`, mutate, validate
references, serialize, conditional-write. A version mismatch surfaces as
`AppError::Conflict`.

### The delete-with-dependents contract

`delete_domain` and `delete_practice` take an optional `disposition`:

```rust
enum DeleteDisposition {
    Cascade,                       // delete all dependents too
    ReparentTo { target: Uuid },   // move dependents under a sibling
    AbortIfOrphan,                 // fail the delete; dependents untouched
}
```

Without a disposition, the tool refuses — and the refusal lists the
dependent entities (names + UUIDs) so the orchestrator can dialog with
the user and re-issue the call with an explicit choice. The orchestrator
never guesses whether a destructive action is safe; the tool tells it.

## Authoring substates & `switch_focus`

A project carries one advisory field, `focus_substate`, one of four
**authoring substates**:

| Substate | What it steers |
|----------|----------------|
| Scoping | Describe the domain, audience, goals |
| Structuring | Propose domains + practices with CVR |
| Questions | Draft questions per practice |
| Refining | Polish, surgical edits before publish |

Substates are purely advisory — they choose which system-prompt fragment
the orchestrator runs and which UI accent shows. They gate nothing: the
surgical CRUD tools, `publish_assessment`, and `reset_draft_from_version`
all work from any substate. Data integrity comes from the draft/version
model above, not from a state machine.

`switch_focus(substate, reason)` writes `focus_substate` and nothing
else. The orchestrator infers focus from the conversation most of the
time and calls `switch_focus` only on a deliberate reset (the user says
"let's go back to the domains"). Over-calling is harmless.

The respondent and analyst experiences aren't substates — they're
separate binaries (`amaker-assess`, `amaker-analyze`), each with its own
prompt set and read-only-or-form UI.

## Related subsystems

- **Answer capture.** `amaker-assess` serves the response form;
  `PATCH /api/projects/{id}/response/answers/{question_id}` and its
  sibling `DELETE` upsert/clear a single answer.
- **Analysis layer.** `compute_scorecard`, `compute_gaps`,
  `compute_roadmap` are pure functions of `(Assessment, Response)` — they
  don't care whether the `Assessment` came from the draft or a published
  version. The narrative is the one LLM-backed analysis step.
- **Metamodel.** The CVR triad, binary polarity, per-assessment
  vocabularies, effort ranges, and respondent IDs are defined in
  [Core concepts](../vision/concepts.md).

## Testing strategy

The `InMemory` backend is the workhorse for unit and integration tests:
zero setup, deterministic, fast. The `LocalFileSystem` backend (rooted at
a `tempfile::TempDir`) is used for the few tests that inspect on-disk
shape.

- Each structural tool has a `#[tokio::test]` over an `InMemory`-backed
  `StorageService` asserting the resulting YAML.
- Delete-with-dependents rejection paths assert the error shape +
  dependent list.
- Publish / reset / response-load flows have end-to-end tests.
- `etag_conditional_write_detects_conflict` in `services/storage.rs`
  covers conditional-write semantics.

Tests need nothing external — no rustfs, no cloud credentials. The
trade-off: `InMemory` and `LocalFileSystem` condition writes on the ETag,
so a backend that conditions on a different field (GCS, on the
generation) is only exercised against a live deployment.

## Boundaries

- **Multi-tenancy.** A `tenants/{tenant_id}/projects/{project_id}/…`
  layout is the obvious next step — an extra prefix segment plus a tenant
  resolution step. Not built.
- **Auth and signed URLs.** Separate work, deferred.
- **Garbage collection of orphan version blobs.** Reclaimable manually if
  it ever happens; no automated GC.
- **Edit audit log.** The tool-result text returned to the orchestrator
  is the record of what changed. A durable append-only `events/*.json`
  stream is a possible future addition.

## References

In-code anchors:

- Backend selection: `crates/amaker-core/src/storage_backend.rs`
- Storage facade: `crates/amaker-core/src/services/storage.rs`
  (conditional write: `save_assessment_yaml_if_match`)
- Draft edit helper: `crates/amaker-core/src/services/draft.rs`
- Response loader + version binding:
  `crates/amaker-core/src/services/responses.rs`
- `publish_assessment`, `reset_draft_from_version`, surgical CRUD,
  `switch_focus`: `crates/amaker-author/src/services/tools.rs`
- UUID newtypes: `crates/amaker-core/src/models/ids.rs`
- Assessment / Project data models:
  `crates/amaker-core/src/models/assessment.rs`,
  `crates/amaker-core/src/models/project.rs`
- Response data model: `crates/amaker-core/src/models/response.rs`
