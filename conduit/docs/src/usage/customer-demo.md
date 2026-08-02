# The customer demo (the engagement, end to end)

The suite's north-star deliverable: sit a customer down and run, live and
repeatably, a complete fictional engagement — **"Como modernizes a client's
engineering practice, end to end, with humans at every gate."** The client
is fully generic (a small-to-mid-size product team and its decision
corpus, seeded from llm-wiki's starter content); every claim shown is
machine evidence produced in front of the audience; nothing ever leaves
localhost.

The kit lives in `demo/kit/`: one `demo-up`, five beat scripts, one
`demo-down`. Each beat prints its talking point, the exact commands it
runs, and the machine evidence it just produced. The pre-baked rehearsal
(rehearsal 6, 2026-08-01 — the first on the single-workspace layout:
every product resolves in-tree, and every corpus the beats touch is a
per-run KB space per adroit ADR-0020 / conduit ADR-0017) is committed
verbatim under `demo/kit/rehearsals/`; a live-lane re-rehearsal is
pending (owner-run; the prior transcripts are in git history). Every
output quoted on this page is from a committed transcript unless marked
otherwise.

**Design rules** (ADR-0015): a *pre-baked/live split* — every AI lane ships
a pre-authored artifact for the fast path and a `--live` flag that
recomputes it on local ollama; *kit-owns-no-state* — all run state lives in
a per-`demo-up` workdir under gitignored `demo/runs/`, and the kit never
writes to a sibling repo; *evidence-per-beat* — no beat ends on narration.

## Seeing it: where every artifact lives

The kit owns no hidden state: **everything a run produces lives in one
per-run workdir**, and `demo/kit/.current` always points at the active
one. Open a second terminal beside the demo and look at anything, any
time:

```sh
WORK=$(cat demo/kit/.current)     # the active run's home
ls "$WORK"                        # one artifact per hand-off, named for its beat
```

What accumulates there, beat by beat:

| After | You should see |
|---|---|
| `demo-up` | `corpus-space/` — **the KB**: the client corpus seeded as typed pages (`wiki/decisions/`, 16 files, frontmatter as the machine seam); `conduit.toml` wired to it |
| beat 1 | `pulse-report.json` — the prior period's k-anonymous sentiment signal |
| beat 2 | `assessment.yaml` — the schema-validated Assess artifact |
| beat 3 | `prescribe/space/` — the imports, as typed pages in a scratch space; the stored-plan reads hit `corpus-space` |
| beat 4 | issues/PRs on the forge (the beat prints URLs); `verify` re-checks the merged PR 6/6 |
| beat 5 | `tuesday-report.json`, and `corpus-space/wiki/measures/como-YYYY-MM.md` — **the month, priced, landing in the KB beside the decisions it prices** |

To read the KB the way the tools do, at any point:

```sh
.conduit/bin/adroit list --dir "$WORK/corpus-space"          # the decision corpus
head -14 "$WORK/corpus-space"/wiki/decisions/0005-*.md       # one page: frontmatter + prose
cat "$WORK/corpus-space"/wiki/measures/*.md                  # after beat 5
```

Each beat also prints an ` -> inspect:` hint naming exactly this. A
non-default forge port (`FORGE_PORT`) is set **only for `demo-up`** — the
run records it in the workdir and every later step reads the record, so
beats work from any shell with no environment to carry. The
workdir (space included) is disposable — `demo-down` leaves the forge
gone and the workdir behind for inspection; delete it whenever.

## Stand up: `demo/kit/demo-up`

One command from a checkout. It resolves every product in-tree (the
workspace directory beside conduit) and prints where each came
from, builds whatever is missing (conduit, the in-tree adroit, the
`assessments` / `tuesday-report` / `pulse-simulate` binaries), stands up
the throwaway Gitea seeded with the client corpus (the legacy-format repo
of record), creates the per-run workdir — including `corpus-space`, the
per-run KB space adroit's `seed` derives from that repo's
`docs/src/adr` (ADR-0017: repo of record canonical, space derived) — with
the standing label set, pre-warms the local model for the live lanes, and
prints the beat menu:

```text
 demo/kit/beat-1-measure-prior      Measure (prior): pulse's verified-anonymous team signal
 demo/kit/beat-2-assess [--live]    Assess: brief + signals -> schema-valid assessment
 demo/kit/beat-3-prescribe [--live] Prescribe: assessment -> ADRs; accept; stored plan
 demo/kit/beat-4-adopt [--restart]  Adopt: stored plan -> human-gated PR -> merged -> verify 6/6
 demo/kit/beat-5-measure            Measure: tuesday --strict + Adopt<->Measure cross-check
 demo/kit/demo-down                 Tear down: forge destroyed, workdir removed, nothing left
```

Idempotent: re-running keeps a live forge and workdir. Rehearsed: 15s with
sibling binaries already built — that now includes seeding the per-run
corpus-space (the first-ever run additionally pays the cargo builds —
minutes, and that is setup, not a beat).

## Timing

| Step | Rehearsal 4 (pre-baked + KB, 2026-07-28) | Rehearsal 2 (live + restart, 2026-06-12, log in git history) |
|---|---|---|
| demo-up | 15s | 6s |
| beat 1 — measure prior | 4s | 0s |
| beat 2 — assess | 0s | **321s** (`--live`, llama3.2) |
| beat 3 — prescribe | 0s | **296s** (`--live`, llama3.2) |
| beat 4 — adopt | 8s (re-run: 1s) | 5s (`--restart`) |
| beat 5 — measure | 0s | 0s |
| demo-down | 2s | 2s |

Every beat except the two opt-in ollama lanes lands far under the 60s bar.
The live lanes are the slow ones by design — that is what the pre-baked
variants are for. Pre-warm note: `demo-up` loads the model into ollama's
memory, so a live beat pays no cold start; run-2 of the full dogfood loop
measured the same lanes at 355.9s (assess, zero retries) and ~5.2 min
(import `--ai` + `plan --save`), consistent with rehearsal 2. (Clock
footnote: rehearsal 2's WALL-CLOCK lines used the realtime clock, which
WSL2 stepped ~23s against the assessments binary's monotonic internal
timer — both numbers are preserved in that transcript. The kit now times
beats on the monotonic clock (`/proc/uptime`), so the narration and the
binaries' own elapsed marks can no longer disagree.)

## Beat 1 — Measure the prior period (pulse)

**Say.** Before Como prescribes anything, we measure. pulse collects team
sentiment with verified anonymity — k-anonymity suppression is tested on
both sides of the threshold. The report is deterministic by design: same
seed, byte-identical bytes. We prove that, live, by running it twice.

**Run.** `demo/kit/beat-1-measure-prior` — pulse's own `just dogfood`,
twice, then sha-compare.

**The audience sees** (rehearsal 6):

```text
   run 1 sha256: 299d5e3e6e3b4c9acda14d3c3b94c4485bd1061642ab1076b0cf5fa912dfb737
   run 2 sha256: 299d5e3e6e3b4c9acda14d3c3b94c4485bd1061642ab1076b0cf5fa912dfb737
   BYTE-IDENTICAL: yes
   schema pulse.measure-report/v1  seed 42  flows 10/10 passed, 0 failed
   signal: "How confident are you that this iteration's changes improved the portfolio?" -> avg 3.7 (10 unique pseudonyms, suppressed: false)
```

The weakest signal (iteration-pace sustainability, avg 2.7) is exactly the
kind of finding the next beat's assessment takes as context — the loop's
return edge, shown before the loop even starts.

## Beat 2 — Assess (assessments)

**Say.** An assessment that used to take a consultant weeks is authored in
minutes from the client's own brief, on a 3B model running on this laptop —
no cloud, nothing leaves the room. The output is schema-validated YAML
behind mechanical quality gates (degeneracy, dedupe, leakage), and the
client's architect reviews every question before it is asked.

**Run.** `demo/kit/beat-2-assess` — fast path: the kit's pre-baked
assessment (authored 2026-06-12 by this same pipeline, 355.9s, zero
retries) is copied in and **re-validated live**; the audience watches the
gate pass, not a slide. `--live` recomputes the whole thing on ollama
(~5.5 min) with the beat-1 pulse report as `--context` — plus beat 5's
tuesday effort report as a second `--context` once a prior period has
closed in the same workdir (the loop's return edge).

**The audience sees** (rehearsal 6 fast path):

```text
   valid: 'Software Engineering Maturity Assessment' — 4 domains, 8 practices, 96 questions
   exit code: 0
```

Rehearsal 2's `--live` lane authored a fresh 4-domain / 8-practice /
89-question assessment in 321s and passed the same validation — the
pre-baked artifact is a cache, not a fake.

## Beat 3 — Prescribe (adroit + the client corpus)

**Say.** Findings become decisions. adroit ingests the assessment and
seeds one proposed ADR per practice — a governed, machine-readable
decision corpus. The human gate: nothing is prescribed until the client's
architect moves a decision to Accepted. Then the trick that controls AI
cost and risk: each accepted decision carries a **stored** implementation
plan inside the document itself. AI is paid once, at authoring time; every
read after that is deterministic and provider-free.

**Run.** `demo/kit/beat-3-prescribe` — the mechanical import runs live
into a scratch KB space inside the workdir (`prescribe/space`, the same
hand-scaffold the suite gates use; the kit never writes to the llm-wiki
checkout, and the client corpus itself is a generated artifact), an
accept transition runs live, and the stored plan is read twice from the
workdir's seeded `corpus-space` with the AI environment scrubbed.
`--live` adds the ollama flesh-out of all eight ADRs plus `plan --save`
(~5 min). Since the KB-only pin, `-o json` statuses read lowercase
(`accepted`) — the KB decision schema's enum.

**The audience sees** (rehearsal 6):

```text
   seeded 8 proposed ADR(s), 0 skipped (dedupe guard)
   ADR-0001  Automated Testing  [proposed]
   ...
   Updated ADR-0001 status to Accepted (.../prescribe/space/wiki/decisions/0001-automated-testing.md)
   ...
   ADR-0005: stored = true
   read 1 sha256: aaa56e54efbd94d598107d4604c20595d93d903cf1710aa72a2251023a81c19e
   read 2 sha256: aaa56e54efbd94d598107d4604c20595d93d903cf1710aa72a2251023a81c19e
   SHA-IDENTICAL: yes — no AI was configured for either read
```

That sha is the same value the full dogfood run and earlier rehearsals
(git history) recorded for this plan — the stored plan has been byte-stable across machines,
days, runs, **and the path-mode → KB-mode pin bump** (the seed carries the
stored plan into the space verbatim).

## Beat 4 — Adopt (conduit): the flagship

**Say.** The pitch, verbatim from the portfolio's agentic-delivery page:
the human gates aren't a safety disclaimer bolted onto an agent — they're
what you're buying. **You never have to trust an agent; you have to review
a pull request, which your team already knows how to do.** Three gates by
name: the *scope* gate (nothing runs until a reviewer labels the issue
`conduit:run` — you read the plan before any code exists), the *review*
gate (every change arrives as a PR in your own forge), and the *merge*
gate (conduit has no merge method — structurally unrepresentable, and the
actor account cannot even approve its own PRs). And we don't claim
success — we machine-verify it.

**Run.** `demo/kit/beat-4-adopt` — plan (stored, no AI env) → scripted
reviewer labels `conduit:run` → one tick to InReview → reviewer approves
and merges through the API (in real life: the forge UI) → next tick
observes Merged → `verify 5 -o json` → the forge-neutrality transcript
diff, **three-way** since the GitLab adapter landed (ADR-0016): gitea
executes live, github and gitlab are dry-run by construction. `--restart`
inserts the crash sub-beat: `kill -9` mid-Coding, recover, audit the live
forge for duplicates.

**The audience sees** (rehearsal 6; restart evidence from the 2026-06-12
live rehearsal, whose log is in git history):

```text
   plan for ADR-0005: stored plan (deterministic read from the ADR document)
   planned ADR-0005 as task adr-0005 — issue 1 on gitea como/client-corpus ...: label it conduit:run to start
   labeled issue 1 with conduit:run (as reviewer)
   ...
   PR 2: [ADR-0005] Automated Testing
   labels: adr:ADR-0005, effort:1-super-quick
   ...
   review APPROVED: HTTP 200
   merge: HTTP 200
   adr-0005   Merged   1   conduit/adr-0005/automated-testing

   PASS  title_prefix / trailer_final_line / exactly_one_effort_label /
         adr_label_present / branch_shape / never_adr_namespace
   overall: pass=true  pr=2  task=adr-0005

   FORGE-NEUTRAL (N=3): identical (7 lines)
   9cf0b8d8...c7a6e  t-gitea.jsonl
   9cf0b8d8...c7a6e  t-github.jsonl
   9cf0b8d8...c7a6e  t-gitlab.jsonl
```

```text
   state: Coding | pending: RunEngine done=false        <- the kill -9 crash record
   issues carrying the adr-0005 task marker: [1] (want exactly one)
   PRs with head conduit/adr-0005/*:          [2] (want exactly one)
```

Timing note: 5–8s wall-clock for the whole lifecycle including the crash
sub-beat (deterministic FakeEngine — the engine seam is the demo's subject,
not the model). A live-engine encore (real coding agent, ~5.5 min,
producing the corpus's actual glossary page) was proven in the full
dogfood run-2; the kit keeps it out of the default path because its output
is nondeterministic.

## Beat 5 — Measure (tuesday) and the loop closes

**Say.** Adoption you can count: tuesday reads the merged PRs off the
client's forge and attributes measured hours to the decision that caused
them. Strict mode exits nonzero if any merged PR is unaccounted for. Then
the double-entry bookkeeping — conduit's `verify` and tuesday's report are
two independent codebases, and the cross-check asserts they agree on the
same PR, effort, and decision. And the measurement doesn't leave the room
as a JSON file: a second `tuesday-report` run with `--kb` writes the month
as a `measure-report` typed page into the same KB space the decisions live
in — then (when an llm-wiki binary resolves) the kit registers the space,
ingests it, and answers the harness question — *what did this decision
cost?* — from the client's own pages. This report plus the pulse signal
are exactly what the next assessment consumes: the loop closes on camera.

**Run.** `demo/kit/beat-5-measure`. The llm-wiki query close is optional
and skip-with-notice (resolution: env `LLM_WIKI_BIN` → the sibling
`../llm-wiki` release build → PATH; `demo/kit/preflight` reports the
availability as an env fact). Everything else in the beat runs regardless
— the pre-baked path's only hard prerequisite stays docker.

**The audience sees** (rehearsal 6):

```text
   exit code: 0 (strict mode satisfied)
   como 2026-July: 1 allocation(s), 0 unallocated PR(s) [strict requires 0]
   PR 2 "[ADR-0005] Automated Testing" -> ADR-0005 (SuperQuick, 160.0h)
   adr_totals: ADR-0005 = 160.0h
   pr:     conduit=2 tuesday=2
   effort: conduit=effort:1-super-quick tuesday=effort:1-super-quick (SuperQuick)
   adr:    conduit=ADR-0005 tuesday=ADR-0005 (adr_totals: 160.0h)
   CROSS-CHECK PASS: PR 2, effort:1-super-quick, ADR-0005 — Adopt and Measure agree
```

Then the KB lane (rehearsal 6 — the llm-wiki close was armed via the
sibling release build):

```text
   page: .../corpus-space/wiki/measures/como-2026-07.md
   adr_hours:
     ADR-0005: 160.0
   ...
   Ingested: 17 pages, 0 unchanged, 1 assets, 0 warnings, 0 redactions
   ...
   slug:  measures/como-2026-07
   uri:   wiki://client/measures/como-2026-07
   title: Capacity report — como 2026-07
   score: 5.62
```

The search hit for *"what did this decision cost"* is the capacity report
tuesday wrote thirty seconds earlier — 16 seeded decisions plus the month's
measurement, one queryable space, zero persistent state (`demo-down`
removes it with the workdir).

## Tear down: `demo/kit/demo-down`

**Say.** The whole engagement ran on a throwaway forge on this machine.
Nothing was ever pushed anywhere but localhost.

```text
   forge container: gone
   forge volume: gone
   workdir: gone
   remotes touched: none (localhost was the only push target, ever)
```

## Appendix: run it yourself

The customer's engineer can replay everything above from this repo:

```sh
git clone <conduit> && cd conduit
just init                      # toolchain + mdbook (rust, docker, jq required)
demo/kit/preflight             # verify docker is up (+ pull llama3.2 for --live)
demo/kit/demo-up               # resolves, builds, seeds, prints the menu
demo/kit/beat-1-measure-prior
demo/kit/beat-2-assess         # add --live to recompute on your ollama
demo/kit/beat-3-prescribe      # add --live likewise
demo/kit/beat-4-adopt          # add --restart for the crash sub-beat
demo/kit/beat-5-measure
demo/kit/demo-down
```

Every product resolves in-tree — the workspace directory beside conduit —
with an env override (`COMO_<REPO>_DIR`) for pointing a beat at an
out-of-tree checkout, then skip-with-notice (only the client corpus is a
hard requirement — it is built by `demo/client-corpus-build.sh` from
llm-wiki's starter content at
`kit/starter/decisions/`; a beat whose repo did not resolve says so and
names the knobs). The in-tree adroit builds via `just init-adroit`.

**The honest note.** The suite repos are published — adroit, tuesday, pulse,
conduit, portfolio, assessments, and llm-wiki all have a remote `main` — so
on a fresh machine every clone-cache leg resolves remotely with no kit
change. What needs what:

| Leg | Status today |
|---|---|
| client corpus (hard) | built from llm-wiki `kit/starter/decisions/` — published, resolves remotely |
| adroit (`just init-adroit`) | in-tree — built from this workspace |
| pulse, assessments, tuesday (beats 1/2/5) | published — resolve remotely |
| llm-wiki **binary** (only beat 5's KB query close) | optional: `LLM_WIKI_BIN` → sibling release build → PATH; absent = skip-with-notice |
| ollama `llama3.2` (only for `--live`) | local install, any machine — never remote |

Requirements (run `demo/kit/preflight` to check them): docker with its daemon up
(the throwaway forge), the rust toolchain + `just`, `jq`, `curl`; ollama with
`llama3.2` only for the `--live` variants; an llm-wiki binary only for beat 5's
optional KB query close (preflight reports its availability as an env fact).
The pre-baked fast path runs with no model — and no llm-wiki binary —
installed at all.
