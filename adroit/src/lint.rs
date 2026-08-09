//! `adroit lint`: authoring-quality checks on a single decision's prose —
//! read-only, and distinct from `check` (which validates *corpus* integrity).
//!
//! The mechanical checks catch the ways a draft is obviously unfinished:
//! sections left as nothing but their italic `_…_` prompt, no honest
//! negative consequences, only one option considered, duplicated section
//! skeletons, bracket placeholders. Deterministic by design — no provider —
//! so `lint` is usable in CI. The regressions each rule guards were observed
//! in real model-authored corpora (see the per-rule comments); the rules
//! crossed the greenfield boundary with their tests.

use serde::Serialize;

/// Finding weight, shared with `check`: an `Error` gates the exit code, a
/// `Warning` advises (visible, but a CI gate stays green).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema, strum::Display)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

/// One authoring-quality finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct LintFinding {
    pub severity: Severity,
    pub message: String,
}

/// Run the mechanical authoring checks over a decision body, returning
/// findings (empty = clean). Pure and deterministic — no network.
pub fn lint(body: &str) -> Vec<LintFinding> {
    let mut out = Vec::new();

    // 1. Sections left as nothing but their italic `_…_` prompt → unfilled.
    //    Template-agnostic: any section whose only content is a shipped
    //    prompt is the author's to write.
    for (heading, content) in sections(body) {
        if prompt_only(&content) {
            let name = heading.trim_start_matches('#').trim();
            out.push(err(format!(
                "`{name}` still holds only its prompt — replace it with real content"
            )));
        }
    }

    // 2. Honest negative consequences (people skip these). A prompt-only
    //    section is already caught above, so only flag a missing or genuinely
    //    empty one. Depth-tolerant: MADR nests `### Negative Consequences`
    //    under the Decision Outcome, but models (and humans) routinely record
    //    it at `##` — depth is shape, not substance.
    match section(body, "### Negative Consequences")
        .or_else(|| section(body, "## Negative Consequences"))
    {
        None => out.push(err(
            "no `Negative Consequences` section — document the trade-offs honestly".into(),
        )),
        Some(c) if c.trim().is_empty() => out.push(err(
            "`Negative Consequences` is empty — every decision has downsides; name them".into(),
        )),
        _ => {}
    }

    // 3. More than one option considered (record the alternatives you
    //    rejected). Skip while the section is still the prompt — that's (1).
    if let Some(opts) = section(body, "## Considered Options")
        && !prompt_only(&opts)
        && list_items(&opts) + option_headings(&opts) < 2
    {
        out.push(err(
            "fewer than two options under `## Considered Options` — record the alternatives \
             you weighed and rejected"
                .into(),
        ));
    }

    // 4. Repeated top-level sections (observed: a model echoed the seed
    //    skeleton, duplicating `## Status` / `## Stakeholders`). A Warning —
    //    a duplicate reads as an echo/merge artifact to clean up, not an
    //    unfinished draft.
    for (name, count) in repeated_top_level_sections(body) {
        out.push(warn(format!(
            "`## {name}` appears {count} times — duplicated top-level section \
             (often a model echo of the template); keep one"
        )));
    }

    // 5. Whole-line bracket placeholders (observed: a model closed a body
    //    with "[Insert implementation plan or other details as needed]" — a
    //    NOVEL placeholder no template contained, so the prompt check (1)
    //    was silent). A Warning: filler to delete or replace. Fenced code is
    //    exempt — an example config legitimately shows `[insert API key]`.
    let mut in_fence = false;
    for line in body.lines() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence && bracket_placeholder(line) {
            out.push(warn(format!(
                "`{}` is a bracket placeholder — model-shaped filler, not content; \
                 replace it or delete the line",
                line.trim()
            )));
        }
    }

    out
}

/// Openers that mark a whole-line `[…]` span as an unfilled placeholder.
/// Curated from observed model output plus the classic template-filler
/// shapes — matched case-insensitively on a word boundary, so `[insertion
/// order matters]` never matches `insert`. A curated list over a clever
/// heuristic: novel entries get added when observed, and legitimate prose is
/// never guessed at.
const PLACEHOLDER_OPENERS: [&str; 23] = [
    // imperative template-filler verbs
    "insert",
    "add",
    "describe",
    "list",
    "enter",
    "provide",
    "specify",
    "replace",
    "fill",
    "include",
    "write",
    "outline",
    "summarize",
    "attach",
    // possessive / nominal placeholder shapes
    "your",
    "name of",
    // classic unfilled-value tokens
    "to be",
    "todo",
    "tbd",
    "tba",
    "fixme",
    "placeholder",
    "optional",
];

