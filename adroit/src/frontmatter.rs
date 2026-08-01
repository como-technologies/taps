use serde::{Deserialize, Serialize};

use crate::adr::{AdrId, Created, Number, ReviewBy, Status};
use crate::naming::AdrRef;

/// The page type adroit owns. Written on every serialize (the KB `decision`
/// schema requires `type`); preserved verbatim if a page already carries one.
const PAGE_TYPE: &str = "decision";

/// The YAML frontmatter fields persisted to disk.
///
/// Separate from `Adr` because the core model has runtime-only fields
/// (`git_sha`, `body`) that don't belong in the YAML block.
///
/// The supersession and review fields are optional and only serialized when
/// present (`skip_serializing_if`), so existing files stay clean and legacy
/// files without them still parse (`#[serde(default)]`).
#[derive(Debug, Serialize, Deserialize)]
struct Frontmatter {
    id: AdrId,
    title: String,
    /// Head-owned display identity (ADR-0020): `ADR-NNNN` for numeric
    /// references, the bare slug otherwise. Replaces the old `number:` field.
    reference: String,
    status: Status,
    created: Created,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    supersedes: Option<AdrRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    superseded_by: Option<AdrRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    relates_to: Vec<AdrRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    depends_on: Vec<AdrRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    refines: Vec<AdrRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    review_by: Option<ReviewBy>,
    /// Every key adroit does not own, captured by `serde(flatten)` on parse
    /// and re-emitted after the named fields in original order. This is the
    /// data-loss guard for KB-resident pages carrying foreign frontmatter
    /// (`type:`, `citations:`, …) — a rewrite must never destroy them.
    #[serde(flatten)]
    extra: serde_yaml_ng::Mapping,
}

/// Render an ADR as a frontmatter + body string for writing to disk.
/// What can go wrong serializing or parsing the frontmatter profile.
#[derive(Debug, thiserror::Error)]
pub enum FrontmatterError {
    #[error("ADR reference must be assigned before serializing")]
    MissingReference,
    #[error("missing opening frontmatter delimiter `---`")]
    MissingOpenDelimiter,
    #[error("missing closing frontmatter delimiter `---`")]
    MissingCloseDelimiter,
    #[error("invalid frontmatter YAML: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),
}

///
/// Returns an error if the ADR's `number` has not been assigned.
pub fn serialize(adr: &crate::adr::Adr) -> Result<String, FrontmatterError> {
    if adr.number.is_none() && adr.slug.is_none() {
        return Err(FrontmatterError::MissingReference);
    }
    let reference = display_reference(&adr.reference());

    // The KB decision schema requires `type`; stamp it unless the page
    // already carries one (which then round-trips verbatim via `extra`).
    let mut extra = adr.extra.clone();
    if !extra.contains_key("type") {
        extra.insert(
            serde_yaml_ng::Value::String("type".into()),
            serde_yaml_ng::Value::String(PAGE_TYPE.into()),
        );
    }

    let fm = Frontmatter {
        id: adr.id,
        title: adr.title.clone(),
        reference,
        status: adr.status,
        created: adr.created,
        supersedes: adr.supersedes.clone(),
        superseded_by: adr.superseded_by.clone(),
        relates_to: adr.relates_to.clone(),
        depends_on: adr.depends_on.clone(),
        refines: adr.refines.clone(),
        review_by: adr.review_by,
        extra,
    };

    let yaml = serde_yaml_ng::to_string(&fm)?;
    let mut out = String::from("---\n");
    out.push_str(&yaml);
    out.push_str("---\n");
    // Canonical body: exactly one trailing newline. `deserialize` trims the
    // body's tail anyway, so a caller-supplied trailing newline (a rendered
    // template, a spliced draft) must not produce a non-canonical `\n\n` file
    // that the next rewrite would then "minimally" normalize.
    let body = adr.body.trim_end();
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body);
        out.push('\n');
    }
    Ok(out)
}

/// Parse a frontmatter + body string back into an Adr.
///
/// The `git_sha` field is left as `None` — the caller can populate it.
pub fn deserialize(input: &str) -> Result<crate::adr::Adr, FrontmatterError> {
    let (yaml, body) = split_frontmatter(input)?;
    let fm: Frontmatter = serde_yaml_ng::from_str(yaml)?;
    let (number, slug) = parse_reference(&fm.reference);
    Ok(crate::adr::Adr {
        id: fm.id,
        number,
        slug,
        title: fm.title,
        status: fm.status,
        created: fm.created,
        body: body.trim_end().to_owned(),
        git_sha: None,
        supersedes: fm.supersedes,
        superseded_by: fm.superseded_by,
        relates_to: fm.relates_to,
        depends_on: fm.depends_on,
        refines: fm.refines,
        review_by: fm.review_by,
        extra: fm.extra,
    })
}

