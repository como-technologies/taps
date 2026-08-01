//! Mechanical quality gates over AI-authored assessments.
//!
//! Small local models echo prompt scaffolding as content (iteration-1 run-1
//! authored an assessment literally named "Assessment Name"), duplicate
//! practice names across domains, and cite `--context` artifacts as subject
//! matter. Schema validation cannot catch any of that — these checks are the
//! mechanical normalization layer the iteration-1 learnings call for: every
//! AI-output consumer needs one; prompt phrasing alone is not a contract.
//!
//! The checks are pure functions over the parsed [`Assessment`] so the same
//! gate runs in the authoring retry loop and in `assessments validate`.

use crate::models::assessment::{Assessment, Question};

/// The literal scaffold strings from `src/prompts/generate_structure.md`'s
/// example output. A model echoing any of these back in a load-bearing
/// field produced a degenerate document, not content. A unit test pins each
/// entry to the prompt text so the list cannot drift from the scaffold.
pub const PLACEHOLDER_ECHOES: &[&str] = &[
    "Assessment Name",
    "What this assessment evaluates",
    "Intended outcome",
    "Domain Name",
    "What this domain covers",
    "Benefits of addressing this well",
    "Consequences of ignoring",
    "Practice Name",
    "What this practice is",
    "Specific benefits",
    "Specific consequences",
    "Another Practice",
    "Next Domain",
];

/// Whether one field value is degenerate: empty, an ellipsis stand-in, a
/// bracketed template placeholder, or a verbatim (case-insensitive) echo of
/// a known prompt placeholder.
pub fn is_degenerate_value(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }
    // Ellipsis stand-ins: "...", "…", and dot/ellipsis runs.
    if trimmed.chars().all(|c| c == '.' || c == '…') {
        return true;
    }
    if is_template_placeholder(trimmed) {
        return true;
    }
    PLACEHOLDER_ECHOES
        .iter()
        .any(|p| trimmed.eq_ignore_ascii_case(p))
}

/// Whether `trimmed` is nothing but a bracketed template stand-in:
/// `[Assessment Name]`, `<your domain>`, `{{goal}}`, `{practice}` — a
/// fill-in slot the model failed to fill, in any of the common template
/// styles (fault injection showed these are NOT verbatim scaffold echoes,
/// so the echo list alone misses them). The value must be fully enclosed
/// with no earlier closing delimiter, so prose that merely starts with a
/// bracket ("[Draft] release checklist") or contains bracket pairs
/// ("[a] and [b]") is not flagged.
fn is_template_placeholder(trimmed: &str) -> bool {
    for (open, close) in [("[", "]"), ("<", ">"), ("{{", "}}"), ("{", "}")] {
        if let Some(inner) = trimmed
            .strip_prefix(open)
            .and_then(|rest| rest.strip_suffix(close))
            && !inner.contains(close)
        {
            return true;
        }
    }
    false
}

/// Every load-bearing field of `assessment` whose value is degenerate,
/// as human-readable findings (field path plus offending value).
///
/// Load-bearing fields: assessment `name`/`description`/`goal`, each
/// domain's and practice's `name`/`context`/`value`/`risk`, and each
/// question's `text`.
pub fn degenerate_fields(assessment: &Assessment) -> Vec<String> {
    let mut findings = Vec::new();
    let mut check = |path: String, value: &str| {
        if is_degenerate_value(value) {
            findings.push(format!("{path} is a placeholder echo: {value:?}"));
        }
    };

    check("assessment name".to_string(), &assessment.name);
    check(
        "assessment description".to_string(),
        &assessment.description,
    );
    check("assessment goal".to_string(), &assessment.goal);
    for domain in &assessment.domains {
        for (field, value) in [
            ("name", &domain.name),
            ("context", &domain.context),
            ("value", &domain.value),
            ("risk", &domain.risk),
        ] {
            check(format!("domain '{}' {field}", domain.name), value);
        }
        for practice in &domain.practices {
            for (field, value) in [
                ("name", &practice.name),
                ("context", &practice.context),
                ("value", &practice.value),
                ("risk", &practice.risk),
            ] {
                check(format!("practice '{}' {field}", practice.name), value);
            }
            for question in &practice.questions {
                check(
                    format!("practice '{}' question text", practice.name),
                    &question.text,
                );
            }
        }
    }
    findings
}

