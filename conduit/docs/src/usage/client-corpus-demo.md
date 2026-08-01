# Client-corpus demo

The parameterized dogfood beat: conduit drives work on the **client
corpus** — the fictional client's decision corpus, a generated repo seeded
from llm-wiki's starter content — instead of its own repo. Same loop,
different corpus, proving the demo machinery is parameterized rather than
hardwired to conduit's self-dogfood. Validated end-to-end against the
throwaway forge on 2026-07-28; every output below is captured from that run
(`FORGE_PORT=3210` on that host — the default is 3000).

Three demo-shape points over the [original walkthrough](./demo.md):

1. **A constructed corpus repo.** The client corpus is a generated
   artifact, never a checkout of a real repo: `demo/client-corpus-build.sh`
   copies llm-wiki's starter content (`kit/starter/decisions/` — 5 accepted
   + 11 proposed decisions in the legacy ADR format the pinned adroit's
   `seed` bootstraps a KB space from)
   into `docs/src/adr/` of a fresh single-commit git repo under the
   gitignored `.como/build/client-corpus`, and prints its path. The
   llm-wiki checkout itself resolves through the suite resolution
   convention (ADR-0014): `COMO_LLM_WIKI_DIR` → the sibling `../llm-wiki` →
   a read-only clone into the gitignored `.como/deps/llm-wiki` cache from
   `${COMO_LLM_WIKI_GIT:-${COMO_GIT_BASE:-https://github.com/como-technologies}/llm-wiki.git}`
   → a hard, actionable error naming those knobs. `COMO_OFFLINE=1` uses a
   populated cache as-is and never clones. llm-wiki is only ever read;
   the built repo is wiped and rebuilt on every run.
2. **Parameterized seeding.** `demo/gitea-init.sh` takes `SEED_REPO_DIR`
   (which local repo's `main` seeds the forge) and `REPO_NAME` (the forge
   repo under org `como`). Defaults preserve the self-dogfood demo
   (`.`/`conduit-dogfood`); token filenames stay pinned at
   `.secrets/conduit-bot.token` and `.secrets/reviewer.token` either way.
   `demo/demo-trigger.sh` takes the same `REPO_NAME`.
3. **Per-run unique workdirs.** Run 1 taught that the repo's shared
   `.conduit/` store is not single-writer: two flows interleaving in one
   store stomp each other's cursors and task records. The demo flow writes
   ALL its state under a caller-supplied or timestamped workdir created by
   `demo/client-corpus-init.sh` — never a shared fixed path.

## 1. Forge up, seeded with the client corpus

```sh
SEED_REPO_DIR="$(bash demo/client-corpus-build.sh)" REPO_NAME=client-corpus just forge-up
```

(When a `client-corpus`-named seed checkout is absent, `gitea-init.sh`
invokes the builder itself before failing with the knob-naming error.)

Captured:

```text
client-corpus-build: llm-wiki -> ../llm-wiki (sibling checkout)
client-corpus-build: built .../conduit/.como/build/client-corpus (corpus: docs/src/adr, from .../llm-wiki/kit/starter/decisions)
created user conduit-bot
created user reviewer
minted token for conduit-bot -> .secrets/conduit-bot.token
minted token for reviewer -> .secrets/reviewer.token
To http://localhost:3210/como/client-corpus.git
 * [new branch]      main -> main
forge ready: http://localhost:3210 (org como, repo client-corpus; tokens in .secrets/)
```

## 2. The per-run workdir

```sh
RUN_DIR=$(bash demo/client-corpus-init.sh)
cd "$RUN_DIR"
```

The script takes an already-built corpus repo via `CLIENT_CORPUS_DIR`
(demo-up and the tests pass one) or builds it via
`demo/client-corpus-build.sh` (the llm-wiki resolution chain above),
refuses to reuse an existing dir (unique per run; default
`demo/runs/<UTC timestamp>`, gitignored), and stocks the workdir with
everything `conduit` resolves from its cwd:

- `corpus-space` — the per-run KB space (the pinned adroit is KB-only per
  its ADR-0020): a hand-scaffolded `wiki.toml` + `wiki/decisions`, seeded
  from the built corpus repo's legacy `docs/src/adr` with the pinned
  adroit's `seed`, then git-inited (KB spaces are git-backed; llm-wiki's
  ingest commits into them). The corpus repo — and the forge seed — stays
  legacy-format: repo of record canonical, space derived (ADR-0017).
  Wiped and reseeded per run like every other workdir artifact.
- `conduit.toml` — `demo/client-corpus.conduit.toml` with the placeholders
  resolved: `[adroit] dir` → the workdir's `corpus-space`, and
  the forge `repo` → the `REPO_NAME` knob (default `client-corpus`,
  matching `forge-up`/`demo-trigger`). Point the machinery at a different
  corpus repo by setting `REPO_NAME` — no hand-edit of the generated config
  (`tests/demo_init.rs` pins this, plus the space scaffold + seed + config
  wiring). The rest of the config is explicit: gitea `como/<REPO_NAME>`,
  fake engine.