/// Remap every numeric reference to ADR `old` to ADR `new` across a frontmatter
/// ADR's reference fields (`supersedes` / `superseded_by` / `relates_to` /
/// `depends_on` / `refines`), returning the re-serialized document — or `None`
/// if nothing referenced `old` (so the caller leaves the file untouched).
///
/// These refs are bare numbers in the YAML block, not markdown links, so
/// `links::relabel_links_to` (which `renumber` uses for inbound links) can't see
/// them. Without this, `renumber 1 9` would strand another ADR's
/// `superseded_by: 1` — a dangling pointer `check` flags as a broken
/// supersession. The markdown profile heals the equivalent (a `## Status`
/// `Superseded by [ADR-0001](…)` link) via that same link relabeling, so this
/// closes the frontmatter-only gap. Unparseable input yields `None` (renumber
/// must not hard-fail on a malformed neighbor; `check` reports those separately).
pub fn remap_numeric_refs(text: &str, old: Number, new: Number) -> Option<String> {
    fn remap(r: &mut AdrRef, old: Number, new: Number) -> bool {
        if *r == AdrRef::Number(old.get()) {
            *r = AdrRef::Number(new.get());
            true
        } else {
            false
        }
    }

    let mut adr = deserialize(text).ok()?;
    let mut changed = false;
    if let Some(r) = adr.supersedes.as_mut() {
        changed |= remap(r, old, new);
    }
    if let Some(r) = adr.superseded_by.as_mut() {
        changed |= remap(r, old, new);
    }
    for r in adr
        .relates_to
        .iter_mut()
        .chain(adr.depends_on.iter_mut())
        .chain(adr.refines.iter_mut())
    {
        changed |= remap(r, old, new);
    }
    if !changed {
        return None;
    }
    serialize(&adr).ok()
}

/// Render an [`AdrRef`] as the persisted `reference:` string: `ADR-NNNN` for
/// numbers, the bare slug otherwise.
fn display_reference(r: &AdrRef) -> String {
    match r {
        AdrRef::Number(n) => format!("ADR-{n:04}"),
        AdrRef::Slug(s) => s.clone(),
    }
}

/// Parse a persisted `reference:` string back into the model's identity pair.
fn parse_reference(reference: &str) -> (Option<Number>, Option<String>) {
    if let Some(num) = reference
        .strip_prefix("ADR-")
        .and_then(|n| n.parse::<u32>().ok())
    {
        (Some(Number::new(num)), None)
    } else {
        (None, Some(reference.to_string()))
    }
}