/// True if `line` is nothing but a **bracket-placeholder span** — a whole
/// line (optionally behind a list marker) of the form `[Insert …]` /
/// `[Your Name]` / `[TBD]`: model-shaped filler, not content.
///
/// Conservative by design; legitimate bracket constructs never match:
/// - links / images / reference definitions / footnotes continue past the
///   closing `]`, so the whole-line requirement excludes them;
/// - checkboxes (`[ ]`, `[x]`) and citations (`[1]`, `[^1]`) have empty or
///   single-token inner text that's not in the curated opener list — as do
///   TOML-style `[section]` lines;
/// - the opener must end on a word boundary (end / space / `:`);
/// - a 4-space- or tab-indented line is an indented code block, never
///   flagged (callers additionally skip fenced code).
pub fn bracket_placeholder(line: &str) -> bool {
    if line.starts_with("    ") || line.starts_with('\t') {
        return false; // indented code block
    }
    let t = strip_list_marker(line.trim());
    let Some(inner) = t.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) else {
        return false;
    };
    if inner.contains('[') || inner.contains(']') {
        return false; // composite construct (reference link, nested spans)
    }
    let inner = inner.trim().to_lowercase();
    PLACEHOLDER_OPENERS.iter().any(|o| {
        inner == *o
            || inner
                .strip_prefix(o)
                .is_some_and(|rest| rest.starts_with([' ', ':']))
    })
}

/// Strip an optional leading list marker (`- `, `* `, `N. `) from a trimmed
/// line, returning the rest (shared by the prompt and placeholder detectors).
fn strip_list_marker(t: &str) -> &str {
    t.strip_prefix("- ")
        .or_else(|| t.strip_prefix("* "))
        .or_else(|| {
            t.split_once(". ")
                .filter(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                .map(|(_, rest)| rest)
        })
        .unwrap_or(t)
        .trim()
}

/// Top-level (`## `) section names appearing more than once, with their
/// counts, in first-appearance order. Case-insensitive on the heading text.
fn repeated_top_level_sections(body: &str) -> Vec<(String, usize)> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    for line in body.lines() {
        let t = line.trim_start();
        if let Some(name) = t.strip_prefix("## ") {
            let name = name.trim();
            match seen.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
                Some((_, c)) => *c += 1,
                None => seen.push((name.to_string(), 1)),
            }
        }
    }
    seen.retain(|(_, c)| *c > 1);
    seen
}

/// True if `line` is an italic authoring prompt — `_…_` with non-empty inner
/// text — after stripping an optional leading list marker.
fn is_prompt_line(line: &str) -> bool {
    let t = strip_list_marker(line.trim());
    t.len() >= 2
        && t.starts_with('_')
        && t.ends_with('_')
        && !t[1..t.len() - 1].trim_matches('_').trim().is_empty()
}

/// True if a section's `content` is nothing but its prompt: at least one
/// prompt line and no other (non-blank) content. Empty sections are *not*
/// prompt-only. Shared with `crate::plan`, which treats a prompt-only
/// `## Implementation` section as a replaceable template placeholder.
pub(crate) fn prompt_only(content: &str) -> bool {
    let mut saw_prompt = false;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if is_prompt_line(line) {
            saw_prompt = true;
        } else {
            return false;
        }
    }
    saw_prompt
}

/// Split `body` into `(heading_line, content)` pairs — each heading's text
/// runs up to the next heading of any level. Lines before the first heading
/// are dropped (there's no section to attribute them to).
fn sections(body: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in body.lines() {
        if line.trim_start().starts_with('#') {
            out.push((line.trim_start().to_string(), String::new()));
        } else if let Some((_, content)) = out.last_mut() {
            content.push_str(line);
            content.push('\n');
        }
    }
    out
}

fn err(message: String) -> LintFinding {
    LintFinding {
        severity: Severity::Error,
        message,
    }
}

fn warn(message: String) -> LintFinding {
    LintFinding {
        severity: Severity::Warning,
        message,
    }
}

/// The text under `heading`, up to the next heading of the same-or-higher
/// level. `None` if the heading is absent. Case-insensitive on the heading
/// text — `## Negative consequences` is `## Negative Consequences`; case is
/// shape, not substance, like depth (the Step 4 walk's first real corpus
/// failed all four decisions on sentence-case headings).
fn section(body: &str, heading: &str) -> Option<String> {
    let level = heading.bytes().take_while(|b| *b == b'#').count();
    let mut lines = body.lines();
    lines
        .by_ref()
        .find(|l| l.trim().eq_ignore_ascii_case(heading))?;
    let mut content = String::new();
    for line in lines {
        let t = line.trim_start();
        if t.starts_with('#') && t.bytes().take_while(|b| *b == b'#').count() <= level {
            break;
        }
        content.push_str(line);
        content.push('\n');
    }
    Some(content)
}

