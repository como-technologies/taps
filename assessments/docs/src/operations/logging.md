# Logging & debugging

All three binaries log to **stdout only** — there is no file-based logging.
In a container the platform captures stdout; locally you read it in the
terminal.

## Two formats, auto-selected

| Format | Looks like | Used when |
|--------|-----------|-----------|
| **text** | pretty, colored, human-readable | local dev (`just run`, `cargo run`) |
| **JSON** | one Cloud Logging structured object per line | Cloud Run |

Selection (`amaker_core::observability::init_tracing`):

1. `LOG_FORMAT` env var — `json` or `text` — wins if set.
2. Otherwise JSON when `K_SERVICE` is set (Cloud Run injects it).
3. Otherwise text.

So a local run stays readable and Cloud Run gets structured logs with no
configuration. To force either way:

```bash
LOG_FORMAT=json just run-author      # structured JSON locally
LOG_FORMAT=text  ...                 # plain text in a container
```

A JSON line carries a real `severity` derived from the `tracing` level —
this is what makes severity-based log queries work:

```json
{"time":"2026-05-17T02:57:05.4Z","severity":"ERROR","target":"amaker_core::error","message":"Error: CompletionError ...","logging.googleapis.com/sourceLocation":{"file":"crates/amaker-core/src/error.rs","line":"42"}}
```

| `tracing` level | Cloud Logging `severity` |
|-----------------|--------------------------|
| `TRACE` / `DEBUG` | `DEBUG` |
| `INFO` | `INFO` |
| `WARN` | `WARNING` |
| `ERROR` | `ERROR` |

## Log level

`<PREFIX>_LOG_LEVEL` (`AUTHOR_` / `ASSESS_` / `ANALYZE_`; default `info`)
takes standard [`tracing` `EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html)
syntax, so you can scope a level to a target:

```bash
# everything at info, but the surgical tools at debug
AUTHOR_LOG_LEVEL='info,amaker_author::services::tools=debug' just run-author
```

The author binary pins `rig::agent::prompt_request=warn` by default — that
target prints the full tool args + result of every tool call, which at
assessment size is the entire prompt. Override it explicitly if you need
that firehose:

```bash
AUTHOR_LOG_LEVEL='info,rig::agent::prompt_request=info' just run-author
```

## Reading logs on Cloud Run

> **Don't use `gcloud run services logs read`.** It renders the legacy
> `textPayload`, which structured JSON entries don't have — you'll see
> near-blank lines. Use `gcloud logging read` (below) or the
> [Logs Explorer](https://console.cloud.google.com/logs) instead.

**Recent application logs** for one service:

```bash
gcloud logging read \
  'resource.type=cloud_run_revision
   AND resource.labels.service_name=amaker-author
   AND jsonPayload.message:*' \
  --freshness=1h --limit=50 \
  --format='value(severity, jsonPayload.message)'
```

**Errors and warnings only** — now that `severity` is real:

```bash
gcloud logging read \
  'resource.type=cloud_run_revision
   AND resource.labels.service_name=amaker-author
   AND severity>=WARNING
   AND jsonPayload.message:*' \
  --freshness=1h \
  --format='value(severity, jsonPayload.message)'
```

The `jsonPayload.message:*` clause matters: Cloud Run also emits its own
HTTP **request logs**, which are severity-tagged from the response status
(a `502` → `ERROR`, a `404` → `WARNING`) but carry an `httpRequest`, not a
`message`. Without that clause `severity>=WARNING` returns those too, with
blank message columns. To look at *just* the request logs instead:

```bash
gcloud logging read \
  'resource.type=cloud_run_revision
   AND resource.labels.service_name=amaker-author
   AND httpRequest.status>=400' \
  --freshness=1h \
  --format='value(httpRequest.status, httpRequest.requestUrl)'
```

**Follow logs live** (`tail -f` equivalent):

```bash
gcloud beta logging tail \
  'resource.type=cloud_run_revision AND resource.labels.service_name=amaker-author'
```

**One project's activity across all three services** — the project UUID
appears in handler log lines, so a substring match works:

```bash
gcloud logging read \
  'resource.type=cloud_run_revision
   AND jsonPayload.message:"b82ae76a-ca63-40d7-9aaf-94898b325240"' \
  --freshness=6h --format='value(resource.labels.service_name, severity, jsonPayload.message)'
```

## Local debugging

The format stays text locally, so just raise the level:

```bash
AUTHOR_LOG_LEVEL=debug just run-author
```

`just run` prefixes each binary's lines (`[author]`, `[assess]`,
`[analyze]`) so the three interleaved streams stay legible.

## Health checks

Each service serves `GET /healthz` → `200 ok` — a process-up probe that
touches no storage or LLM. Locally:

```bash
curl -fsS http://localhost:3000/healthz && echo ok
```

On the deployed services this path is behind IAP too (see
[Authentication](./auth.md)), so an unauthenticated `curl` gets the IAP
sign-in redirect, not `ok`. Cloud Run's own startup/liveness probe hits
the container directly, ahead of IAP, so it still works as a probe.