/// Split a document into (frontmatter_yaml, body) at the `---` delimiters.
fn split_frontmatter(input: &str) -> Result<(&str, &str), FrontmatterError> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with("---") {
        return Err(FrontmatterError::MissingOpenDelimiter);
    }
    let after_open = trimmed[3..].trim_start_matches(['\r', '\n']);
    let close_pos = after_open
        .find("\n---")
        .ok_or(FrontmatterError::MissingCloseDelimiter)?;
    // Keep the newline before the delimiter: it belongs to the YAML block, and
    // a keep-chomping block scalar (`|+`) as the final value needs it to
    // round-trip losslessly.
    let yaml = &after_open[..close_pos + 1];
    let rest = &after_open[close_pos + 4..]; // skip "\n---"
    let body = rest.trim_start_matches(['\r', '\n']);
    Ok((yaml, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adr::{Adr, Number, Status};

    fn sample_adr() -> Adr {
        let mut adr = Adr::new("Use PostgreSQL").unwrap();
        adr.number = Some(Number::new(1));
        adr
    }

    #[test]
    fn round_trip() {
        let adr = sample_adr();
        let text = serialize(&adr).unwrap();
        let parsed = deserialize(&text).unwrap();

        assert_eq!(parsed.id, adr.id);
        assert_eq!(parsed.number, adr.number);
        assert_eq!(parsed.title, adr.title);
        assert_eq!(parsed.status, adr.status);
        assert_eq!(parsed.created, adr.created);
    }

    #[test]
    fn round_trip_with_body() {
        let mut adr = sample_adr();
        adr.body = "## Context\n\nWe need a database.\n\n## Decision\n\nPostgreSQL.".to_string();
        let text = serialize(&adr).unwrap();
        let parsed = deserialize(&text).unwrap();

        assert_eq!(parsed.body, adr.body);
    }

    #[test]
    fn serialize_requires_number() {
        let adr = Adr::new("No number yet").unwrap();
        assert!(serialize(&adr).is_err());
    }

    #[test]
    fn deserialize_missing_opening_delimiter() {
        let input = "no frontmatter here";
        assert!(deserialize(input).is_err());
    }

    #[test]
    fn deserialize_missing_closing_delimiter() {
        let input = "---\nid: bad\n";
        assert!(deserialize(input).is_err());
    }

    #[test]
    fn serialized_format_has_delimiters() {
        let adr = sample_adr();
        let text = serialize(&adr).unwrap();
        assert!(text.starts_with("---\n"));
        assert!(text.contains("\n---\n"));
    }

    #[test]
    fn all_statuses_round_trip() {
        for status in [
            Status::Proposed,
            Status::Accepted,
            Status::Rejected,
            Status::Deprecated,
            Status::Superseded,
        ] {
            let mut adr = sample_adr();
            adr.status = status;
            let text = serialize(&adr).unwrap();
            let parsed = deserialize(&text).unwrap();
            assert_eq!(parsed.status, status);
        }
    }

    #[test]
    fn empty_body_round_trip() {
        let adr = sample_adr();
        assert!(adr.body.is_empty());
        let text = serialize(&adr).unwrap();
        let parsed = deserialize(&text).unwrap();
        assert!(parsed.body.is_empty());
    }

    #[test]
    fn supersession_fields_round_trip() {
        let mut adr = sample_adr();
        adr.supersedes = Some(AdrRef::Number(2));
        adr.superseded_by = Some(AdrRef::Number(9));
        let text = serialize(&adr).unwrap();
        // Numeric refs serialize as bare numbers (byte-identical with the old
        // `Option<Number>` fields).
        assert!(text.contains("supersedes: 2"));
        assert!(text.contains("superseded_by: 9"));
        let parsed = deserialize(&text).unwrap();
        assert_eq!(parsed.supersedes, Some(AdrRef::Number(2)));
        assert_eq!(parsed.superseded_by, Some(AdrRef::Number(9)));
    }

    #[test]
    fn remap_numeric_refs_retargets_matching_and_leaves_others() {
        let mut adr = sample_adr(); // ADR 1
        adr.superseded_by = Some(AdrRef::Number(2));
        adr.depends_on = vec![AdrRef::Number(2), AdrRef::Number(3)];
        let text = serialize(&adr).unwrap();

        // 2 -> 9 retargets every ref to ADR 2, leaving ADR 3 untouched.
        let out = remap_numeric_refs(&text, Number::new(2), Number::new(9)).unwrap();
        let parsed = deserialize(&out).unwrap();
        assert_eq!(parsed.superseded_by, Some(AdrRef::Number(9)));
        assert_eq!(
            parsed.depends_on,
            vec![AdrRef::Number(9), AdrRef::Number(3)]
        );

        // No ref equals ADR 7 -> None, so the caller leaves the file byte-identical.
        assert!(remap_numeric_refs(&text, Number::new(7), Number::new(8)).is_none());
        // Slug refs are never numeric, so they're left alone too.
        assert!(remap_numeric_refs("not frontmatter", Number::new(2), Number::new(9)).is_none());
    }

    #[test]
    fn supersession_slug_refs_round_trip() {
        let mut adr = sample_adr();
        adr.superseded_by = Some(AdrRef::Slug("20260601-adopt-x".into()));
        let text = serialize(&adr).unwrap();
        assert!(text.contains("superseded_by: 20260601-adopt-x"));
        let parsed = deserialize(&text).unwrap();
        assert_eq!(
            parsed.superseded_by,
            Some(AdrRef::Slug("20260601-adopt-x".into()))
        );
    }

    #[test]
    fn supersession_fields_absent_when_none() {
        let adr = sample_adr();
        let text = serialize(&adr).unwrap();
        // Clean files: optional fields are not emitted when unset.
        assert!(!text.contains("supersedes:"));
        assert!(!text.contains("superseded_by:"));
        assert!(!text.contains("relates_to:"));
        assert!(!text.contains("depends_on:"));
        assert!(!text.contains("refines:"));
        assert!(!text.contains("review_by:"));
    }

    #[test]
    fn typed_links_round_trip() {
        let mut adr = sample_adr();
        adr.depends_on = vec![AdrRef::Number(2), AdrRef::Number(3)];
        adr.relates_to = vec![AdrRef::Slug("20260601-adopt-x".into())];
        let text = serialize(&adr).unwrap();
        assert!(text.contains("depends_on:"));
        assert!(text.contains("relates_to:"));
        // `refines` is empty → not emitted (clean files).
        assert!(!text.contains("refines:"));

        let parsed = deserialize(&text).unwrap();
        assert_eq!(
            parsed.depends_on,
            vec![AdrRef::Number(2), AdrRef::Number(3)]
        );
        assert_eq!(
            parsed.relates_to,
            vec![AdrRef::Slug("20260601-adopt-x".into())]
        );
        assert!(parsed.refines.is_empty());
    }

    /// Splice foreign key lines into a serialized doc just before the closing
    /// delimiter — the position adroit's own writes would re-emit them at.
    fn with_foreign_keys(text: &str, foreign: &str) -> String {
        let close = text.rfind("\n---\n").unwrap();
        let mut out = String::from(&text[..close + 1]);
        out.push_str(foreign);
        out.push_str(&text[close + 1..]);
        out
    }

    // `type:` is adroit-owned now (stamped by serialize); foreign keys are
    // everything else the KB substrate hangs on a page.
    const FOREIGN: &str = "citations:\n- evidence/transcripts/2026-07-20-kb-chat.md@abc123\nkb_meta:\n  confidence: 0.9\n  reviewed: true\n";

    #[test]
    fn unknown_keys_survive_a_parse_serialize_round_trip() {
        let text = with_foreign_keys(&serialize(&sample_adr()).unwrap(), FOREIGN);
        let parsed = deserialize(&text).unwrap();
        assert_eq!(parsed.extra.len(), 3);
        assert_eq!(parsed.extra.get("type").unwrap().as_str(), Some("decision"));
        assert!(parsed.extra.get("citations").unwrap().is_sequence());
        // Nested values keep their YAML typing, not stringified.
        assert_eq!(
            parsed.extra.get("kb_meta").unwrap()["confidence"].as_f64(),
            Some(0.9)
        );

        // Re-emitting reproduces the input byte-for-byte (extras were already
        // in adroit's emit position and style), and the round trip converges.
        let out = serialize(&parsed).unwrap();
        assert_eq!(out, text);
        assert_eq!(serialize(&deserialize(&out).unwrap()).unwrap(), out);
    }

    #[test]
    fn unknown_keys_survive_a_status_rewrite_byte_intact() {
        let text = with_foreign_keys(&serialize(&sample_adr()).unwrap(), FOREIGN);
        let mut parsed = deserialize(&text).unwrap();
        parsed.status = Status::Accepted;
        // The set-status rewrite: everything byte-identical but the status line.
        assert_eq!(
            serialize(&parsed).unwrap(),
            text.replace("status: proposed", "status: accepted")
        );
    }

    #[test]
    fn unknown_keys_coexist_with_every_optional_field() {
        // Guards the serde(flatten) buffering path: untagged AdrRefs (number
        // AND slug) plus the `with`-module dates must still round-trip when a
        // flattened map forces content buffering.
        let mut adr = sample_adr();
        adr.supersedes = Some(AdrRef::Number(2));
        adr.superseded_by = Some(AdrRef::Slug("20260601-adopt-x".into()));
        adr.depends_on = vec![AdrRef::Number(3)];
        adr.review_by = Some("2026-07-01".parse::<ReviewBy>().unwrap());
        let text = with_foreign_keys(&serialize(&adr).unwrap(), FOREIGN);

        let parsed = deserialize(&text).unwrap();
        assert_eq!(parsed.supersedes, Some(AdrRef::Number(2)));
        assert_eq!(
            parsed.superseded_by,
            Some(AdrRef::Slug("20260601-adopt-x".into()))
        );
        assert_eq!(parsed.depends_on, vec![AdrRef::Number(3)]);
        assert_eq!(parsed.review_by, adr.review_by);
        assert_eq!(parsed.created, adr.created);
        assert_eq!(parsed.extra.len(), 3);
        assert_eq!(serialize(&parsed).unwrap(), text);
    }

    #[test]
    fn a_fresh_page_carries_exactly_the_stamped_type() {
        let adr = sample_adr();
        assert!(adr.extra.is_empty());
        let parsed = deserialize(&serialize(&adr).unwrap()).unwrap();
        assert_eq!(parsed.extra.len(), 1);
        assert_eq!(parsed.extra.get("type").unwrap().as_str(), Some("decision"));
    }

    #[test]
    fn remap_numeric_refs_preserves_unknown_keys() {
        let mut adr = sample_adr();
        adr.superseded_by = Some(AdrRef::Number(2));
        let text = with_foreign_keys(&serialize(&adr).unwrap(), FOREIGN);
        let out = remap_numeric_refs(&text, Number::new(2), Number::new(9)).unwrap();
        assert!(out.contains("type: decision"));
        assert!(out.contains("kb_meta:"));
        assert!(out.contains("superseded_by: 9"));
    }

    #[test]
    fn review_by_round_trips() {
        use crate::adr::ReviewBy;
        let mut adr = sample_adr();
        adr.review_by = Some("2026-07-01".parse::<ReviewBy>().unwrap());
        let text = serialize(&adr).unwrap();
        assert!(text.contains("review_by: 2026-07-01"));
        let parsed = deserialize(&text).unwrap();
        assert_eq!(parsed.review_by, adr.review_by);
    }
}
