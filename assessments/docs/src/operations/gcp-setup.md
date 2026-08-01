# Google Cloud setup

This takes you from nothing to a Google Cloud project ready for the
[Cloud Run deployment runbook](./deployment.md). Run it once.

Every step is a single copy-pasteable block: IDs are captured into shell
variables with `$(...)` rather than copied by eye, and each block re-reads
what it needs from `gcloud config`, so the blocks work whether you run them
in one session or one at a time.

Examples assume the **`comotechnologies.io`** organization, but every block
works the same on another Workspace domain or a personal Google account —
the org is detected automatically.

## 1. Install the `gcloud` CLI

| Platform | Command |
|----------|---------|
| macOS / Linux (Homebrew) | `brew install --cask google-cloud-sdk` |
| Linux / macOS (installer) | `curl https://sdk.cloud.google.com \| bash` then restart the shell |
| Debian/Ubuntu (apt) | Follow <https://cloud.google.com/sdk/docs/install#deb> |
| Windows | Installer at <https://cloud.google.com/sdk/docs/install> |

Confirm it's on `PATH`:

```bash
gcloud version
```

You also need **Docker** locally to build the images (see the deployment
runbook).

## 2. Sign in

```bash
gcloud auth login                       # opens a browser
gcloud auth application-default login   # ADC, used by local tooling
```

Sign in with your `comotechnologies.io` account — or any Google account; a
personal Gmail works fine.

## 3. Create the project

Project IDs are **globally unique** across all of Google Cloud. The block
below appends a random suffix so it works on first paste; replace the whole
value with a fixed ID of your choosing if you prefer (re-run if `create`
reports the ID is already in use).

The org is detected automatically — `ORG_ID` ends up empty on a personal
account, and the `--organization` flag is then omitted.

```bash
PROJECT_ID="amaker-$(openssl rand -hex 3)"
ORG_ID=$(gcloud organizations list --format='value(name.basename())' 2>/dev/null | head -n1)

gcloud projects create "$PROJECT_ID" --name="Amaker" \
  ${ORG_ID:+--organization="$ORG_ID"}
gcloud config set project "$PROJECT_ID"

echo "Created project '$PROJECT_ID' (organization: ${ORG_ID:-none, personal account})"
```

From here on, every block reads the project back from `gcloud config`, so you
never have to retype or remember the ID.

## 4. Link a billing account

Cloud Run, Artifact Registry, and Cloud Storage all require billing — even
though Cloud Run's free tier covers light usage.

```bash
BILLING_ID=$(gcloud billing accounts list \
  --filter='open=true' --format='value(name.basename())' | head -n1)

gcloud billing projects link "$(gcloud config get-value project 2>/dev/null)" \
  --billing-account="$BILLING_ID"

echo "Linked billing account: $BILLING_ID"
```

If `BILLING_ID` comes back empty, create a billing account at
<https://console.cloud.google.com/billing> first. If you have more than one,
this picks the first open account — set `BILLING_ID` by hand to choose.

## 5. Enable the required APIs

Acts on the active project from step 3:

```bash
gcloud services enable \
  run.googleapis.com \
  artifactregistry.googleapis.com \
  secretmanager.googleapis.com \
  storage.googleapis.com
```

## 6. Pick a default region

```bash
gcloud config set run/region us-central1
```

Swap `us-central1` for a region near you. Later blocks read this back.

## 7. Create the container registry

The three service images are pushed to an Artifact Registry Docker repo:

```bash
REGION=$(gcloud config get-value run/region 2>/dev/null)

gcloud artifacts repositories create amaker \
  --repository-format=docker \
  --location="$REGION" \
  --description="Amaker service images"

gcloud auth configure-docker "${REGION}-docker.pkg.dev"

echo "Image prefix: ${REGION}-docker.pkg.dev/$(gcloud config get-value project 2>/dev/null)/amaker"
```

That printed image prefix is the `$REPO` value the deployment runbook uses —
the runbook re-derives it the same way, so there's nothing to copy down.

## 8. (Optional) Custom domain

By default each service gets a `https://<service>-<hash>.<region>.run.app`
URL, which works fine. To serve under `comotechnologies.io` instead:

1. **Verify domain ownership** — once per domain:

   ```bash
   gcloud domains verify comotechnologies.io
   ```

   This opens Search Console; add the TXT record it gives you to the
   domain's DNS.

2. **Map a subdomain to each service** (after the services exist — see the
   deployment runbook):

   ```bash
   REGION=$(gcloud config get-value run/region 2>/dev/null)
   for sub in author assess analyze; do
     gcloud beta run domain-mappings create \
       --service="amaker-$sub" \
       --domain="$sub.comotechnologies.io" \
       --region="$REGION"
   done
   ```

   Each mapping prints DNS records (usually `CNAME`s) to add at your DNS
   provider.

3. Use those domains as the `*_BASE_URL` values in the runbook instead of
   the `.run.app` URLs — and you can skip its two-pass deploy, since the
   URLs are known up front.

**Other domains / personal accounts:** swap `comotechnologies.io` for any
domain you control. With no domain at all, skip this section entirely and
use the `.run.app` URLs.

> Cloud Run domain mappings aren't offered in every region. Where they
> aren't, front the services with a global external Application Load
> Balancer + serverless NEGs instead — see the Cloud Run docs.

## Next

Your project is ready. Continue with the
[Cloud Run deployment runbook](./deployment.md) to build, push, and deploy
the three services.
