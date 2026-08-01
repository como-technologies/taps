# Authentication (IAP)

The deployed services are gated by **Identity-Aware Proxy (IAP)** — Google's
proxy runs the entire sign-in flow in front of each Cloud Run service, so a
request never reaches the app (or the Anthropic API) without an allowed
Google identity. There is no auth code in the app: IAP is the whole story.

Access is an **allowlist** kept in the repo at
[`deploy/iap-allowlist.txt`](https://github.com/como-technologies/taps/blob/main/assessments/deploy/iap-allowlist.txt)
and synced to IAM by `just iap-sync` — the modern equivalent of an
`htpasswd` file, except it's auditable and version-controlled.

All four Cloud Run services are gated — `amaker-author`,
`amaker-assess`, `amaker-analyze`, and `amaker-docs` (the mdBook user
manual). `amaker-assess` is the respondent form; gating it means
respondents must currently be allowlisted Google accounts too. Real
external-respondent access (per-assessment invite links) is separate,
later work.

## Prerequisites

- The services are deployed (see [Deploying to Cloud Run](./deployment.md)).
- The IAP and Cloud Resource Manager APIs are enabled:

  ```bash
  gcloud services enable iap.googleapis.com cloudresourcemanager.googleapis.com
  ```

- The **OAuth consent screen** is configured once, in the Cloud Console
  under *APIs & Services → OAuth consent screen*. Choose the **External**
  user type — `Internal` would restrict sign-in to the Workspace org and
  block the individual outside accounts you want to allow. The consent
  screen only governs who can *complete a Google sign-in*; the IAP
  allowlist below is the actual access control. (`gcloud … --iap` in the
  next step provisions the OAuth *client* itself — only the consent
  screen needs a human.)

## 1. Enable IAP

Turns IAP on for every service in `IAP_SERVICES`:

```bash
just iap-enable
```

The first `--iap` call in a project may print a warning suggesting you
enable IAP via the Cloud Run console UI. It generally still succeeds from
the CLI — confirm by checking that an unauthenticated request gets
bounced to Google (see [Verification](#verification)). If it genuinely
didn't take, toggle IAP once on any service in the Cloud Run console
(*Security* tab); that bootstraps the project, then re-run `just iap-enable`.

(That runs `gcloud run services update <svc> --region <region> --iap` for
each.) Once enabled, every request to a service's URL is intercepted by
IAP — an unauthenticated visitor gets the Google sign-in page.

## 2. Set the allowlist

Edit `deploy/iap-allowlist.txt` — one member per line:

```
# The whole Workspace org:
domain:comotechnologies.io
# Individual Google accounts:
user:alice@gmail.com
user:bob@gmail.com
```

Then apply it to every service:

```bash
just iap-sync
```

`iap-sync` treats the file as the source of truth: it grants
`roles/iap.httpsResourceAccessor` to every member listed and **revokes**
anyone holding it who isn't. To see the current state:

```bash
just iap-status
```

Adding or removing someone is: edit the file, commit it, `just iap-sync`.

## How access is decided

A request reaches an app only if **both** hold:

1. The visitor completes Google sign-in (governed by the OAuth consent
   screen).
2. Their identity matches a member in `deploy/iap-allowlist.txt` — either
   the `domain:` binding (any `comotechnologies.io` account) or a
   `user:` / `group:` binding.

Everyone else gets an IAP "you don't have access" page. The app never
runs, so the Anthropic API is never touched.

## Verification

A quick non-browser check that IAP is intercepting — an unauthenticated
request should get a `302` to Google with an `x-goog-iap` header, not the
app:

```bash
curl -s -D - -o /dev/null https://amaker-author-…run.app/ | grep -iE 'HTTP/|x-goog-iap'
# HTTP/2 302
# x-goog-iap-generated-response: true
```

Then in a browser:

- Signed into an allowlisted account → you land in the app.
- Incognito, or signed into a non-allowlisted account → IAP blocks you
  before the app loads.

## Known limitation: the "Regenerate narrative" button

That button is a cross-origin POST from `amaker-analyze` to
`amaker-author`. Under IAP the two services are separate origins with
separate IAP sessions, so the POST only succeeds once the user has also
loaded the `amaker-author` app directly in the same browser (which mints
an author-side IAP session). For internal users working across all three
apps this generally just works; a robust fix — `amaker-analyze` calling
`amaker-author` server-to-server with a service-account identity token —
is follow-up work.

## What the app could do with the identity

IAP forwards the verified user on the `X-Goog-Authenticated-User-Email`
request header. The app doesn't read it today, but it's there for a
future "signed in as …" indicator or per-user audit logging — no
re-verification needed; IAP has already authenticated it.
