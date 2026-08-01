//! Model-based ("oracle") testing for the adroit hardening blitz.
//!
//! A proptest generates a random sequence of mutating CLI commands and a random
//! **naming scheme** (the one remaining on-disk dimension — the KB decision
//! page is the single profile and flat the single layout, ADR-0020). Each
//! command is run against the **real `adroit` binary** on a throwaway KB space
//! in a `TempDir` (so the full stack — `main.rs` dispatch, templates, the
//! `Store` write path — is exercised exactly as a user would). In parallel a
//! tiny in-memory **oracle** tracks what the corpus *should* contain. After
//! every command we assert a battery of invariants: the on-disk state agrees
//! with the oracle, `adroit check` is clean, and the repo is link-canonical
//! (relink is a no-op).
//!
//! The oracle is a pure *outcome predictor*: it never re-implements adroit's
//! serialization logic. For schemes whose identity isn't deterministic
//! (uuid, or date with dedup) it **reads the assigned identity back** from disk
//! after `new`, then predicts everything else — so the oracle stays small and is
//! unlikely to carry its own bugs.
//!
//! Determinism: `ADROIT_TODAY` pins "today" so the `date` scheme's `YYYYMMDD-`
//! slugs are stable; the oracle runs `date_source=filesystem` to stay git-free.
//!
//! See the book's Hardening & Quality page (docs/src/dev/hardening.md).

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

use adroit::adr::{Adr, Status};
use adroit::config::DateSource;
use adroit::naming::NamingScheme;
use adroit::store::{Store, StoreOptions};
use adroit::view::Severity;

use proptest::prelude::*;

/// Fixed "today" so the date scheme's slugs are deterministic.
const TODAY: &str = "2026-06-04";
/// Fixed review date for the `SetReview` command.
const REVIEW_DATE: &str = "2026-12-31";

/// All five statuses, indexable by a generated index.
const STATUSES: [Status; 5] = [
    Status::Proposed,
    Status::Accepted,
    Status::Rejected,
    Status::Deprecated,
    Status::Superseded,
];

/// Per-test case budget: `PROPTEST_CASES` if set, else `default`.
fn cases(default: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

// ---------------------------------------------------------------------------
// Profile (the naming scheme is the one remaining dimension)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Profile {
    naming: NamingScheme,
}

impl Profile {
    fn store_options(&self) -> StoreOptions {
        StoreOptions {
            review_overdue_days: None,
            date_source: DateSource::Filesystem,
            naming: self.naming,
        }
    }

    fn naming_arg(&self) -> &'static str {
        match self.naming {
            NamingScheme::Sequential => "sequential",
            NamingScheme::Date => "date",
            NamingScheme::Uuid => "uuid",
        }
    }
}

// ---------------------------------------------------------------------------
// Commands (abstract — resolved against current state at apply time)
// ---------------------------------------------------------------------------