- `.secrets` — symlink to the repo's gitignored token dir
- `.conduit/bin` — symlink to the pinned adroit install

Captured (rehearsal 4, where demo-up drives this script):

```text
Seeded 16 ADR(s) from .../conduit/.como/build/client-corpus/docs/src/adr into demo/runs/kit-20260729T012852Z/corpus-space/wiki/decisions.
demo workdir ready: .../conduit/demo/runs/kit-20260729T012852Z (corpus space: .../demo/runs/kit-20260729T012852Z/corpus-space, seeded from .../conduit/.como/build/client-corpus/docs/src/adr)
```

Task records, plan snapshots, cursors, the git cache, and engine workspaces
all land under `<workdir>/.conduit/` — inspectable, disposable, and never
shared with another run. (This one-workdir-per-corpus shape is also the
supported multi-repo answer per ADR-0011.)

## 3. The dogfood input

```sh
conduit init
.conduit/bin/adroit list --status accepted --dir corpus-space -o json
```

Five accepted generic decisions (statuses read lowercase — `accepted` —
since the KB-only pin); ADR-0001, ADR-0004, and ADR-0005 carry **stored**
plans:

```text
ADR-0001 Adopt trunk-based development with short-lived branches
ADR-0002 Require ADRs for cross-team architectural decisions
ADR-0003 Pin and audit third-party dependencies in CI
ADR-0004 Maintain a glossary of shared engineering terms in the knowledge base
ADR-0005 Automated Testing
```

## 4. Plan → trigger → run → review → merge → verify

```sh
conduit plan 1                            # stored plan: deterministic, no AI env
REPO_NAME=client-corpus just demo-trigger # reviewer labels issue 1 conduit:run
conduit run --once                        # Scoped -> Coding -> InReview
# reviewer approves + merges PR 2 via the API (in real life: the Gitea UI)
conduit run --once                        # observes PrMerged -> Merged
conduit verify 1 -o json                  # the executable tuesday contract
```

Captured — the plan read was stored (no ollama, no AI env anywhere in the
run):

```text
plan for ADR-0001: stored plan (deterministic read from the ADR document)
planned ADR-0001 as task adr-0001 — issue 1 on gitea como/client-corpus at http://localhost:3210: label it conduit:run to start
```

Captured — one tick to InReview, merge observed on the next:

```text
conduit run: single tick via gitea como/client-corpus at http://localhost:3210 (engine: fake (complete))
adr-0001  InReview  1  conduit/adr-0001/adopt-trunk-based-development-with-short
# PR 2: [ADR-0001] Adopt trunk-based development with short-lived branches
#   labels: adr:ADR-0001, effort:1-super-quick
# reviewer: review HTTP 200, merge HTTP 200
adr-0001  Merged    1  conduit/adr-0001/adopt-trunk-based-development-with-short
```

Captured — `conduit verify 1 -o json`, ALL SIX CHECKS PASS, exit 0:

```json
{
  "checks": [
    {"name": "title_prefix",            "pass": true},
    {"name": "trailer_final_line",      "pass": true},
    {"name": "exactly_one_effort_label","pass": true},
    {"name": "adr_label_present",       "pass": true},
    {"name": "branch_shape",            "pass": true},
    {"name": "never_adr_namespace",     "pass": true}
  ],
  "pass": true,
  "pr": 2,
  "task": "adr-0001"
}
```

(Full per-check `detail` strings omitted here for width; the shape and
semantics are the [demo walkthrough's](./demo.md) section 8.)

## 5. Forge neutrality on the client corpus

```sh
conduit demo-transcript 1 --forge gitea  > t-gitea.jsonl
conduit demo-transcript 1 --forge github > t-github.jsonl
conduit demo-transcript 1 --forge gitlab > t-gitlab.jsonl
diff t-gitea.jsonl t-github.jsonl && diff t-gitea.jsonl t-gitlab.jsonl \
  && echo "FORGE-NEUTRAL: identical"
```

Captured — the 7-line normalized streams are byte-identical three ways
(gitea live, github and gitlab DryRun by construction, ADR-0016):

```text
FORGE-NEUTRAL: identical
b119003e0d6d2809debd259f9f14871e53cb11b61170229a6775d4b75fbba865  t-gitea.jsonl
b119003e0d6d2809debd259f9f14871e53cb11b61170229a6775d4b75fbba865  t-github.jsonl
b119003e0d6d2809debd259f9f14871e53cb11b61170229a6775d4b75fbba865  t-gitlab.jsonl
```

## 6. The harvest rule, and forge down

The merged PR's diff was the FakeEngine's deterministic artifact
(`docs/impl/adr-0001.md`) — demo evidence, not corpus content. The built
corpus repo is disposable by construction (wiped and rebuilt from
llm-wiki's starter content on every run), so there is nothing to harvest
and nothing to protect: work merged on the throwaway forge simply dies with
it. The llm-wiki checkout the content came from is only ever read — seeding
pushes *from* the built copy; nothing ever pushes *to* llm-wiki.

```sh
just forge-down   # container + volume destroyed; llm-wiki untouched
```