/// Degenerate findings for one practice's freshly generated questions — the
/// per-practice variant the question retry loop uses. A question whose
/// `text` is empty, an ellipsis, or a template placeholder ("[Insert
/// question ...]") is a fill-in slot, not a check; it is schema-valid, so
/// only this gate can reject it.
pub fn degenerate_question_fields(questions: &[Question]) -> Vec<String> {
    questions
        .iter()
        .filter(|q| is_degenerate_value(&q.text))
        .map(|q| format!("question text is a placeholder echo: {:?}", q.text))
        .collect()
}

/// A practice name normalized for duplicate detection: lowercased, with
/// whitespace runs collapsed to single spaces and ends trimmed.
pub fn normalized_practice_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Practice names duplicated (after normalization) anywhere in the
/// assessment — across domains or within one — as human-readable findings
/// naming the practice and every domain it appears in.
pub fn duplicate_practice_names(assessment: &Assessment) -> Vec<String> {
    // Insertion-ordered map: normalized name -> (first display name, the
    // domain of every occurrence).
    let mut occurrences: Vec<(String, String, Vec<String>)> = Vec::new();
    for domain in &assessment.domains {
        for practice in &domain.practices {
            let key = normalized_practice_name(&practice.name);
            match occurrences.iter_mut().find(|(k, _, _)| *k == key) {
                Some((_, _, domains)) => domains.push(domain.name.clone()),
                None => occurrences.push((key, practice.name.clone(), vec![domain.name.clone()])),
            }
        }
    }
    occurrences
        .into_iter()
        .filter(|(_, _, domains)| domains.len() > 1)
        .map(|(_, display, domains)| {
            format!(
                "practice '{display}' appears {} times (in domains {})",
                domains.len(),
                domains
                    .iter()
                    .map(|d| format!("'{d}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect()
}

/// Mechanically drop duplicate practices, keeping the first occurrence in
/// document order; domains left without practices are dropped too. Returns
/// what was dropped, for the warning surface. This is the bounded fallback
/// after corrective retries fail — mirroring adroit import's dedupe guard.
pub fn drop_duplicate_practices(assessment: &mut Assessment) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut dropped = Vec::new();
    for domain in &mut assessment.domains {
        let domain_name = domain.name.clone();
        domain.practices.retain(|practice| {
            if seen.insert(normalized_practice_name(&practice.name)) {
                true
            } else {
                dropped.push(format!("'{}' (domain '{domain_name}')", practice.name));
                false
            }
        });
    }
    if !dropped.is_empty() {
        assessment.domains.retain(|d| !d.practices.is_empty());
    }
    dropped
}

/// The tokens a `--context` artifact contributes to the leakage ban list:
/// the artifact's file name, plus — when the content parses as JSON — every
/// object key (at any depth) that looks like data-shape jargon rather than
/// natural language: at least 4 characters and containing an underscore
/// (`per_tenant`, `total_flows`, ...). Plain-word keys (`month`, `errors`)
/// are not banned; they appear naturally in prose.
pub fn forbidden_context_tokens(file_name: &str, content: &str) -> Vec<String> {
    let mut tokens = vec![file_name.to_string()];
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(content) {
        collect_jargon_keys(&value, &mut tokens);
    }
    tokens
}

/// Recursively collect object keys that read as data-shape jargon
/// (length >= 4, containing an underscore), deduplicated.
fn collect_jargon_keys(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if key.len() >= 4 && key.contains('_') && !out.iter().any(|t| t == key) {
                    out.push(key.clone());
                }
                collect_jargon_keys(child, out);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                collect_jargon_keys(child, out);
            }
        }
        _ => {}
    }
}

/// One finding per (field, token) pair where `value` cites `token`
/// case-insensitively.
fn leaks_in(path: &str, value: &str, tokens: &[String], out: &mut Vec<String>) {
    let lower = value.to_lowercase();
    for token in tokens {
        if lower.contains(&token.to_lowercase()) {
            out.push(format!("{path} cites the context artifact token '{token}'"));
        }
    }
}

/// Leakage findings for one question's text and optional fields.
fn leaks_in_question(path: &str, question: &Question, tokens: &[String], out: &mut Vec<String>) {
    leaks_in(&format!("{path} text"), &question.text, tokens, out);
    for (field, value) in [
        ("guidance", &question.guidance),
        ("evidence", &question.evidence),
        ("remediation", &question.remediation),
    ] {
        if let Some(value) = value {
            leaks_in(&format!("{path} {field}"), value, tokens, out);
        }
    }
}

/// Every text field of `assessment` that cites a banned context token,
/// case-insensitively, as human-readable findings naming field and token.
///
/// Covers EVERY authored string the artifact serializes — required
/// structure fields, the optional enrichment fields the schema accepts even
/// though the prompts never ask for them (domain/practice `terminology`,
/// practice `guidance`/`roles`/`effort` — fault injection proved a leak can
/// ride in on those), and question text/guidance/evidence/remediation.
pub fn leaky_assessment_fields(assessment: &Assessment, tokens: &[String]) -> Vec<String> {
    let mut findings = Vec::new();
    if tokens.is_empty() {
        return findings;
    }
    for (field, value) in [
        ("name", &assessment.name),
        ("description", &assessment.description),
        ("goal", &assessment.goal),
    ] {
        leaks_in(&format!("assessment {field}"), value, tokens, &mut findings);
    }
    for domain in &assessment.domains {
        for (field, value) in [
            ("name", &domain.name),
            ("context", &domain.context),
            ("value", &domain.value),
            ("risk", &domain.risk),
        ] {
            leaks_in(
                &format!("domain '{}' {field}", domain.name),
                value,
                tokens,
                &mut findings,
            );
        }
        if let Some(terminology) = &domain.terminology {
            leaks_in(
                &format!("domain '{}' terminology", domain.name),
                terminology,
                tokens,
                &mut findings,
            );
        }
        for practice in &domain.practices {
            for (field, value) in [
                ("name", &practice.name),
                ("context", &practice.context),
                ("value", &practice.value),
                ("risk", &practice.risk),
            ] {
                leaks_in(
                    &format!("practice '{}' {field}", practice.name),
                    value,
                    tokens,
                    &mut findings,
                );
            }
            for (field, value) in [
                ("guidance", &practice.guidance),
                ("terminology", &practice.terminology),
            ] {
                if let Some(value) = value {
                    leaks_in(
                        &format!("practice '{}' {field}", practice.name),
                        value,
                        tokens,
                        &mut findings,
                    );
                }
            }
            for question in &practice.questions {
                leaks_in_question(
                    &format!("practice '{}' question", practice.name),
                    question,
                    tokens,
                    &mut findings,
                );
            }
        }
    }
    findings
}

/// Leakage findings for one practice's freshly generated questions —
/// the per-practice variant the question retry loop uses.
pub fn leaky_question_fields(questions: &[Question], tokens: &[String]) -> Vec<String> {
    let mut findings = Vec::new();
    if tokens.is_empty() {
        return findings;
    }
    for question in questions {
        leaks_in_question(
            &format!("question {:?}", question.text),
            question,
            tokens,
            &mut findings,
        );
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::assessment::{Domain, Practice};

    /// The gate's placeholder list must be the prompt's actual scaffold —
    /// every entry appears verbatim in generate_structure.md, so the list
    /// cannot drift from what the model is shown.
    #[test]
    fn every_placeholder_echo_is_a_verbatim_prompt_scaffold_string() {
        let prompt = include_str!("../prompts/generate_structure.md");
        for placeholder in PLACEHOLDER_ECHOES {
            assert!(
                prompt.contains(placeholder),
                "placeholder {placeholder:?} is not in generate_structure.md — \
                 the gate list drifted from the prompt scaffold"
            );
        }
    }

    #[test]
    fn placeholder_echoes_are_degenerate_case_insensitively() {
        assert!(is_degenerate_value("Assessment Name"));
        assert!(is_degenerate_value("assessment name"));
        assert!(is_degenerate_value("  Intended outcome  "));
        assert!(is_degenerate_value("What this assessment evaluates"));
    }

    #[test]
    fn empty_and_ellipsis_values_are_degenerate() {
        assert!(is_degenerate_value(""));
        assert!(is_degenerate_value("   "));
        assert!(is_degenerate_value("..."));
        assert!(is_degenerate_value("…"));
        assert!(is_degenerate_value(" ... "));
    }

    /// Novel bracket placeholders (fault-injection finding): template
    /// stand-ins are degenerate in every common style, but prose that
    /// merely contains brackets is not.
    #[test]
    fn bracket_template_placeholders_are_degenerate() {
        for placeholder in [
            "[Assessment Name]",
            "[Insert question about the release process]",
            "<your domain>",
            "<describe what this assessment evaluates>",
            "{{goal}}",
            "{practice name}",
            "  [Intended outcome]  ",
        ] {
            assert!(
                is_degenerate_value(placeholder),
                "{placeholder:?} is a template placeholder"
            );
        }
        for prose in [
            "[Draft] release checklist",
            "[a] and [b] comparisons",
            "{a} and {b} comparisons",
            "Ship in <5 minutes",
            "Risk scored 4 [high]",
        ] {
            assert!(
                !is_degenerate_value(prose),
                "{prose:?} is real content, not a placeholder"
            );
        }
    }

    #[test]
    fn real_content_is_not_degenerate() {
        assert!(!is_degenerate_value("Engineering Maturity"));
        assert!(!is_degenerate_value(
            "How mature the team's engineering practices are"
        ));
        // Containing a placeholder as a substring is fine — only a verbatim
        // echo is degenerate.
        assert!(!is_degenerate_value(
            "Consequences of ignoring pipeline discipline, such as delayed releases"
        ));
    }

    fn assessment(name: &str, description: &str, goal: &str) -> Assessment {
        let mut a = Assessment::new(name.into(), description.into(), goal.into());
        let mut domain = Domain::new(
            "Delivery".into(),
            "How code ships".into(),
            "Predictable releases".into(),
            "Outages".into(),
        );
        domain.practices.push(Practice::new(
            "Continuous Integration".into(),
            "Merging and verifying changes".into(),
            "Fast feedback".into(),
            "Broken main".into(),
        ));
        a.domains.push(domain);
        a
    }

    /// The run-1 capstone artifact's load-bearing fields, verbatim: the
    /// degeneracy gate exists because this document validated cleanly.
    #[test]
    fn the_run1_placeholder_echo_artifact_is_degenerate() {
        let run1 = assessment(
            "Assessment Name",
            "What this assessment evaluates",
            "Intended outcome",
        );
        let findings = degenerate_fields(&run1);
        assert_eq!(findings.len(), 3, "{findings:?}");
        assert!(findings[0].contains("name") && findings[0].contains("Assessment Name"));
        assert!(findings[1].contains("description"));
        assert!(findings[2].contains("goal"));
    }

    #[test]
    fn degenerate_domain_and_practice_fields_are_reported_with_their_path() {
        let mut a = assessment("Real Name", "Real description", "Real goal");
        a.domains[0].risk = "Consequences of ignoring".into();
        a.domains[0].practices[0].name = "Practice Name".into();
        a.domains[0].practices[0].value = "...".into();

        let findings = degenerate_fields(&a);

        assert_eq!(findings.len(), 3, "{findings:?}");
        assert!(
            findings[0].contains("Delivery") && findings[0].contains("risk"),
            "domain finding must locate the field: {findings:?}"
        );
        assert!(
            findings[1].contains("Practice Name"),
            "practice finding must show the echo: {findings:?}"
        );
        assert!(
            findings[2].contains("value"),
            "ellipsis finding must locate the field: {findings:?}"
        );
    }

    /// Question text is load-bearing too (fault-injection finding): a
    /// schema-valid "[Insert question ...]" must be flagged, both by the
    /// per-practice variant the retry loop uses and by the whole-assessment
    /// gate `validate` applies.
    #[test]
    fn placeholder_question_text_is_degenerate() {
        let questions = vec![
            Question::new("[Insert question about the release process]".to_string()),
            Question::new("Is CI green on every merge?".to_string()),
        ];
        let findings = degenerate_question_fields(&questions);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("[Insert question"));

        let mut a = assessment("Real Name", "Real description", "Real goal");
        a.domains[0].practices[0]
            .questions
            .push(Question::new("...".to_string()));
        let findings = degenerate_fields(&a);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].contains("Continuous Integration") && findings[0].contains("question text"),
            "finding must locate the question: {findings:?}"
        );
    }

    #[test]
    fn a_healthy_assessment_has_no_degenerate_fields() {
        let a = assessment(
            "Engineering Maturity",
            "How mature the practices are",
            "Find the gaps",
        );
        assert!(degenerate_fields(&a).is_empty());
    }

    // ===== practice dedupe (M2) =====

    #[test]
    fn practice_names_normalize_case_and_whitespace() {
        assert_eq!(
            normalized_practice_name("Learning from Failure"),
            normalized_practice_name("  learning  FROM\tfailure ")
        );
        assert_ne!(
            normalized_practice_name("Learning from Failure"),
            normalized_practice_name("Learning from Success")
        );
    }

    fn domain_with(name: &str, practice_names: &[&str]) -> Domain {
        let mut d = Domain::new(
            name.into(),
            format!("{name} context"),
            format!("{name} value"),
            format!("{name} risk"),
        );
        for p in practice_names {
            d.practices.push(Practice::new(
                (*p).into(),
                format!("{p} context"),
                format!("{p} value"),
                format!("{p} risk"),
            ));
        }
        d
    }

    /// The run-1 wart: "Learning from Failure" authored into both Testing
    /// and Operations. Detection must catch it case/whitespace-insensitively
    /// and name both domains so the corrective feedback is actionable.
    #[test]
    fn cross_domain_duplicate_practices_are_detected_and_located() {
        let mut a = assessment("A", "d", "g");
        a.domains = vec![
            domain_with("Testing", &["Learning from Failure", "Test Automation"]),
            domain_with("Operations", &["learning  from failure"]),
        ];

        let findings = duplicate_practice_names(&a);

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("Learning from Failure"));
        assert!(findings[0].contains("Testing") && findings[0].contains("Operations"));
    }

    #[test]
    fn unique_practices_yield_no_duplicate_findings() {
        let mut a = assessment("A", "d", "g");
        a.domains = vec![
            domain_with("Testing", &["Test Automation"]),
            domain_with("Operations", &["Incident Response"]),
        ];
        assert!(duplicate_practice_names(&a).is_empty());
    }

    #[test]
    fn drop_keeps_first_occurrence_and_drops_emptied_domains() {
        let mut a = assessment("A", "d", "g");
        a.domains = vec![
            domain_with("Testing", &["Learning from Failure", "Test Automation"]),
            domain_with("Operations", &["Learning From Failure"]),
        ];

        let dropped = drop_duplicate_practices(&mut a);

        assert_eq!(dropped.len(), 1, "{dropped:?}");
        assert!(dropped[0].contains("Learning From Failure"));
        assert!(dropped[0].contains("Operations"));
        assert_eq!(a.practice_count(), 2, "first occurrences survive");
        assert_eq!(
            a.domain_count(),
            1,
            "the emptied Operations domain is dropped"
        );
        assert_eq!(a.domains[0].name, "Testing");
        assert_eq!(a.domains[0].practices[0].name, "Learning from Failure");
    }

    #[test]
    fn drop_is_a_no_op_on_unique_practices() {
        let mut a = assessment("A", "d", "g");
        a.domains = vec![
            domain_with("Testing", &["Test Automation"]),
            domain_with("Operations", &["Incident Response"]),
        ];

        assert!(drop_duplicate_practices(&mut a).is_empty());
        assert_eq!(a.practice_count(), 2);
        assert_eq!(a.domain_count(), 2);
    }

    // ===== context leakage (M3) =====

    /// The shape of run-1's pulse-report.json, abridged: snake_case keys at
    /// several depths are data-shape jargon and get banned; plain-word keys
    /// and short keys do not (they appear naturally in prose).
    const PULSE_LIKE_JSON: &str = r#"{
        "schema": "pulse.measure-report/v1",
        "seed": 42,
        "total_flows": 10,
        "errors": [],
        "per_tenant": [
            {"tenant_name": "iteration-retro", "total_flows": 10}
        ],
        "batches": [
            {"aggregation": {"average_score": 4.2, "segments": []}}
        ]
    }"#;

    #[test]
    fn forbidden_tokens_are_the_filename_plus_snake_case_json_keys() {
        let tokens = forbidden_context_tokens("pulse-report.json", PULSE_LIKE_JSON);

        for expected in [
            "pulse-report.json",
            "total_flows",
            "per_tenant",
            "tenant_name",
            "average_score",
        ] {
            assert!(
                tokens.iter().any(|t| t == expected),
                "{expected:?} must be banned: {tokens:?}"
            );
        }
        for natural in [
            "schema",
            "seed",
            "errors",
            "batches",
            "aggregation",
            "segments",
        ] {
            assert!(
                !tokens.iter().any(|t| t == natural),
                "plain-word key {natural:?} must NOT be banned: {tokens:?}"
            );
        }
        // No duplicates even though total_flows appears at two depths.
        let flows = tokens.iter().filter(|t| *t == "total_flows").count();
        assert_eq!(flows, 1, "{tokens:?}");
    }

    #[test]
    fn non_json_context_bans_only_its_filename() {
        let tokens = forbidden_context_tokens("notes.md", "# Notes\nsome prose with under_scores");
        assert_eq!(tokens, vec!["notes.md".to_string()]);
    }

    fn tokens(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// The run-1 leak, verbatim: question guidance saying "Check the
    /// 'pulse-report.json' file under 'per_tenant' to see the total_flows
    /// and successful values" — the context artifact cited as the subject.
    #[test]
    fn the_run1_leaky_guidance_is_detected_per_token() {
        let mut a = assessment("A", "d", "g");
        let mut q = Question::new("Are all flows succeeding?".to_string());
        q.guidance = Some(
            "Check the 'pulse-report.json' file under 'per_tenant' to see \
             the total_flows and successful values"
                .to_string(),
        );
        a.domains[0].practices[0].questions.push(q);

        let banned = tokens(&["pulse-report.json", "per_tenant", "total_flows"]);
        let findings = leaky_assessment_fields(&a, &banned);

        assert_eq!(findings.len(), 3, "{findings:?}");
        assert!(findings.iter().any(|f| f.contains("pulse-report.json")));
        assert!(findings.iter().any(|f| f.contains("per_tenant")));
        assert!(
            findings[0].contains("guidance"),
            "finding must locate the field: {findings:?}"
        );
    }

    #[test]
    fn leakage_matching_is_case_insensitive_and_covers_structure_fields() {
        let mut a = assessment("A", "d", "g");
        a.domains[0].practices[0].context = "Tracks the PER_TENANT rollup".to_string();

        let findings = leaky_assessment_fields(&a, &tokens(&["per_tenant"]));

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("Continuous Integration"));
    }

    /// Optional enrichment fields (fault-injection finding): a leak can
    /// ride in on practice guidance/roles/effort/terminology or domain
    /// terminology — fields the prompts never ask for but the schema
    /// accepts and the final YAML serializes.
    #[test]
    fn leaks_in_optional_enrichment_fields_are_detected() {
        let mut a = assessment("A", "d", "g");
        a.domains[0].terminology = Some("per_tenant stages".to_string());
        let practice = &mut a.domains[0].practices[0];
        practice.guidance = Some("Adopt the per_tenant rollup from pulse-report.json".to_string());
        practice.terminology = Some("Control (see pulse-report.json)".to_string());

        let findings = leaky_assessment_fields(&a, &tokens(&["per_tenant", "pulse-report.json"]));

        for expected in [
            "domain 'Delivery' terminology",
            "practice 'Continuous Integration' guidance",
            "practice 'Continuous Integration' terminology",
        ] {
            assert!(
                findings.iter().any(|f| f.contains(expected)),
                "missing finding for {expected:?}: {findings:?}"
            );
        }
        // guidance leaks both tokens; the other two leak one each.
        // (roles/effort are question-level fields in this model — see
        // leaky_question_fields_checks_all_optional_fields.)
        assert_eq!(findings.len(), 4, "{findings:?}");
    }

    #[test]
    fn clean_content_has_no_leak_findings() {
        let a = assessment("A", "d", "g");
        assert!(
            leaky_assessment_fields(&a, &tokens(&["per_tenant", "pulse-report.json"])).is_empty()
        );
        assert!(leaky_assessment_fields(&a, &[]).is_empty());
    }

    #[test]
    fn leaky_question_fields_checks_all_optional_fields() {
        let mut q = Question::new("Is the report reviewed monthly?".to_string());
        q.evidence = Some("A row in adr_totals".to_string());
        let qs = vec![q, Question::new("Is CI green?".to_string())];

        let findings = leaky_question_fields(&qs, &tokens(&["adr_totals"]));

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("adr_totals"));
        assert!(findings[0].contains("evidence"));
    }
}