/// A generated, abstract mutating command. Index fields are taken modulo the
/// current ADR count at apply time, so a sequence is always valid; behaviour is
/// gated by the active `Profile` (e.g. `Renumber` is a no-op off `sequential`).
#[derive(Debug, Clone)]
enum Op {
    New {
        title: String,
    },
    SetStatus {
        which: usize,
        status: Status,
    },
    Supersede {
        newer: usize,
        older: usize,
    },
    SetReview {
        which: usize,
        clear: bool,
    },
    Renumber {
        which: usize,
    },
    Relink,
    /// `link <src> <--relates-to|--depends-on|--refines> <dst>` — a typed
    /// cross-ADR link. Not modeled in the oracle (the link target/relation
    /// aren't tracked); the invariants (`check` clean + relink-canonical) verify
    /// the link is valid.
    Link {
        src: usize,
        dst: usize,
        rel: usize,
    },
    /// `draft <which>` with the AI fake seam — exercises the AI body-splice write
    /// path across the schemes. Only the *prose* is rewritten; identity, status,
    /// title, supersession, and review stay mechanical, so the oracle's model is
    /// unchanged and its invariants verify the splice keeps the repo valid.
    Draft {
        which: usize,
    },
    /// `import --from-assessment <file>` — bulk-seed proposed ADRs from a generated
    /// assessment. Like `new` it only creates (Proposed) ADRs, so the model reads
    /// the new identities back from disk; the invariants then verify the seeded
    /// repo stays valid. `nonce` keeps practice titles unique so each import lands
    /// fresh (the title dedup is covered in `tests/cli.rs`). `ai` runs the `--ai`
    /// flesh-out pass via the fake seam (only the prose changes, so the model is
    /// unchanged — the invariants verify the splice keeps it valid).
    Import {
        nonce: u64,
        n: usize,
        ai: bool,
    },
    /// `plan <which> --save` with the AI fake seam (ADR-0008) — exercises the
    /// plan-persistence write path across the schemes: the canned plan is
    /// spliced into the document as the marked `## Implementation` section
    /// (replacing the template placeholder, with `--force` once a plan is
    /// already stored). The model tracks `has_plan`; the invariants verify the
    /// stored plan reads back verbatim and the repo stays valid.
    PlanSave {
        which: usize,
    },
}

/// The three typed-link relations `link` accepts.
const LINK_RELS: [&str; 3] = ["--relates-to", "--depends-on", "--refines"];

/// Canned AI body for the `Draft` op's `ADROIT_AI_FAKE` seam. The splice keeps
/// everything before the first `## Context …` and replaces the rest with this.
const FAKE_DRAFT: &str = "## Context and Problem Statement\n\nA fake-drafted context.\n\n\
## Considered Options\n\n1. One\n2. Two\n\n## Decision Outcome\n\nChosen: one, because reasons.\n\n\
### Negative Consequences\n\n- A genuine trade-off.";

/// Canned plan for the `PlanSave` op's `ADROIT_AI_FAKE` seam. Carries its own
/// `## ` sub-heading on purpose: the stored section is end-marker-bracketed, so
/// free-form plan markdown must round-trip verbatim (no truncation at the next
/// heading). No relative ADR links — those would (correctly) be rewritten by
/// relink and break the verbatim read-back assertion.
const FAKE_PLAN: &str = "1. Stand up the schema.\n2. Wire the adapter.\n\n\
## Rollout\n\n- [ ] Flag on in staging.";

// ---------------------------------------------------------------------------
// Oracle
// ---------------------------------------------------------------------------

/// The oracle's view of one ADR — only the fields a command can change. ADRs are
/// keyed by their **addressing token** (`addr`): the number for sequential, the
/// slug for date, the uuid for uuid.
#[derive(Debug, Clone)]
struct ModelAdr {
    addr: String,
    title: String,
    status: Status,
    /// The addr of the superseding ADR, if any.
    superseded_by: Option<String>,
    review_by: Option<String>,
    /// Whether a `plan --save` persisted the canned plan into the document
    /// (ADR-0008). `Draft` resets it — the AI body splice replaces all prose,
    /// the stored plan section included.
    has_plan: bool,
}

struct Harness {
    dir: tempfile::TempDir,
    profile: Profile,
    model: Vec<ModelAdr>,
}

impl Harness {
    fn new(profile: Profile) -> Self {
        let dir = tempfile::tempdir().unwrap();
        // Scaffold the KB space (ADR-0020): adroit never creates the space
        // itself, only the decisions/ dir inside it.
        std::fs::write(dir.path().join("wiki.toml"), "name = \"oracle\"\n").unwrap();
        std::fs::create_dir_all(dir.path().join("wiki").join("decisions")).unwrap();
        Self {
            dir,
            profile,
            model: Vec::new(),
        }
    }

    /// Next sequential number (sequential cells only): max existing addr + 1.
    fn next_number(&self) -> u32 {
        self.model
            .iter()
            .filter_map(|a| a.addr.parse::<u32>().ok())
            .max()
            .unwrap_or(0)
            + 1
    }

