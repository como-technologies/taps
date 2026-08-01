//! Body-level markdown surgery plus the **legacy corpus parser**.
//!
//! The one on-disk profile is the KB decision page (ADR-0020; see
//! [`crate::frontmatter`]). What remains here is profile-independent:
//!
//! - `## References` upsert/parse ([`upsert_reference`] / [`parse_references`])
//!   — the forge writes issue/PR URLs into bodies, format-preserving and
//!   idempotent.
//! - [`normalize_lone_cr`], the newline-normalization guard those rewriters
//!   share.
//! - The **legacy corpus parser** ([`parse_markdown`]): reads a pre-KB
//!   MADR-style document (`# ADR-NNNN: Title` H1, `## Status` region,
//!   optional `> State:` banner) into an [`Adr`] whose body is already the KB
//!   page shape (H1 / banner / status region stripped — those fields move to
//!   frontmatter). Used ONLY by `adroit seed`, the one-way bootstrap of a
//!   legacy corpus into a fresh space.

use std::borrow::Cow;

use time::format_description::well_known::Iso8601;

use crate::adr::{Adr, Created, Number, ReviewBy, Status};

/// Errors raised parsing a legacy-corpus document.
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("missing H1 heading `# ADR-NNNN: Title`")]
    MissingHeading,
}

// ---------------------------------------------------------------------------
// Legacy corpus parser (used only by `adroit seed`)
// ---------------------------------------------------------------------------

/// Parse the H1 heading into `(number, title)`.
///
/// Accepts `# ADR-0006: Title`, `# ADR 0006: Title`, and `# 0006. Title`
/// (Nygard-style). Also tolerates a plain `# Title` with no number, in which
/// case `number` is `None` and the whole heading text is the title.
fn parse_heading(line: &str) -> (Option<Number>, String) {
    let h = line.trim_start_matches('#').trim();
    // Strip an optional "ADR" prefix and separators.
    let rest = h
        .strip_prefix("ADR-")
        .or_else(|| h.strip_prefix("ADR "))
        .or_else(|| h.strip_prefix("ADR"))
        .unwrap_or(h)
        .trim_start();
    // Number is the leading run of digits, if any.
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let Ok(n) = digits.parse::<u32>() else {
        // No leading number → the heading itself is the title.
        return (None, h.to_string());
    };
    let after = rest[digits.len()..].trim_start();
    // Title follows a `:` or `.` separator.
    let title = after
        .strip_prefix(':')
        .or_else(|| after.strip_prefix('.'))
        .unwrap_or(after)
        .trim()
        .to_string();
    (Some(Number::new(n)), title)
}

fn is_status_heading(line: &str) -> bool {
    let t = line.trim();
    t.eq_ignore_ascii_case("## Status")
}

fn is_heading(line: &str) -> bool {
    line.trim_start().starts_with('#')
}

fn is_references_heading(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case("## References")
}

/// What the parser extracts from the `## Status` region of a legacy document.
///
/// Supersession is captured as the **raw fragment** after the `Superseded by` /
/// `Supersedes` keyword (the `[label](target)` or bare token). Resolving it to
/// an [`crate::naming::AdrRef`] happens in [`parse_markdown`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct StatusRegion {
    status: Option<Status>,
    supersedes: Option<String>,
    superseded_by: Option<String>,
    review_by: Option<ReviewBy>,
    created_on: Option<time::Date>,
}