/// Count `### …` sub-headings in a block — MADR's long form (and most
/// models) record each considered option as its own `###` heading rather
/// than a list item, and both styles record an option.
fn option_headings(block: &str) -> usize {
    block
        .lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with("### "))
        .count()
}

/// Count markdown list items (`- …` or `N. …`) in a block.
fn list_items(block: &str) -> usize {
    block
        .lines()
        .map(str::trim_start)
        .filter(|l| {
            l.starts_with("- ")
                || l.starts_with("* ")
                || l.split_once(". ")
                    .is_some_and(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh MADR-shaped draft whose prose sections are still their
    /// italic authoring prompts — the shape a template ships.
    const FRESH: &str = "## Status\n\nProposed\n\n\
        ## Context and Problem Statement\n\n_Describe the context and the problem._\n\n\
        ## Considered Options\n\n- _Option 1_\n- _Option 2_\n\n\
        ## Decision Outcome\n\n_Chosen option and why._\n\n\
        ### Negative Consequences\n\n- _The honest downsides._\n";

    const FINISHED: &str = "# ADR-0001: Adopt feature flags\n\n## Status\n\nProposed\n\n\
        ## Context and Problem Statement\n\nWe ship risky changes and want to decouple deploy from release.\n\n\
        ## Considered Options\n\n1. Feature flags\n2. Long-lived branches\n\n\
        ## Decision Outcome\n\nChosen option: feature flags, because they decouple deploy from release.\n\n\
        ### Negative Consequences\n\n- Flag debt accumulates and needs periodic cleanup.\n";

    #[test]
    fn fresh_template_is_flagged_unfinished() {
        let f = lint(FRESH);
        assert!(!f.is_empty());
        assert!(
            f.iter()
                .any(|x| x.message.contains("still holds only its prompt")),
            "should flag sections left as their prompt, got: {f:?}"
        );
        assert!(
            f.iter()
                .any(|x| x.message.contains("Context and Problem Statement")),
            "context prompt should be flagged, got: {f:?}"
        );
        // The pre-existing mechanical checks gate CI: prompt-echo findings
        // are errors.
        assert!(f.iter().all(|x| x.severity == Severity::Error), "{f:?}");
    }

    #[test]
    fn prompt_only_detects_list_and_prose_prompts() {
        assert!(is_prompt_line("_a prose prompt_"));
        assert!(is_prompt_line("  - _a bulleted prompt_"));
        assert!(is_prompt_line("1. _a numbered prompt_"));
        assert!(!is_prompt_line("- a real bullet"));
        assert!(!is_prompt_line("real prose"));
        assert!(!is_prompt_line("_emphasis_ inside real prose")); // not the whole line
        assert!(prompt_only("\n_just the prompt_\n"));
        assert!(!prompt_only("\nreal content\n"));
        assert!(!prompt_only("\n")); // empty is not prompt-only
    }

    #[test]
    fn finished_body_is_clean() {
        assert_eq!(lint(FINISHED), Vec::new());
    }

    #[test]
    fn missing_negative_consequences_is_flagged() {
        let body = "## Context and Problem Statement\n\nReal context.\n\n\
            ## Considered Options\n\n1. A real option\n2. Another real option\n\n\
            ## Decision Outcome\n\nWe picked the first one for cost reasons.\n";
        let f = lint(body);
        assert!(
            f.iter()
                .any(|x| x.message.contains("Negative Consequences"))
        );
    }

    #[test]
    fn options_recorded_as_subheadings_are_counted() {
        // MADR's long form (and most models) record each option as its own
        // `###` heading under `## Considered Options` rather than a list
        // item. Two such headings are two recorded options.
        let body = "## Considered Options\n\n### Option 1: Vault\n\nManaged secrets.\n\n\
            ### Option 2: SOPS\n\nIn-repo encryption.\n\n\
            ## Decision Outcome\n\nChosen: Vault, for the obvious reasons.\n\n\
            ### Negative Consequences\n\n- New infrastructure to run.\n";
        assert_eq!(lint(body), Vec::new());
    }

    #[test]
    fn single_option_is_flagged() {
        let body = "## Considered Options\n\n1. The only option\n\n\
            ## Decision Outcome\n\nPicked it for the obvious reasons.\n\n\
            ### Negative Consequences\n\n- A real downside here.\n";
        let f = lint(body);
        assert!(f.iter().any(|x| x.message.contains("two options")));
    }

    #[test]
    fn heading_case_is_shape_not_substance() {
        // The Step 4 walk's first real corpus wrote `## Negative
        // consequences` (sentence case) and lint flagged all four decisions
        // as missing the section. Case is shape, like depth — both
        // spellings are honest documentation.
        let body = "## Context and Problem Statement\n\nReal context.\n\n\
            ## Considered options\n\n1. A\n2. B\n\n\
            ## Decision outcome\n\nChosen: A, for reasons.\n\n\
            ## Negative consequences\n\n- A real downside.\n";
        assert_eq!(lint(body), Vec::new());
    }

    #[test]
    fn negative_consequences_at_h2_is_accepted() {
        // Models record the consequences sections at `##` depth where MADR
        // nests them as `###` — both depths are honest documentation.
        let body = "## Context and Problem Statement\n\nReal context here.\n\n\
            ## Considered Options\n\n1. A real option\n2. Another real option\n\n\
            ## Decision Outcome\n\nChosen: the first one, because reasons.\n\n\
            ## Positive Consequences\n\n* Faster feedback loops.\n\n\
            ## Negative Consequences\n\n* Initial investment required.\n";
        assert_eq!(lint(body), Vec::new());
    }

    #[test]
    fn empty_h2_negative_consequences_is_flagged() {
        // Depth tolerance must not weaken the honesty check.
        let body = "## Considered Options\n\n1. A\n2. B\n\n\
            ## Decision Outcome\n\nChosen: A, for reasons.\n\n\
            ## Negative Consequences\n\n## References\n\n- none\n";
        let f = lint(body);
        assert!(
            f.iter()
                .any(|x| x.message.contains("Negative Consequences") && x.message.contains("empty")),
            "{f:?}"
        );
    }

    #[test]
    fn repeated_top_level_sections_warn() {
        // A duplicated `## Status` / `## Stakeholders` skeleton echo is a
        // Warning finding (visible, but not a CI failure).
        let body = "## Status\n\nProposed\n\n## Stakeholders\n\n- Team\n\n\
            ## Status\nProposed\n\n## Stakeholders\n\n- Team\n\n\
            ## Context and Problem Statement\n\nReal context.\n\n\
            ## Considered Options\n\n1. A\n2. B\n\n\
            ## Decision Outcome\n\nChosen: A, because reasons.\n\n\
            ### Negative Consequences\n\n- A real downside.\n";
        let f = lint(body);
        let warnings: Vec<_> = f
            .iter()
            .filter(|x| x.severity == Severity::Warning)
            .collect();
        assert!(
            warnings.iter().any(|x| x.message.contains("## Status")),
            "{f:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|x| x.message.contains("## Stakeholders")),
            "{f:?}"
        );
        // Nothing else is wrong with this body — every finding is a warning.
        assert!(f.iter().all(|x| x.severity == Severity::Warning), "{f:?}");
    }

    #[test]
    fn bracket_placeholder_lines_warn() {
        let body =
            format!("{FINISHED}\n---\n\n[Insert implementation plan or other details as needed]\n");
        let f = lint(&body);
        assert!(
            f.iter().any(|x| x.severity == Severity::Warning
                && x.message.contains("placeholder")
                && x.message
                    .contains("[Insert implementation plan or other details as needed]")),
            "{f:?}"
        );
        assert!(f.iter().all(|x| x.severity == Severity::Warning), "{f:?}");
    }

    #[test]
    fn bracket_placeholders_in_fenced_code_are_not_flagged() {
        // A fenced example legitimately shows where a value goes.
        let body = format!("{FINISHED}\n```yaml\n[Insert API key here]\n```\n");
        assert_eq!(lint(&body), Vec::new());
    }

    #[test]
    fn bracket_placeholder_detection_is_conservative() {
        for line in [
            "[Insert implementation plan or other details as needed]",
            "[Your Name]",
            "[your name]",
            "[TBD]",
            "[TODO: add the rollout diagram]",
            "[To be determined]",
            "[Describe the rollout]",
            "[Name of the approver]",
            "[Optional: include metrics]",
            "- [Insert step]",
            "* [List the stakeholders]",
            "3. [Add a step here]",
            "  [Fill in the dates]",
        ] {
            assert!(bracket_placeholder(line), "should flag {line:?}");
        }
        for line in [
            "- [ ] a real task",                   // checkbox
            "- [x] done task",                     // checked checkbox
            "[ ]",                                 // bare empty checkbox
            "[x]",                                 // bare checked checkbox
            "[1]",                                 // citation
            "[^1]: a footnote definition",         // footnote
            "[MADR](https://adr.github.io/madr/)", // whole-line link
            "[madr]: https://adr.github.io/madr/", // reference definition
            "[the MADR spec][madr]",               // reference-style link
            "![diagram](./diagram.png)",           // image
            "[dependencies]",                      // TOML section: single token
            "[insertion-order]",                   // single token, curated-word prefix
            "[insertion order matters]",           // word boundary: insertion != insert
            "See [above] for details",             // span is not the whole line
            "    [Insert anything]",               // 4-space indented code
            "\t[Insert anything]",                 // tab-indented code
            "",                                    // empty
        ] {
            assert!(!bracket_placeholder(line), "should keep {line:?}");
        }
    }
}
