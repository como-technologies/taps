# Deploying to Google Cloud Run

Amaker deploys as **three independent Cloud Run services** — `amaker-author`,
`amaker-assess`, `amaker-analyze` — sharing one blob-storage bucket. Each is a
container built from the repo's single [`Dockerfile`](https://github.com/como-technologies/taps/blob/main/assessments/Dockerfile).

This is a runbook, not a GCP tutorial — work through
[Google Cloud setup](./gcp-setup.md) first (gcloud installed, project created,
billing linked, APIs enabled, Artifact Registry repo created).

Like the setup guide, every block is self-contained: it re-reads the project
and region from `gcloud config` and derives names from them, so there are no
placeholders to hand-edit and nothing to copy between blocks. Run the runbook
top to bottom.

## Why three services

The binaries are separate origins by design (see
[Draft / Publish](../architecture/draft-publish.md)): the author holds the LLM
transport, assess is a no-LLM respondent form, analyze is a read-only viewer.
They talk to each other over HTTP and share state only through the blob store —
never a local disk. That's what makes each one a clean stateless container.

## 1. Storage bucket

The binaries never create the bucket — `object_store` operates at the object
level. Create it once, up front.

The simplest backend on GCP is **native GCS** (`STORAGE_BACKEND=gcs`): the
Cloud Run service account authenticates via Application Default Credentials,
so there are no access keys to manage. The bucket name is derived from the
project ID, which is itself globally unique:

```bash
PROJECT_ID=$(gcloud config get-value project 2>/dev/null)
REGION=$(gcloud config get-value run/region 2>/dev/null)
BUCKET="${PROJECT_ID}-data"

gcloud storage buckets create "gs://${BUCKET}" --location="$REGION"

# Let the Cloud Run runtime service account read/write the bucket.
SA="$(gcloud projects describe "$PROJECT_ID" \
  --format='value(projectNumber)')-compute@developer.gserviceaccount.com"
gcloud storage buckets add-iam-policy-binding "gs://${BUCKET}" \
  --member="serviceAccount:${SA}" --role=roles/storage.objectAdmin

echo "Bucket: gs://${BUCKET}"
```

All three services point at this one bucket so they read each other's writes.