/// Parse the whole `## Status` region (the lines between the `## Status` heading
/// and the next heading) for the status word, both supersession directions, an
/// optional `Review by:` line, and an optional `Created:` provenance line.
///
/// Supersession wording supported (tolerant of a `[ADR-NNNN](path)` link or a
/// bare `ADR-NNNN`, and of an optional leading `>` banner marker):
/// - `Superseded by [ADR-NNNN](...)` -> `superseded_by`
/// - `Supersedes [ADR-NNNN](...)` -> `supersedes`
/// - `Review by: YYYY-MM-DD` -> `review_by`
/// - `Created: YYYY-MM-DD` -> `created_on` (decision provenance; ADR-0011)
fn parse_status_region(input: &str) -> StatusRegion {
    let mut region = StatusRegion::default();
    let mut lines = input.lines();
    // Advance to the `## Status` heading.
    for line in lines.by_ref() {
        if is_status_heading(line) {
            break;
        }
    }
    for line in lines {
        if is_heading(line) {
            break; // left the region
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        parse_status_line(trimmed, &mut region);
    }
    region
}

/// Parse a single non-blank line from the status region, filling `region`.
fn parse_status_line(line: &str, region: &mut StatusRegion) {
    // Strip an optional leading banner marker (`>` and bold markers).
    let v = line
        .trim_start_matches('>')
        .trim()
        .trim_start_matches("**")
        .trim();

    if let Some(rest) = strip_prefix_ci(v, "Superseded by") {
        region.status.get_or_insert(Status::Superseded);
        if region.superseded_by.is_none() {
            region.superseded_by = Some(rest.trim().to_string());
        }
        return;
    }
    if let Some(rest) = strip_prefix_ci(v, "Supersedes") {
        if region.supersedes.is_none() {
            region.supersedes = Some(rest.trim().to_string());
        }
        return;
    }
    if let Some(rest) = strip_prefix_ci(v, "Review by:").or_else(|| strip_prefix_ci(v, "Review by"))
    {
        if region.review_by.is_none() {
            let date = rest.trim().trim_start_matches(':').trim();
            region.review_by = date.parse::<ReviewBy>().ok();
        }
        return;
    }
    if let Some(rest) = strip_prefix_ci(v, "Created:").or_else(|| strip_prefix_ci(v, "Created")) {
        if region.created_on.is_none() {
            let date = rest.trim().trim_start_matches(':').trim();
            region.created_on = time::Date::parse(date, &Iso8601::DATE).ok();
        }
        return;
    }
    // A bare status word (e.g. "Accepted", "Proposed") sets the status if we
    // have not already inferred one from a supersession note. Try the whole line
    // first, then its first word — so a qualified status line like
    // "Proposed — implementation approach evolving based on spike" still resolves
    // to Proposed instead of falling through to the default.
    if region.status.is_none() {
        let status = v.parse::<Status>().ok().or_else(|| {
            v.split_whitespace()
                .next()
                .and_then(|word| word.parse::<Status>().ok())
        });
        if let Some(status) = status {
            region.status = Some(status);
        }
    }
}

/// Case-insensitive `strip_prefix`, returning the remainder after `prefix`.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// The KB-page body of a legacy document: everything except the H1 line, a
/// leading `> State:` banner, and the `## Status` region (heading through the
/// last line before the next `##` heading) — those become frontmatter fields
/// on seed.
fn legacy_body(input: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut in_status = false;
    let mut seen_h1 = false;
    let mut content_started = false;
    for line in input.lines() {
        if is_status_heading(line) {
            in_status = true;
            continue;
        }
        if in_status {
            if is_heading(line) {
                in_status = false; // fall through: this heading is body content
            } else {
                continue;
            }
        }
        let t = line.trim_start();
        if !seen_h1 && t.starts_with("# ") {
            seen_h1 = true;
            continue;
        }
        // The banner sits between the H1 and the first content; drop it there
        // (a `> State:` deeper in prose is content and stays).
        if !content_started && t.starts_with("> State:") {
            continue;
        }
        if !line.trim().is_empty() {
            content_started = true;
        }
        kept.push(line);
    }
    let joined = kept.join("\n");
    joined.trim_matches('\n').trim_end().to_string()
}

/// Parse a full legacy (pre-KB, MADR-style) document into an [`Adr`] whose
/// body is already the KB page shape (see [`legacy_body`]). `dir_status` is the
/// status implied by the by_status directory the file lived in; it wins over
/// the `## Status` section when supplied. A legacy `Created:` line maps into
/// the page's `created` (midnight UTC); supersession notes resolve through the
/// sequential naming scheme (`ADR-NNNN`, the shape legacy corpora use).
///
/// Used only by `adroit seed` — live reads go through [`crate::frontmatter`].
pub fn parse_markdown(input: &str, dir_status: Option<Status>) -> Result<Adr, FormatError> {
    let naming = crate::naming::NamingScheme::Sequential;
    let heading = input
        .lines()
        .find(|l| l.trim_start().starts_with("# "))
        .ok_or(FormatError::MissingHeading)?;
    let (number, title) = parse_heading(heading);

    let region = parse_status_region(input);

    let status = dir_status.or(region.status).unwrap_or(Status::Proposed);

    Ok(Adr {
        id: crate::adr::AdrId::new(),
        number,
        slug: None,
        title,
        status,
        created: region
            .created_on
            .map(Created::from_date)
            .unwrap_or_else(Created::now),
        body: legacy_body(input),
        git_sha: None,
        supersedes: region
            .supersedes
            .as_deref()
            .and_then(|f| naming.ref_in_note(f)),
        superseded_by: region
            .superseded_by
            .as_deref()
            .and_then(|f| naming.ref_in_note(f)),
        // Typed relational links did not exist in the legacy profile.
        relates_to: Vec::new(),
        depends_on: Vec::new(),
        refines: Vec::new(),
        review_by: region.review_by,
        extra: serde_yaml_ng::Mapping::new(),
    })
}

// ---------------------------------------------------------------------------
// Body-level rewriters (profile-independent)
// ---------------------------------------------------------------------------

/// Replace a *lone* `\r` (a carriage return not part of `\r\n`) with `\n`, leaving
/// `\r\n` and `\n` untouched. adroit never writes lone-CR files, but an imported or
/// hand-edited one would otherwise defeat the rewriters' newline detection (the
/// lone `\r` fuses with a joined `\n` into `\r\n` on the next pass) and make
/// `upsert_reference` non-idempotent.
/// Returns the input unchanged (borrowed) when there is no lone CR, so a
/// consistent-newline document round-trips byte-for-byte.
fn normalize_lone_cr(s: &str) -> Cow<'_, str> {
    let bytes = s.as_bytes();
    let has_lone_cr = bytes
        .iter()
        .enumerate()
        .any(|(i, &b)| b == b'\r' && bytes.get(i + 1) != Some(&b'\n'));
    if !has_lone_cr {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' && chars.peek() != Some(&'\n') {
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// Upsert a `- <label>: <url>` bullet into the ADR's `## References` section,
/// preserving the rest of the document byte-for-byte. The section is created at
/// the end of the file if absent. **Idempotent per label**: re-running with the
/// same label replaces only that line's URL, and a no-change write is
/// byte-identical — so the forge integration can call it repeatedly.
pub fn upsert_reference(original: &str, label: &str, url: &str) -> String {
    let original = normalize_lone_cr(original);
    let newline = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines: Vec<String> = original.split(newline).map(str::to_string).collect();
    let entry = format!("- {label}: {url}");
    let label_prefix = format!("- {label}:").to_ascii_lowercase();

    match lines.iter().position(|l| is_references_heading(l)) {
        Some(h) => {
            // Section runs from the heading to the next heading (or EOF).
            let end = lines[h + 1..]
                .iter()
                .position(|l| is_heading(l))
                .map_or(lines.len(), |rel| h + 1 + rel);
            let existing = lines[h + 1..end]
                .iter()
                .position(|l| {
                    l.trim_start()
                        .to_ascii_lowercase()
                        .starts_with(&label_prefix)
                })
                .map(|rel| h + 1 + rel);
            match existing {
                Some(idx) => lines[idx] = entry,
                None => {
                    // Append after the section's last non-blank line.
                    let mut at = end;
                    while at > h + 1 && lines[at - 1].trim().is_empty() {
                        at -= 1;
                    }
                    lines.insert(at, entry);
                }
            }
        }
        None => {
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
            lines.push(String::new());
            lines.push("## References".to_string());
            lines.push(String::new());
            lines.push(entry);
        }
    }

    let mut out = lines.join(newline);
    if original.ends_with('\n') && !out.ends_with('\n') {
        out.push_str(newline);
    }
    out
}

/// Parse the `- label: url` bullets from an ADR's `## References` section, in
/// order. The forge integration writes these (issue / pull request URLs) and
/// reads them back on `set-status` / `supersede`.
pub fn parse_references(original: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut in_section = false;
    for line in original.lines() {
        if is_references_heading(line) {
            in_section = true;
            continue;
        }
        if in_section && is_heading(line) {
            break;
        }
        if in_section
            && let Some(rest) = line.trim().strip_prefix("- ")
            && let Some((label, url)) = rest.split_once(':')
        {
            out.push((label.trim().to_string(), url.trim().to_string()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::AdrRef;

    /// Parse a legacy markdown ADR (the shape `seed` bootstraps).
    fn pm(input: &str, dir_status: Option<Status>) -> Adr {
        parse_markdown(input, dir_status).unwrap()
    }

    const SAMPLE: &str = "# ADR-0006: Adopt ADRs as Team Decision Process\n\
\n\
> State: Accepted\n\
\n\
## Status\n\
\n\
Accepted\n\
\n\
## Context and Problem Statement\n\
\n\
We need a consistent way to capture architectural decisions.\n";

    #[test]
    fn parse_heading_adr_dash() {
        let (n, t) = parse_heading("# ADR-0006: Adopt ADRs as Team Decision Process");
        assert_eq!(n, Some(Number::new(6)));
        assert_eq!(t, "Adopt ADRs as Team Decision Process");
    }

    #[test]
    fn parse_heading_nygard_dot() {
        let (n, t) = parse_heading("# 0042. Use PostgreSQL");
        assert_eq!(n, Some(Number::new(42)));
        assert_eq!(t, "Use PostgreSQL");
    }

    #[test]
    fn parse_heading_plain_title_has_no_number() {
        let (n, t) = parse_heading("# Adopt Crossplane");
        assert_eq!(n, None);
        assert_eq!(t, "Adopt Crossplane");
    }

    #[test]
    fn parse_markdown_uses_dir_status() {
        let adr = pm(SAMPLE, Some(Status::Accepted));
        assert_eq!(adr.number, Some(Number::new(6)));
        assert_eq!(adr.title, "Adopt ADRs as Team Decision Process");
        assert_eq!(adr.status, Status::Accepted);
    }

    #[test]
    fn parse_markdown_falls_back_to_section_status() {
        let adr = pm(SAMPLE, None);
        assert_eq!(adr.status, Status::Accepted);
    }

    #[test]
    fn parse_markdown_strips_h1_banner_and_status_region_from_the_body() {
        let adr = pm(SAMPLE, Some(Status::Accepted));
        // The body is the KB page shape: prose only, starting at the first
        // real section — the H1, banner, and `## Status` region move to
        // frontmatter.
        assert!(adr.body.starts_with("## Context and Problem Statement"));
        assert!(!adr.body.contains("# ADR-0006"));
        assert!(!adr.body.contains("> State:"));
        assert!(!adr.body.contains("## Status"));
        assert!(
            adr.body
                .contains("We need a consistent way to capture architectural decisions.")
        );
    }

    #[test]
    fn legacy_body_keeps_a_state_line_inside_prose() {
        // Only the *leading* banner is dropped; a `> State:` quoted in prose is
        // content.
        let doc = "# ADR-0001: X\n\n> State: Accepted\n\n## Status\n\nAccepted\n\n## Context\n\nThe old banner read:\n\n> State: Proposed\n";
        let body = legacy_body(doc);
        assert!(body.starts_with("## Context"));
        assert!(body.contains("> State: Proposed"));
        assert!(!body.contains("> State: Accepted"));
    }

    #[test]
    fn parse_superseded_link() {
        let doc = "# ADR-0002: Adopt ADRs\n\n## Status\n\nSuperseded by [ADR-0006](../accepted/0006-adopt-adrs.md)\n";
        let adr = pm(doc, Some(Status::Superseded));
        assert_eq!(adr.status, Status::Superseded);
        assert_eq!(adr.superseded_by, Some(AdrRef::Number(6)));
    }

    // --- supersession: both directions out of the `## Status` region ---------

    #[test]
    fn parse_supersedes_forward_note() {
        // The newer ADR carries a "Supersedes [ADR-NNNN]" note in `## Status`.
        let doc = "# ADR-0006: Adopt ADRs\n\n## Status\n\nAccepted\n\nSupersedes [ADR-0002](../superseded/0002-adopt-adrs.md)\n\n## Context\n\nBody.\n";
        let adr = pm(doc, Some(Status::Accepted));
        assert_eq!(adr.status, Status::Accepted);
        assert_eq!(adr.supersedes, Some(AdrRef::Number(2)));
        assert_eq!(adr.superseded_by, None);
    }

    #[test]
    fn parse_superseded_by_note() {
        let doc = "# ADR-0002: Adopt ADRs\n\n## Status\n\nSuperseded by [ADR-0006](../accepted/0006-adopt-adrs.md)\n";
        let adr = pm(doc, Some(Status::Superseded));
        assert_eq!(adr.superseded_by, Some(AdrRef::Number(6)));
        assert_eq!(adr.supersedes, None);
    }

    #[test]
    fn parse_supersedes_bare_adr_reference() {
        let doc = "# ADR-0006: Adopt ADRs\n\n## Status\n\nAccepted\n\nSupersedes ADR-0002\n";
        let adr = pm(doc, Some(Status::Accepted));
        assert_eq!(adr.supersedes, Some(AdrRef::Number(2)));
    }

    #[test]
    fn parse_review_by_line() {
        let doc = "# ADR-0003: Use Redis\n\n## Status\n\nProposed\n\nReview by: 2026-07-15\n";
        let adr = pm(doc, Some(Status::Proposed));
        assert_eq!(adr.review_by, Some("2026-07-15".parse().unwrap()));
    }

    // ---- created-date provenance (`Created:` in the `## Status` region) ----

    #[test]
    fn parse_created_line_maps_into_created_midnight_utc() {
        let doc = "# ADR-0003: Use Redis\n\n## Status\n\nProposed\nCreated: 2026-06-12\n\n## Context\n\nx\n";
        let adr = pm(doc, None);
        let created = adr.created.get();
        assert_eq!(created.date(), time::macros::date!(2026 - 06 - 12));
        assert_eq!(created.time(), time::Time::MIDNIGHT);
        assert_eq!(created.offset(), time::UtcOffset::UTC);
    }

    #[test]
    fn created_line_outside_the_status_region_is_ignored() {
        let doc = "# ADR-0003: Use Redis\n\n## Status\n\nProposed\n\n## Context\n\n\
             Created: 2020-01-01 is mentioned in prose.\n";
        let adr = pm(doc, None);
        // No status-region Created: line → `created` is the seed time (today).
        assert_ne!(
            adr.created.get().date(),
            time::macros::date!(2020 - 01 - 01)
        );
        // The prose line stays part of the body.
        assert!(
            adr.body
                .contains("Created: 2020-01-01 is mentioned in prose.")
        );
    }

    #[test]
    fn status_region_tolerates_multibyte_chars_at_prefix_boundary() {
        // Regression: "Proposed — note" has an em-dash starting at byte 9, which
        // sits inside the byte range of the 10-char prefixes "Supersedes" /
        // "Review by:". `strip_prefix_ci` must not panic on a non-char-boundary.
        let doc = "# ADR-0003: Sample\n\n## Status\n\nProposed — implementation evolving\n";
        let region = parse_status_region(doc);
        assert!(region.supersedes.is_none());
        assert!(region.superseded_by.is_none());
        assert!(region.review_by.is_none());
    }

    #[test]
    fn qualified_status_line_resolves_via_first_word() {
        // A real-repo status line with trailing qualification resolves to the
        // bare status via its first word (the whole line doesn't parse).
        let doc = "# ADR-0003: X\n\n## Status\n\nProposed — implementation approach evolving based on spike\n";
        assert_eq!(pm(doc, None).status, Status::Proposed);

        // A section whose first word is not a status defaults to Proposed.
        let prose = "# ADR-0005: X\n\n## Status\n\nSee the discussion thread.\n";
        assert_eq!(pm(prose, None).status, Status::Proposed);
    }

    #[test]
    fn upsert_reference_creates_section_then_upserts_idempotently() {
        let base = "## Context\n\nProse.\n";
        // First write creates the section.
        let a = upsert_reference(base, "Issue", "https://x/issues/7");
        assert!(a.contains("## References"));
        assert!(a.contains("- Issue: https://x/issues/7"));
        assert!(a.ends_with('\n'));
        // Re-writing the same label+url is byte-identical (idempotent).
        assert_eq!(upsert_reference(&a, "Issue", "https://x/issues/7"), a);
        // A second label appends a bullet, not a second section.
        let b = upsert_reference(&a, "Pull Request", "https://x/pull/42");
        assert_eq!(b.matches("## References").count(), 1);
        assert!(b.contains("- Issue: https://x/issues/7"));
        assert!(b.contains("- Pull Request: https://x/pull/42"));
        // Re-using an existing label replaces only its URL.
        let c = upsert_reference(&b, "Issue", "https://x/issues/9");
        assert!(c.contains("- Issue: https://x/issues/9"));
        assert!(!c.contains("issues/7"));
        assert_eq!(parse_references(&c).len(), 2);
        assert_eq!(
            parse_references(&c)[0],
            ("Issue".to_string(), "https://x/issues/9".to_string())
        );
    }

    #[test]
    fn normalize_lone_cr_preserves_crlf_and_lf() {
        assert!(matches!(normalize_lone_cr("a\nb"), Cow::Borrowed(_)));
        assert!(matches!(normalize_lone_cr("a\r\nb"), Cow::Borrowed(_)));
        assert_eq!(normalize_lone_cr("a\rb").as_ref(), "a\nb");
        assert_eq!(normalize_lone_cr("a\r\nb\rc").as_ref(), "a\r\nb\nc");
        // Multibyte stays intact.
        assert_eq!(normalize_lone_cr("é\rx").as_ref(), "é\nx");
    }

    #[test]
    fn upsert_reference_is_idempotent_on_lone_cr() {
        // Regression (hardening blitz #4): a lone `\r` (classic-Mac / corrupted
        // file) used to defeat newline detection and make the rewriters
        // non-idempotent — a second `upsert_reference` duplicated
        // `## References`. It now normalizes a lone `\r` to `\n` first.
        let doc = "## Context\r\rProse.\r"; // CR-only line endings

        let once = upsert_reference(doc, "Issue", "https://x/7");
        assert_eq!(
            upsert_reference(&once, "Issue", "https://x/7"),
            once,
            "upsert not idempotent on lone CR"
        );
        assert_eq!(
            once.matches("## References").count(),
            1,
            "no duplicate section"
        );
        assert!(!once.contains('\r'), "lone CR normalized away");
    }
}
