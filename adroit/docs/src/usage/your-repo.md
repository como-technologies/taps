# Using adroit with Your Repo

adroit operates **exclusively against a KB space** (ADR-0020): a directory
carrying a `wiki.toml`, with decision pages at `<wiki_root>/decisions/`
(default `wiki/decisions/`). In practice the space lives **inside a real
project repo** — decisions committed alongside code — or is stood up
ephemerally per checkout and seeded from a committed legacy corpus. This guide
ties the pieces together.

## 1. Point adroit at your space

Every command accepts a global `--dir` flag that names the **space root** (the
directory holding `wiki.toml`) and takes precedence over config:

```sh
adroit --dir /path/to/kb-space list
```

A `--dir` that is **not** a space is a hard error naming the bootstrap path:

```
not a KB space (no wiki.toml): … — create one with `llm-wiki spaces create`
(or scaffold wiki.toml + wiki/decisions) and seed it with
`adroit seed --from <legacy-dir>`
```

Bootstrap options:

- **With llm-wiki** (the full substrate — schema validation + git-hook
  admission): `llm-wiki spaces create` provisions the space in one command.
- **Minimal scaffold** (no llm-wiki needed — adroit only requires the shape):

  ```sh
  mkdir -p my-space/wiki/decisions
  printf 'name = "my-decisions"\n' > my-space/wiki.toml
  ```

- **From a legacy corpus**: seed a fresh space from a pre-KB ADR tree
  (`# ADR-NNNN:`-style markdown, by-status subdirs or flat):

  ```sh
  adroit seed --from docs/src/adr --dir my-space
  ```

  `seed` refuses a space that already contains ADRs, so it is safe to wire
  into a gate that stands up a throwaway space per run — see
  [CI integration](./ci-integration.md).

The scaffolding verbs (`new` / `import` / `init` / `seed`) create the
`decisions/` directory *inside* an existing space when it's missing, but never
the space itself; and a `--dir` that doesn't exist at all fails with an error
naming the path — a typo'd path surfaces loudly instead of masquerading as an
empty repo.

## 2. Make it the default (skip `--dir` every time)

You have two ways to make a space the default so you can skip `--dir`:

**A `.env` file (per-repo, recommended for a checked-out repo).** adroit loads a
`.env` from the current directory (or a parent) at startup. Copy the tracked
`.env.example` and edit it (your local `.env` is git-ignored):

```sh
cp .env.example .env
# .env  (in your working tree)
ADROIT_DIR=/path/to/kb-space
```

Now plain `adroit list`, `adroit serve`, etc. target that space. Every other
setting has a matching `ADROIT_*` variable that works the same way
(`ADROIT_NAMING`, `ADROIT_DATE_SOURCE`, …, plus the dashboard's `ADROIT_HOST` /
`ADROIT_PORT`). A real shell environment variable overrides the `.env` file.

> **Heads-up:** `ADROIT_DIR` is tilde / `$VAR`-expanded too, so
> `ADROIT_DIR=~/repo/space` works from a `.env` (the shell never sees it to
> expand the `~`).

**Or the user config (global default).** Set `dir` in
`~/.config/adroit/config.yaml` so plain `adroit` commands target your space:

```yaml
dir: /path/to/kb-space
```

`dir` supports `~` and `$ENV_VAR` expansion. Relative values resolve from
adroit's data directory, so use an absolute path (or `~/…`) to point at a repo
elsewhere on disk. The full set of config keys — `default_template`,
`templates_dir`, `default_status`, `open_on_new`, `summary_path`,
`review_days`, `review_quorum`, `review_overdue_days`, `tui_theme`,
`date_source`, `naming` — is documented in the
[CLI Reference](../reference/cli.md#configuration). Run `adroit config` to
see each one's resolved value and where it came from.

If your repo uses its own ADR template, drop it at
`<space>/wiki/decisions/adr-template.md` (adroit prefers a repo-local
template) or set `templates_dir`/`default_template`.

## 3. The daily loop

```sh
# Capture a decision (a proposed page in wiki/decisions/, opens your editor)
adroit new "Use PostgreSQL for the primary datastore"

# Find prior decisions mid-discussion
adroit search postgres

# Propose a review deadline; once it passes, the ADR shows as review-due
adroit set-review 9 2026-07-15

# Generate the review-kickoff doc when it's ready for a formal decision
adroit review 9 --out review-kickoff.md

# Record the outcome — rewrites the page's frontmatter in place
adroit set-status 9 accepted
# ...or supersede an older decision with a newer one
adroit supersede 9 4

# Keep the published index in sync, then commit
adroit index
git add -A && git commit -m "ADR-0009: accept PostgreSQL"
```

Prefer an interactive surface? Run bare `adroit` for the [TUI](./tui.md)
(browse, triage, and edit in the terminal), or `adroit serve` for the read-only
[web dashboard](./web.md) (browse, search, stats, and a relationship graph that
auto-refreshes as you edit).

## 4. Keep `SUMMARY.md` in sync

If your repo publishes via mdBook (or Confluence from `SUMMARY.md`), `adroit
index` regenerates the ADR section grouped by status (from each page's
frontmatter), preserving the rest of the file. Point it explicitly with
`summary_path` in config, or let adroit discover a `SUMMARY.md` next to or one
level above the decisions directory.

## 5. Gate it in CI

Two commands exit non-zero on a problem, so they drop straight into a CI job to
keep the ADR corpus honest:

```sh
adroit check          # structural validation: duplicate numbers, unparseable
                      # pages, broken supersession refs, broken/stale links
adroit index --check  # fail if SUMMARY.md is stale (run `adroit index` locally)
```

`check` prints each problem to stderr and a one-line summary on failure; on a
clean repo it prints `OK: N ADRs, no problems`. `index --check` never writes —
it just verifies `SUMMARY.md` matches what `adroit index` would produce. A repo
whose committed corpus is still legacy-format keeps its gate green by
bootstrapping an ephemeral space (`seed` + `check`) per run — see
[CI integration](./ci-integration.md).

## A note on safety

adroit's writes are **minimal-diff**: status changes rewrite only the
frontmatter line that changed, body edits rewrite only the body, foreign
frontmatter keys are preserved verbatim, and an unchanged round-trip is
byte-identical. Even so, since these are your real, version-tracked files, your
git history is the backstop — review `git diff` before committing, as you would
for any change.
