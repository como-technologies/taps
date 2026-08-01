# Metamodel

Every assessment follows a four-level hierarchy, defined in
`src/models/assessment.rs` and serialized with serde. The JSON Schema served
at `GET /api/schema` is generated from these same structs with
[schemars](https://docs.rs/schemars), so the model below *is* the export
contract.

```text
Assessment
  └─ Domain        (major focus areas; 3-7 per assessment recommended)
      └─ Practice  (specific capabilities; 2-5 per domain recommended)
          └─ Question  (binary checks; 3-12 per practice recommended)
```

The counts are authoring guidance baked into the schema descriptions, not
hard limits — nothing enforces them at validation time.

## Assessment (root)

| Field         | Type     | Notes                                   |
| ------------- | -------- | --------------------------------------- |
| `id`          | UUID     | defaults to a fresh UUID if omitted     |
| `name`        | string   | required                                |
| `description` | string   | required                                |
| `goal`        | string   | required                                |
| `domains`     | Domain[] | required                                |
| `created_at`  | datetime | defaults to now if omitted              |
| `updated_at`  | datetime | defaults to now if omitted              |

## Domain

| Field         | Type       | Notes                                          |
| ------------- | ---------- | ---------------------------------------------- |
| `id`          | UUID       | defaults if omitted                            |
| `name`        | string     | required                                       |
| `context`     | string     | what this domain covers                        |
| `value`       | string     | why it matters                                 |
| `risk`        | string     | consequences of ignoring it                    |
| `practices`   | Practice[] | required                                       |
| `terminology` | string?    | alternative label (e.g. "Stage", "Pillar")     |

## Practice

| Field         | Type       | Notes                                          |
| ------------- | ---------- | ---------------------------------------------- |
| `id`          | UUID       | defaults if omitted                            |
| `name`        | string     | required                                       |
| `context`     | string     | what this practice covers                      |
| `value`       | string     | why it matters                                 |
| `risk`        | string     | consequences of ignoring it                    |
| `questions`   | Question[] | required                                       |
| `guidance`    | string?    | optional implementation guidance               |
| `roles`       | string[]   | roles typically responsible (defaults empty)   |
| `effort`      | string?    | typical effort to implement if missing         |
| `terminology` | string?    | alternative label (e.g. "Capability")          |

## Question

Questions are the leaves: binary checks (yes/no/unknown), not free-form survey
prompts. Note that `context`/`value`/`risk` live on domains and practices —
questions carry `text` and `polarity` plus optional authoring aids.

| Field         | Type                     | Notes                                     |
| ------------- | ------------------------ | ----------------------------------------- |
| `id`          | UUID                     | defaults if omitted                       |
| `text`        | string                   | required                                  |
| `polarity`    | `positive` \| `negative` | required — see below                      |
| `guidance`    | string?                  | how to verify the question                |
| `evidence`    | string?                  | what would prove a "yes"                  |
| `remediation` | string?                  | what to do if the answer is "no"          |

### Polarity

- `positive` — "yes" means the practice is in place (the common case)
- `negative` — "yes" means a problem exists (risk-focused questions)