    fn find(&self, which: usize) -> Option<usize> {
        if self.model.is_empty() {
            None
        } else {
            Some(which % self.model.len())
        }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_adroit"));
        c.arg("--dir")
            .arg(self.dir.path())
            .args([
                "--naming",
                self.profile.naming_arg(),
                "--date-source",
                "filesystem",
            ])
            .env("ADROIT_TODAY", TODAY)
            .env("EDITOR", "true")
            .env("VISUAL", "true");
        c
    }

    fn run(&self, args: &[&str]) -> Result<(), TestCaseError> {
        let out = self.cmd().args(args).output().expect("spawn adroit");
        prop_assert!(
            out.status.success(),
            "`adroit {}` failed ({}) in {:?}\nstdout: {}\nstderr: {}",
            args.join(" "),
            out.status,
            self.profile,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        Ok(())
    }

    /// Open a read-only store over the current on-disk repo.
    fn store(&self) -> Result<Store, TestCaseError> {
        Store::open_with(self.dir.path(), self.profile.store_options())
            .map_err(|e| TestCaseError::fail(format!("open store: {e}")))
    }

    /// Every ADR currently on disk, paired with its path.
    fn observe(&self) -> Result<Vec<(PathBuf, Adr)>, TestCaseError> {
        self.store()?
            .list_with_paths()
            .map_err(|e| TestCaseError::fail(format!("list_with_paths: {e}")))
    }

    fn apply(&mut self, op: &Op) -> Result<(), TestCaseError> {
        match op {
            Op::New { title } => {
                let before: HashSet<PathBuf> =
                    self.observe()?.into_iter().map(|(p, _)| p).collect();
                self.run(&["new", title, "--no-edit"])?;

                // Read the assigned identity back (robust for uuid/date dedup).
                let after = self.observe()?;
                let news: Vec<&(PathBuf, Adr)> =
                    after.iter().filter(|(p, _)| !before.contains(p)).collect();
                prop_assert_eq!(news.len(), 1, "`new` must create exactly one ADR");
                let adr = &news[0].1;
                self.model.push(ModelAdr {
                    addr: adr.reference().addr(),
                    title: title.clone(),
                    status: Status::Proposed,
                    superseded_by: None,
                    review_by: None,
                    has_plan: false,
                });
            }
            Op::SetStatus { which, status } => {
                let Some(i) = self.find(*which) else {
                    return Ok(());
                };
                let addr = self.model[i].addr.clone();
                self.run(&["set-status", &addr, &status.to_string().to_lowercase()])?;
                self.model[i].status = *status;
                // A status change flips only the frontmatter `status:` field;
                // any `superseded_by:` ref is kept.
            }
            Op::Supersede { newer, older } => {
                if self.model.len() < 2 {
                    return Ok(());
                }
                let a = newer % self.model.len();
                let b = older % self.model.len();
                if a == b {
                    return Ok(());
                }
                let new_addr = self.model[a].addr.clone();
                let old_addr = self.model[b].addr.clone();
                self.run(&["supersede", &new_addr, &old_addr])?;
                self.model[b].status = Status::Superseded;
                self.model[b].superseded_by = Some(new_addr);
            }
            Op::SetReview { which, clear } => {
                let Some(i) = self.find(*which) else {
                    return Ok(());
                };
                let addr = self.model[i].addr.clone();
                if *clear {
                    self.run(&["set-review", &addr, "--clear"])?;
                    self.model[i].review_by = None;
                } else {
                    self.run(&["set-review", &addr, REVIEW_DATE])?;
                    self.model[i].review_by = Some(REVIEW_DATE.to_string());
                }
            }
            Op::Renumber { which } => {
                // Sequential-only — the CLI refuses it for other schemes, so
                // emitting it elsewhere would be a false failure.
                if self.profile.naming != NamingScheme::Sequential {
                    return Ok(());
                }
                let Some(i) = self.find(*which) else {
                    return Ok(());
                };
                let old = self.model[i].addr.clone();
                let new = self.next_number().to_string();
                self.run(&["renumber", &old, &new])?;
                self.model[i].addr = new.clone();
                // renumber retargets every inbound reference to this ADR:
                // `[ADR-old]` body links via relabeling and the frontmatter
                // YAML `superseded_by:` field via the model remap (#8). So a
                // supersession pointer at `old` follows to `new`.
                for a in &mut self.model {
                    if a.superseded_by.as_deref() == Some(old.as_str()) {
                        a.superseded_by = Some(new.clone());
                    }
                }
            }
            Op::Relink => {
                self.run(&["relink"])?;
            }
            Op::Link { src, dst, rel } => {
                // Needs two distinct ADRs.
                if self.model.len() < 2 {
                    return Ok(());
                }
                let a = src % self.model.len();
                let b = dst % self.model.len();
                if a == b {
                    return Ok(());
                }
                let src_addr = self.model[a].addr.clone();
                let dst_addr = self.model[b].addr.clone();
                let flag = LINK_RELS[rel % LINK_RELS.len()];
                self.run(&["link", &src_addr, flag, &dst_addr])?;
                // No model update: the typed link isn't tracked. Correctness is
                // covered by the post-command invariants — `check` stays clean
                // and the repo stays link-canonical.
            }
            Op::Draft { which } => {
                let Some(i) = self.find(*which) else {
                    return Ok(());
                };
                let addr = self.model[i].addr.clone();
                // `ADROIT_AI_FAKE` drives the splice offline; null stdin so the
                // interview reads EOF (empty answers). The fake provider is always
                // compiled, independent of the `ai` feature.
                let out = self
                    .cmd()
                    .args(["draft", &addr, "--no-edit"])
                    .env("ADROIT_AI_FAKE", FAKE_DRAFT)
                    .stdin(std::process::Stdio::null())
                    .output()
                    .expect("spawn adroit");
                prop_assert!(
                    out.status.success(),
                    "`adroit draft {}` failed in {:?}: {}",
                    addr,
                    self.profile,
                    String::from_utf8_lossy(&out.stderr)
                );
                // No model change — draft rewrites only the prose body. A
                // stored plan (ADR-0008) is adroit-managed content the splice
                // preserves, so `has_plan` stands.
            }
            Op::Import { nonce, n, ai } => {
                let before: HashSet<PathBuf> =
                    self.observe()?.into_iter().map(|(p, _)| p).collect();
                // A tiny assessment with `n` uniquely-titled practices in one domain.
                let practices: String = (0..*n)
                    .map(|i| {
                        format!(
                            r#"{{"name":"imported {nonce} {i}","context":"c","value":"v","risk":"r"}}"#
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let assessment = format!(
                    r#"{{"name":"oracle","domains":[{{"name":"alpha","practices":[{practices}]}}]}}"#
                );
                let path = self.dir.path().join("__assessment.json");
                std::fs::write(&path, assessment).expect("write assessment file");
                let path_str = path.to_string_lossy().into_owned();
                // `--ai` runs the flesh-out pass offline via the fake seam.
                let mut cmd = self.cmd();
                cmd.args(["import", "--from-assessment", &path_str]);
                if *ai {
                    cmd.args(["--ai"])
                        .env("ADROIT_AI_FAKE", "A fake fleshed-out ADR body.");
                }
                let out = cmd.output().expect("spawn adroit");
                prop_assert!(
                    out.status.success(),
                    "`adroit import{}` failed in {:?}: {}",
                    if *ai { " --ai" } else { "" },
                    self.profile,
                    String::from_utf8_lossy(&out.stderr)
                );
                // Read every newly-created ADR back from disk (robust to the title
                // dedup), and record each as a Proposed ADR in the oracle.
                for (p, adr) in self.observe()? {
                    if before.contains(&p) {
                        continue;
                    }
                    self.model.push(ModelAdr {
                        addr: adr.reference().addr(),
                        title: adr.title.clone(),
                        status: Status::Proposed,
                        superseded_by: None,
                        review_by: None,
                        has_plan: false,
                    });
                }
            }
            Op::PlanSave { which } => {
                let Some(i) = self.find(*which) else {
                    return Ok(());
                };
                let addr = self.model[i].addr.clone();
                let mut args = vec!["plan", addr.as_str(), "--save"];
                if self.model[i].has_plan {
                    // A stored plan refuses a plain `--save` (cli.rs covers the
                    // refusal); the oracle exercises the explicit overwrite.
                    args.push("--force");
                }
                let out = self
                    .cmd()
                    .args(&args)
                    .env("ADROIT_AI_FAKE", FAKE_PLAN)
                    .stdin(std::process::Stdio::null())
                    .output()
                    .expect("spawn adroit");
                prop_assert!(
                    out.status.success(),
                    "`adroit {}` failed in {:?}: {}",
                    args.join(" "),
                    self.profile,
                    String::from_utf8_lossy(&out.stderr)
                );
                self.model[i].has_plan = true;
            }
        }
        Ok(())
    }

    fn check_invariants(&self) -> Result<(), TestCaseError> {
        let store = self.store()?;
        let entries = store
            .list_with_paths()
            .map_err(|e| TestCaseError::fail(format!("list_with_paths: {e}")))?;

        // (A) The set of ADR identities on disk equals the oracle's.
        let mut disk: Vec<String> = entries.iter().map(|(_, a)| a.reference().addr()).collect();
        disk.sort();
        let mut expected: Vec<String> = self.model.iter().map(|a| a.addr.clone()).collect();
        expected.sort();
        prop_assert_eq!(
            &disk,
            &expected,
            "on-disk ids {:?} != oracle {:?} in {:?}",
            disk,
            expected,
            self.profile
        );

        let by_addr: BTreeMap<String, (&PathBuf, &Adr)> = entries
            .iter()
            .map(|(p, a)| (a.reference().addr(), (p, a)))
            .collect();

        for m in &self.model {
            let (path, adr) = by_addr[&m.addr];

            prop_assert_eq!(adr.status, m.status, "{} status mismatch", &m.addr);

            // (B) Flat is the only layout: every page lives directly in the
            // corpus root — a status change must never have moved it.
            prop_assert_eq!(
                path.parent(),
                Some(store.root()),
                "{} left the decisions dir",
                &m.addr
            );

            prop_assert_eq!(&adr.title, &m.title, "{} title mismatch", &m.addr);

            let disk_sb = adr.superseded_by.as_ref().map(|r| r.addr());
            prop_assert_eq!(
                disk_sb.as_deref(),
                m.superseded_by.as_deref(),
                "{} superseded_by mismatch",
                &m.addr
            );

            let disk_rb = adr.review_by.map(|r| r.to_string());
            prop_assert_eq!(
                disk_rb.as_deref(),
                m.review_by.as_deref(),
                "{} review_by mismatch",
                &m.addr
            );

            // (ADR-0008) A saved plan reads back verbatim from the document —
            // sub-heading included — and no document grows one unasked.
            let disk_plan = adroit::plan::extract(&adr.body);
            let expected_plan = m.has_plan.then_some(FAKE_PLAN);
            prop_assert_eq!(disk_plan, expected_plan, "{} stored-plan mismatch", &m.addr);
        }

        // (D) `adroit check` reports no errors.
        let report = adroit::query::check(&store)
            .map_err(|e| TestCaseError::fail(format!("query::check: {e}")))?;
        let errors: Vec<&str> = report
            .problems
            .iter()
            .filter(|p| p.severity == Severity::Error)
            .map(|p| p.message.as_str())
            .collect();
        prop_assert!(
            errors.is_empty(),
            "check errors in {:?}: {:?}",
            self.profile,
            errors
        );

        // (E) The repo is link-canonical after every command — a relink
        // dry-run rewrites nothing. (Status changes rewrite in place, so no
        // command may leave a stale link behind.)
        let relink = store
            .relink(false)
            .map_err(|e| TestCaseError::fail(format!("relink dry-run: {e}")))?;
        prop_assert_eq!(
            relink.files_changed,
            0,
            "{:?} not link-canonical; relink would rewrite {:?}",
            self.profile,
            relink.changed_files
        );

        Ok(())
    }

    /// Run the read-only verbs against the current (arbitrary) repo state and
    /// assert they don't crash — and that the `-o json` emitters produce
    /// parseable JSON. The verbs are individually tested in `cli.rs`; here they're
    /// stressed on the random states a sequence produces (empty repo,
    /// all-superseded, cyclic links, every scheme).
    fn probe_reads(&self) -> Result<(), TestCaseError> {
        // Whole-repo JSON emitters: must succeed and parse. `check` exits non-zero
        // only on an Error-severity problem, which invariant (D) already rules out.
        for args in [
            ["list", "-o", "json"],
            ["stats", "-o", "json"],
            ["graph", "-o", "json"],
            ["check", "-o", "json"],
        ] {
            let out = self.cmd().args(args).output().expect("spawn adroit");
            prop_assert!(
                out.status.success(),
                "`adroit {}` failed in {:?}: {}",
                args.join(" "),
                self.profile,
                String::from_utf8_lossy(&out.stderr)
            );
            serde_json::from_slice::<serde_json::Value>(&out.stdout).map_err(|e| {
                TestCaseError::fail(format!(
                    "`adroit {}` emitted invalid JSON: {e}",
                    args.join(" ")
                ))
            })?;
        }
        // `publish` (accepted-set export) in dry-run — previews, writes nothing.
        let preview = self.dir.path().join("__publish_preview__");
        let out = self
            .cmd()
            .arg("publish")
            .arg("--out")
            .arg(&preview)
            .arg("--dry-run")
            .output()
            .expect("spawn adroit");
        prop_assert!(
            out.status.success(),
            "`adroit publish --dry-run` failed in {:?}: {}",
            self.profile,
            String::from_utf8_lossy(&out.stderr)
        );
        // Per-ADR read verbs on the first ADR, when there is one.
        if let Some(first) = self.model.first() {
            let id = first.addr.clone();
            for args in [
                vec!["show", id.as_str(), "-o", "json"],
                vec!["status", id.as_str()],
                vec!["related", id.as_str()],
                vec!["dedupe", id.as_str()],
                vec!["search", "a"],
            ] {
                let out = self.cmd().args(&args).output().expect("spawn adroit");
                prop_assert!(
                    out.status.success(),
                    "`adroit {}` failed in {:?}: {}",
                    args.join(" "),
                    self.profile,
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            // AI read verbs via the fake seam (offline) — stressed across the schemes.
            for verb in ["summarize", "plan"] {
                let out = self
                    .cmd()
                    .args([verb, id.as_str()])
                    .env("ADROIT_AI_FAKE", "A fake result paragraph.")
                    .stdin(std::process::Stdio::null())
                    .output()
                    .expect("spawn adroit");
                prop_assert!(
                    out.status.success(),
                    "`adroit {} {}` failed in {:?}: {}",
                    verb,
                    id,
                    self.profile,
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            // `plan -o json` emits a structured envelope — it must succeed and parse.
            let out = self
                .cmd()
                .args(["plan", id.as_str(), "-o", "json"])
                .env("ADROIT_AI_FAKE", "A fake plan paragraph.")
                .stdin(std::process::Stdio::null())
                .output()
                .expect("spawn adroit");
            prop_assert!(
                out.status.success(),
                "`adroit plan -o json` failed in {:?}: {}",
                self.profile,
                String::from_utf8_lossy(&out.stderr)
            );
            serde_json::from_slice::<serde_json::Value>(&out.stdout).map_err(|e| {
                TestCaseError::fail(format!("`adroit plan -o json` emitted invalid JSON: {e}"))
            })?;
            // `ask` (corpus Q&A) needs a non-empty corpus — it errors with "no ADRs
            // to answer from" otherwise, so only probe it once an ADR exists.
            let out = self
                .cmd()
                .args(["ask", "what was decided"])
                .env("ADROIT_AI_FAKE", "A fake answer.")
                .stdin(std::process::Stdio::null())
                .output()
                .expect("spawn adroit");
            prop_assert!(
                out.status.success(),
                "`adroit ask` failed in {:?}: {}",
                self.profile,
                String::from_utf8_lossy(&out.stderr)
            );
            // `lint` exits non-zero on findings (a fresh template has plenty), so
            // don't assert the exit code — just require parseable JSON, i.e. no panic.
            let out = self
                .cmd()
                .args(["lint", id.as_str(), "-o", "json"])
                .output()
                .expect("spawn adroit");
            serde_json::from_slice::<serde_json::Value>(&out.stdout).map_err(|e| {
                TestCaseError::fail(format!(
                    "`adroit lint -o json` not JSON in {:?}: {e}",
                    self.profile
                ))
            })?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

fn arb_title() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-z][a-z0-9]{0,15}").unwrap()
}

fn arb_status() -> impl Strategy<Value = Status> {
    (0usize..5).prop_map(|i| STATUSES[i])
}

fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => arb_title().prop_map(|title| Op::New { title }),
        3 => (any::<usize>(), arb_status())
            .prop_map(|(which, status)| Op::SetStatus { which, status }),
        2 => (any::<usize>(), any::<usize>())
            .prop_map(|(newer, older)| Op::Supersede { newer, older }),
        1 => (any::<usize>(), any::<bool>())
            .prop_map(|(which, clear)| Op::SetReview { which, clear }),
        1 => any::<usize>().prop_map(|which| Op::Renumber { which }),
        1 => Just(Op::Relink),
        2 => (any::<usize>(), any::<usize>(), any::<usize>())
            .prop_map(|(src, dst, rel)| Op::Link { src, dst, rel }),
        2 => any::<usize>().prop_map(|which| Op::Draft { which }),
        1 => (any::<u64>(), 1usize..4, any::<bool>())
            .prop_map(|(nonce, n, ai)| Op::Import { nonce, n, ai }),
        2 => any::<usize>().prop_map(|which| Op::PlanSave { which }),
    ]
}

fn arb_ops() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(arb_op(), 1..16)
}

/// A naming scheme. Sequential (the default) is weighted up.
fn arb_profile() -> impl Strategy<Value = Profile> {
    prop_oneof![
        4 => Just(Profile { naming: NamingScheme::Sequential }),
        2 => Just(Profile { naming: NamingScheme::Date }),
        2 => Just(Profile { naming: NamingScheme::Uuid }),
    ]
}

fn run_cell(profile: Profile, ops: &[Op]) -> Result<(), TestCaseError> {
    let mut h = Harness::new(profile);
    for op in ops {
        h.apply(op)?;
        h.check_invariants()?;
    }
    // Convergence: an explicit full `relink` must leave the repo
    // link-canonical and idempotent, with no loss of state.
    if !h.model.is_empty() {
        h.run(&["relink"])?;
        let store = h.store()?;
        let relink = store
            .relink(false)
            .map_err(|e| TestCaseError::fail(format!("relink convergence: {e}")))?;
        prop_assert_eq!(
            relink.files_changed,
            0,
            "{:?}: relink did not converge to canonical: {:?}",
            h.profile,
            relink.changed_files
        );
        h.check_invariants()?;
    }
    // The read verbs must survive the arbitrary final state without crashing.
    h.probe_reads()?;
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(192), ..ProptestConfig::default() })]

    /// The whole matrix: a random naming scheme × a random command sequence,
    /// with every invariant checked after every command.
    #[test]
    fn oracle_matrix(profile in arb_profile(), ops in arb_ops()) {
        run_cell(profile, &ops)?;
    }
}
