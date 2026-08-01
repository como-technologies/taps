# adroit CI templates

Drop-in CI starters that bake the ADR process into your pipeline. Copy the one
for your platform into **your ADR repo** (not this repo) and adjust the two
knobs at the top: the KB space directory and how `adroit` is obtained.

They encode the two-stage workflow adroit is built around:

1. **Propose on `main`.** ADR content is written and iterated directly on the
   default branch as `status: proposed` pages under `wiki/decisions/`. No gate —
   the goal is low friction.
2. **Decide via PR/MR.** The decision (Proposed → Accepted / Rejected) is the
   PR/MR: it flips the page's `status:` frontmatter in place (the file never
   moves). That's where the team reviews.

What the pipelines do:

- **On every push/PR to `main`** — run `adroit check` (duplicate numbers,
  unparseable pages, broken supersession refs, broken links) and
  `adroit index --check` (SUMMARY.md is in sync). `adroit check` exits
  non-zero only on **errors**; a **stale link** (one that still names an
  existing ADR) is a warning and does not fail the build.
- **In the merge queue / merge train** — `adroit check` also runs on the
  *speculative merge* (`merge_group` on GitHub; merged-results pipelines on
  GitLab). This is what catches an ADR-number collision **between** branches:
  two PRs that each add `0009-*.md` pass on their own branch but fail once both
  land in the merge group, so the second is ejected. Resolve with
  `adroit renumber <dup> <next-free>`.
- **On a decision PR/MR** — generate the review-kickoff doc with
  `adroit review <n>` and post it as the PR/MR description, so reviewers get a
  consistent "here's what you're deciding" brief.
- **After each merge to `main`** — the relink workflow runs `adroit relink`
  (idempotent) and commits the result. Status changes rewrite in place, so
  this is a no-op unless a `renumber` or an out-of-band edit left a link
  pointing at an old filename.

(For a team that wants to avoid number collisions by construction, `adroit`
also supports the `date`/`uuid` naming schemes — see the user manual's
"Concurrent contributors" page.)

## Files

- `github/adr.yml` → copy to `.github/workflows/adr.yml` (validate + review brief)
- `github/relink.yml` → copy to `.github/workflows/adr-relink.yml` (post-merge relink)
- `gitlab/.gitlab-ci.yml` → copy to your repo root (or `include:` it) — includes
  the `adr:relink` job

## Two knobs (top of each file)

- **`ADROIT_DIR`** — path to your KB space root (the directory holding
  `wiki.toml`; decision pages live under `wiki/decisions/` inside it).
- **How `adroit` is installed** — the templates show `cargo install --git`
  (simplest), but pin to a tag, vendor a release binary, or use a prebuilt
  image as you prefer. adroit isn't published to crates.io yet.

These are starting points, not a framework — read them and make them yours.