> The S3-compatible backend (`STORAGE_BACKEND=s3_compatible`) also works against
> Cloudflare R2, Backblaze B2, Hetzner, etc. — set the `*_S3_*` vars instead.
> See [.env.example](https://github.com/como-technologies/taps/blob/main/assessments/.env.example).

## 2. Secret — the Anthropic API key

Only the author binary needs a secret; assess and analyze need none under the
`gcs` backend. The block reads the key without echoing it to the terminal or
shell history:

```bash
read -rs -p "Anthropic API key: " ANTHROPIC_KEY && echo
printf '%s' "$ANTHROPIC_KEY" | gcloud secrets create anthropic-api-key --data-file=-
unset ANTHROPIC_KEY

# Let the Cloud Run runtime service account read the secret.
SA="$(gcloud projects describe "$(gcloud config get-value project 2>/dev/null)" \
  --format='value(projectNumber)')-compute@developer.gserviceaccount.com"
gcloud secrets add-iam-policy-binding anthropic-api-key \
  --member="serviceAccount:${SA}" --role=roles/secretmanager.secretAccessor
```

## 3. Build and push the images

One `Dockerfile`, three images, selected with `--build-arg BIN`:

```bash
PROJECT_ID=$(gcloud config get-value project 2>/dev/null)
REGION=$(gcloud config get-value run/region 2>/dev/null)
REPO="${REGION}-docker.pkg.dev/${PROJECT_ID}/amaker"

for bin in amaker-author amaker-assess amaker-analyze; do
  docker build --build-arg BIN="$bin" -t "$REPO/$bin:latest" .
  docker push "$REPO/$bin:latest"
done
```

The builder compiles the whole workspace once; the 2nd and 3rd builds reuse it
via BuildKit cache mounts.

## 4. Deploy — pass 1

The services reference each other by URL (`AUTHOR_ANALYZE_BASE_URL`, etc.), but
a service's URL isn't known until it exists. So deploy in two passes — first
create all three (cross-links unset for now):

```bash
PROJECT_ID=$(gcloud config get-value project 2>/dev/null)
REGION=$(gcloud config get-value run/region 2>/dev/null)
REPO="${REGION}-docker.pkg.dev/${PROJECT_ID}/amaker"
BUCKET="${PROJECT_ID}-data"

gcloud run deploy amaker-author --image "$REPO/amaker-author:latest" \
  --region "$REGION" --allow-unauthenticated \
  --set-env-vars "AUTHOR_STORAGE_BACKEND=gcs,AUTHOR_GCS_BUCKET=${BUCKET}" \
  --set-secrets ANTHROPIC_API_KEY=anthropic-api-key:latest

gcloud run deploy amaker-assess --image "$REPO/amaker-assess:latest" \
  --region "$REGION" --allow-unauthenticated \
  --set-env-vars "ASSESS_STORAGE_BACKEND=gcs,ASSESS_GCS_BUCKET=${BUCKET}"

gcloud run deploy amaker-analyze --image "$REPO/amaker-analyze:latest" \
  --region "$REGION" --allow-unauthenticated \
  --set-env-vars "ANALYZE_STORAGE_BACKEND=gcs,ANALYZE_GCS_BUCKET=${BUCKET}"
```

## 5. Deploy — pass 2 (cross-service URLs)

This block reads back the three assigned URLs (stable across future revisions)
and wires them in. `--update-env-vars` keeps the storage vars from pass 1:

```bash
REGION=$(gcloud config get-value run/region 2>/dev/null)
url() { gcloud run services describe "$1" --region "$REGION" --format='value(status.url)'; }
A=$(url amaker-author); S=$(url amaker-assess); N=$(url amaker-analyze)
echo "author=$A  assess=$S  analyze=$N"

gcloud run services update amaker-author --region "$REGION" --update-env-vars \
  "AUTHOR_ASSESS_BASE_URL=${S},AUTHOR_ANALYZE_BASE_URL=${N},AUTHOR_PUBLIC_URL=${A}"

gcloud run services update amaker-assess --region "$REGION" --update-env-vars \
  "ASSESS_AUTHOR_BASE_URL=${A},ASSESS_ANALYZE_BASE_URL=${N}"

gcloud run services update amaker-analyze --region "$REGION" --update-env-vars \
  "ANALYZE_AUTHOR_BASE_URL=${A},ANALYZE_ASSESS_BASE_URL=${S}"
```

`AUTHOR_ANALYZE_BASE_URL` is load-bearing twice over: it's both a header link
*and* the CORS allow-origin for the cross-origin "Regenerate narrative" POST
that the analyze page sends to the author. It must equal analyze's real URL.

Open the author URL (`echo $A`) to use the app.

## 6. Lock the services down

The deploy commands above use `--allow-unauthenticated`, which leaves the
services open to the world — fine for a first smoke test, **not** for
anything left running (the author service spends Anthropic tokens). Gate
all three behind Identity-Aware Proxy before real use:
[Authentication (IAP)](./auth.md).

> Mapping custom domains up front (see [Google Cloud setup](./gcp-setup.md#8-optional-custom-domain))
> sidesteps the two-pass dance — you know the URLs before the first deploy.

## Configuration reference

`<PREFIX>` is `AUTHOR` / `ASSESS` / `ANALYZE`.

| Variable | Services | Notes |
|----------|----------|-------|
| `ANTHROPIC_API_KEY` | author | Required. Unprefixed. From Secret Manager. |
| `PORT` | all | Injected by Cloud Run; the binary honors it. |
| `<PREFIX>_HOST` | all | Set to `0.0.0.0` by the Dockerfile. |
| `<PREFIX>_STORAGE_BACKEND` | all | `gcs` (recommended on GCP), `s3_compatible`, `filesystem`, `in_memory`. |
| `<PREFIX>_GCS_BUCKET` | all | The shared bucket, when backend is `gcs`. |
| `<PREFIX>_LOG_LEVEL` | all | Default `info`. `EnvFilter` syntax — see [Logging](./logging.md). |
| `LOG_FORMAT` | all | `json` / `text`. Unset ⇒ JSON on Cloud Run (`K_SERVICE`), text locally. |
| `AUTHOR_ASSESS_BASE_URL` / `AUTHOR_ANALYZE_BASE_URL` / `AUTHOR_PUBLIC_URL` | author | Cross-service URLs. |
| `ASSESS_AUTHOR_BASE_URL` / `ASSESS_ANALYZE_BASE_URL` | assess | Cross-service URLs. |
| `ANALYZE_AUTHOR_BASE_URL` / `ANALYZE_ASSESS_BASE_URL` | analyze | Cross-service URLs. |
| `AUTHOR_AI_MODEL` | author | Optional model pin. |

## Operational notes

- **Health checks** — each service serves `GET /healthz` → `200`. Wire it as
  the Cloud Run startup/liveness probe.
- **Graceful shutdown** — the binaries drain in-flight requests on `SIGTERM`,
  which Cloud Run sends before stopping a revision.
- **Logs** — everything goes to stdout as Cloud Logging structured JSON
  (real `severity`, queryable). No file-based logging. See
  [Logging & debugging](./logging.md) for `gcloud logging read` recipes.
- **Scaling** — the storage layer uses ETag conditional writes, so multiple
  instances of any service are safe to run concurrently. Scale-to-zero is fine.
- **Auth** — gate the services behind Identity-Aware Proxy; see
  [Authentication (IAP)](./auth.md). The deploy steps above leave them
  open, which is only safe for a throwaway smoke test.

## Docs site

This manual itself deploys as a fourth Cloud Run service, `amaker-docs` —
the mdBook output served by nginx, built from `Dockerfile.docs`:

```bash
just deploy-docs
```

It's a static site with no storage, no secrets, and no link from the
three apps — devs just need its URL. Gate it behind IAP like the rest:
`amaker-docs` is already in `IAP_SERVICES`, so `just iap-enable` and
`just iap-sync` cover it. See [Authentication (IAP)](./auth.md).

## Local parity

`just run` runs the three apps with the filesystem backend;
`just run-*-rustfs` runs them against a local rustfs server (an
S3-compatible target) — the same code path as a cloud S3 backend. The
docs build locally with `just book` / `just book-serve`. See the project
README.
