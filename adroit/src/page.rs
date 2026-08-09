//! The decision page model and its frontmatter round-trip.
//!
//! A decision is a KB page of type `decision` (`schemas/decision.json`):
//! YAML frontmatter adroit owns plus a free-form markdown body. The
//! round-trip is sacred — every key adroit does not own rides the flattened
//! [`Decision::extra`] mapping and survives every edit untouched, so a
//! rewrite (`set-status`, `supersede`, `plan --save`) never destroys what it
//! doesn't know.
//!
//! Link fields (`supersedes` / `superseded_by` / `relates_to` / `depends_on`
//! / `refines`) hold **strings** — a page id (preferred) or a slug — exactly
//! as the schema types them. The display `reference:` (`ADR-0007`) is
//! head-owned identity, formatted here and parsed back through
//! [`crate::naming::AdrRef`]; pages address by slug/id — references are for
//! humans.

use serde::{Deserialize, Serialize};
use time::format_description::well_known::{Iso8601, Rfc3339};
use time::{Date, OffsetDateTime};
use ulid::Ulid;

use crate::naming::AdrRef;

// The lifecycle status enum lives in `como-contract` (the suite's shared-seam
// crate): conduit deserializes exactly what adroit serializes, by
// construction. Re-exported so `crate::page::Status` stays the in-crate path.
pub use como_contract::adroit::Status;

/// The page type adroit owns. Written on every serialize (the KB `decision`
/// schema requires `type`); preserved verbatim if a page already carries one.
const PAGE_TYPE: &str = "decision";

// ---------------------------------------------------------------------------
// Newtypes
// ---------------------------------------------------------------------------

/// Stable opaque page identifier: a ULID, persisted in frontmatter as the
/// 26-char Crockford string the `decision` schema's `id` pattern expects.
/// Links target this, never the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DecisionId(Ulid);

impl DecisionId {
    pub fn new() -> Self {
        Self(Ulid::generate())
    }

    /// Parse a 26-char ULID string (either case). `None` when `s` isn't one.
    pub fn parse(s: &str) -> Option<Self> {
        Ulid::from_string(s.trim()).ok().map(Self)
    }

    /// The id as a lowercase slug (the `uuid` naming scheme derives the
    /// display reference from it).
    pub fn slug(self) -> String {
        self.0.to_string().to_lowercase()
    }
}

impl Default for DecisionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DecisionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// UTC timestamp of when a decision was created (RFC 3339 in frontmatter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Created(#[serde(with = "time::serde::rfc3339")] OffsetDateTime);

impl Created {
    pub fn now() -> Self {
        Self(OffsetDateTime::now_utc())
    }

    pub fn get(self) -> OffsetDateTime {
        self.0
    }
}

impl Default for Created {
    fn default() -> Self {
        Self::now()
    }
}

impl std::fmt::Display for Created {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.0.format(&Rfc3339).map_err(|_| std::fmt::Error)?
        )
    }
}

/// A review deadline for a still-proposed decision (a plain calendar date),
/// serialized as ISO-8601 `YYYY-MM-DD` — the `review_by` schema pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ReviewBy(#[serde(with = "review_by_format")] Date);

impl ReviewBy {
    pub fn get(self) -> Date {
        self.0
    }
}

impl std::fmt::Display for ReviewBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.0.format(&Iso8601::DATE).map_err(|_| std::fmt::Error)?
        )
    }
}

impl std::str::FromStr for ReviewBy {
    type Err = PageError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Date::parse(s.trim(), &Iso8601::DATE)
            .map(Self)
            .map_err(|_| PageError::BadReviewDate(s.to_string()))
    }
}

/// Serialize/deserialize a [`Date`] as an ISO-8601 `YYYY-MM-DD` string.
mod review_by_format {
    use super::*;
    use serde::de::Error;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(date: &Date, s: S) -> Result<S::Ok, S::Error> {
        let text = date
            .format(&Iso8601::DATE)
            .map_err(serde::ser::Error::custom)?;
        s.serialize_str(&text)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Date, D::Error> {
        let text = String::deserialize(d)?;
        Date::parse(text.trim(), &Iso8601::DATE).map_err(D::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// What can go wrong building a decision or round-tripping its page profile.
#[derive(Debug, thiserror::Error)]
pub enum PageError {
    #[error("decision title must not be empty")]
    EmptyTitle,
    #[error("invalid review date '{0}', expected ISO-8601 YYYY-MM-DD")]
    BadReviewDate(String),
    #[error("decision reference must be assigned before serializing")]
    MissingReference,
    #[error("missing opening frontmatter delimiter `---`")]
    MissingOpenDelimiter,
    #[error("missing closing frontmatter delimiter `---`")]
    MissingCloseDelimiter,
    #[error("invalid frontmatter YAML: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),
}

// ---------------------------------------------------------------------------
// Decision
// ---------------------------------------------------------------------------

/// A single decision record: the frontmatter fields adroit owns, the body,
/// and everything else on the page (preserved verbatim in `extra`).
#[derive(Debug, Clone)]
pub struct Decision {
    /// Canonical unique identifier (ULID) — the routing identity.
    pub id: DecisionId,
    /// Head-owned display identity. `None` until assigned (a decision in
    /// flight, before its first write).
    pub reference: Option<AdrRef>,
    /// Short title describing the decision.
    pub title: String,
    /// Current lifecycle status.
    pub status: Status,
    /// When this decision was first created (UTC).
    pub created: Created,
    /// Free-form body in markdown (everything after the frontmatter).
    pub body: String,
    /// Id (preferred) or slug of the older decision this one replaces.
    pub supersedes: Option<String>,
    /// Id (preferred) or slug of the newer decision replacing this one.
    pub superseded_by: Option<String>,
    /// Ids/slugs of related pages — the provenance edges `new --relates`
    /// records; the engine's `broken-link` lint enforces targets are real.
    pub relates_to: Vec<String>,
    pub depends_on: Vec<String>,
    pub refines: Vec<String>,
    /// Optional review deadline while the decision is still proposed.
    pub review_by: Option<ReviewBy>,
    /// Frontmatter keys adroit does not own — e.g. `summary:`, `citations:`,
    /// or anything the KB substrate hangs on a page. Captured on parse and
    /// re-emitted verbatim on every write, in original order.
    pub extra: serde_yaml_ng::Mapping,
}

impl Decision {
    /// A fresh proposed decision with only a title; the store assigns the
    /// reference on first write.
    pub fn new(title: impl Into<String>) -> Result<Self, PageError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(PageError::EmptyTitle);
        }
        Ok(Self {
            id: DecisionId::default(),
            reference: None,
            title,
            status: Status::default(),
            created: Created::default(),
            body: String::new(),
            supersedes: None,
            superseded_by: None,
            relates_to: Vec::new(),
            depends_on: Vec::new(),
            refines: Vec::new(),
            review_by: None,
            extra: serde_yaml_ng::Mapping::new(),
        })
    }

    /// The persisted display form of the reference (`ADR-0007`, or the bare
    /// slug for slug schemes). Errors while unassigned.
    pub fn display_reference(&self) -> Result<String, PageError> {
        match &self.reference {
            Some(r) => Ok(display_reference(r)),
            None => Err(PageError::MissingReference),
        }
    }
}

// ---------------------------------------------------------------------------
// Frontmatter round-trip
// ---------------------------------------------------------------------------

/// The YAML frontmatter fields persisted to the page. Separate from
/// [`Decision`] because the model carries the body, which doesn't belong in
/// the YAML block. Optional fields are only serialized when present
/// (`skip_serializing_if`) so pages stay clean, and default on parse so
/// minimal pages still load.
#[derive(Debug, Serialize, Deserialize)]
struct Frontmatter {
    id: DecisionId,
    title: String,
    reference: String,
    status: Status,
    created: Created,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    supersedes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    superseded_by: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    relates_to: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    refines: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    review_by: Option<ReviewBy>,
    /// Every key adroit does not own, captured by `serde(flatten)` on parse
    /// and re-emitted after the named fields in original order — the
    /// data-loss guard for pages carrying foreign frontmatter.
    #[serde(flatten)]
    extra: serde_yaml_ng::Mapping,
}

/// Render a decision as a frontmatter + body page for writing to the KB.
/// Errors if the reference has not been assigned.
pub fn serialize(decision: &Decision) -> Result<String, PageError> {
    let reference = decision.display_reference()?;

    // The KB decision schema requires `type`; stamp it unless the page
    // already carries one (which then round-trips verbatim via `extra`).
    let mut extra = decision.extra.clone();
    if !extra.contains_key("type") {
        extra.insert(
            serde_yaml_ng::Value::String("type".into()),
            serde_yaml_ng::Value::String(PAGE_TYPE.into()),
        );
    }

    let fm = Frontmatter {
        id: decision.id,
        title: decision.title.clone(),
        reference,
        status: decision.status,
        created: decision.created,
        supersedes: decision.supersedes.clone(),
        superseded_by: decision.superseded_by.clone(),
        relates_to: decision.relates_to.clone(),
        depends_on: decision.depends_on.clone(),
        refines: decision.refines.clone(),
        review_by: decision.review_by,
        extra,
    };

    let yaml = serde_yaml_ng::to_string(&fm)?;
    let mut out = String::from("---\n");
    out.push_str(&yaml);
    out.push_str("---\n");
    // Canonical body: exactly one trailing newline, so a caller-supplied
    // trailing newline (a rendered template, a spliced draft) can't produce
    // a non-canonical file the next rewrite would then "normalize".
    let body = decision.body.trim_end();
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body);
        out.push('\n');
    }
    Ok(out)
}

/// Parse a frontmatter + body page back into a [`Decision`].
pub fn deserialize(input: &str) -> Result<Decision, PageError> {
    let (yaml, body) = split_frontmatter(input)?;
    let fm: Frontmatter = serde_yaml_ng::from_str(yaml)?;
    Ok(Decision {
        id: fm.id,
        reference: Some(parse_reference(&fm.reference)),
        title: fm.title,
        status: fm.status,
        created: fm.created,
        body: body.trim_end().to_owned(),
        supersedes: fm.supersedes,
        superseded_by: fm.superseded_by,
        relates_to: fm.relates_to,
        depends_on: fm.depends_on,
        refines: fm.refines,
        review_by: fm.review_by,
        extra: fm.extra,
    })
}

/// Render an [`AdrRef`] as the persisted `reference:` string: `ADR-NNNN` for
/// numbers, the bare slug otherwise.
pub fn display_reference(r: &AdrRef) -> String {
    match r {
        AdrRef::Number(n) => format!("ADR-{n:04}"),
        AdrRef::Slug(s) => s.clone(),
    }
}

/// Parse a persisted `reference:` string back into the display identity.
pub fn parse_reference(reference: &str) -> AdrRef {
    match reference
        .strip_prefix("ADR-")
        .and_then(|n| n.parse::<u32>().ok())
    {
        Some(num) => AdrRef::Number(num),
        None => AdrRef::Slug(reference.to_string()),
    }
}

/// Split a page into (frontmatter_yaml, body) at the `---` delimiters.
fn split_frontmatter(input: &str) -> Result<(&str, &str), PageError> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with("---") {
        return Err(PageError::MissingOpenDelimiter);
    }
    let after_open = trimmed[3..].trim_start_matches(['\r', '\n']);
    let close_pos = after_open
        .find("\n---")
        .ok_or(PageError::MissingCloseDelimiter)?;
    // Keep the newline before the delimiter: it belongs to the YAML block,
    // and a keep-chomping block scalar (`|+`) as the final value needs it to
    // round-trip losslessly.
    let yaml = &after_open[..close_pos + 1];
    let rest = &after_open[close_pos + 4..]; // skip "\n---"
    let body = rest.trim_start_matches(['\r', '\n']);
    Ok((yaml, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Decision {
        let mut d = Decision::new("Use PostgreSQL").unwrap();
        d.reference = Some(AdrRef::Number(1));
        d
    }

    #[test]
    fn round_trip() {
        let d = sample();
        let text = serialize(&d).unwrap();
        let parsed = deserialize(&text).unwrap();
        assert_eq!(parsed.id, d.id);
        assert_eq!(parsed.reference, Some(AdrRef::Number(1)));
        assert_eq!(parsed.title, d.title);
        assert_eq!(parsed.status, d.status);
        assert_eq!(parsed.created, d.created);
    }

    #[test]
    fn round_trip_with_body() {
        let mut d = sample();
        d.body = "## Context\n\nWe need a database.\n\n## Decision\n\nPostgreSQL.".to_string();
        let text = serialize(&d).unwrap();
        assert_eq!(deserialize(&text).unwrap().body, d.body);
    }

    #[test]
    fn serialize_requires_reference() {
        let d = Decision::new("No reference yet").unwrap();
        assert!(matches!(serialize(&d), Err(PageError::MissingReference)));
    }

    #[test]
    fn empty_title_errors() {
        assert!(Decision::new("").is_err());
        assert!(Decision::new("   ").is_err());
    }

    #[test]
    fn deserialize_missing_delimiters() {
        assert!(deserialize("no frontmatter here").is_err());
        assert!(deserialize("---\nid: bad\n").is_err());
    }

    #[test]
    fn all_statuses_round_trip() {
        for status in Status::ALL {
            let mut d = sample();
            d.status = status;
            let parsed = deserialize(&serialize(&d).unwrap()).unwrap();
            assert_eq!(parsed.status, status);
        }
    }

    #[test]
    fn empty_body_round_trip() {
        let d = sample();
        assert!(d.body.is_empty());
        assert!(
            deserialize(&serialize(&d).unwrap())
                .unwrap()
                .body
                .is_empty()
        );
    }

    #[test]
    fn link_fields_are_strings_and_round_trip() {
        // The decision schema types every link field as a string (id
        // preferred, slug allowed) — never a bare YAML number.
        let mut d = sample();
        let target = DecisionId::new().to_string();
        d.supersedes = Some(target.clone());
        d.superseded_by = Some("decisions/0009-newer".into());
        d.relates_to = vec!["assessments/taps-report".into()];
        let text = serialize(&d).unwrap();
        assert!(text.contains(&format!("supersedes: {target}")));
        assert!(text.contains("superseded_by: decisions/0009-newer"));
        let parsed = deserialize(&text).unwrap();
        assert_eq!(parsed.supersedes, Some(target));
        assert_eq!(parsed.superseded_by, Some("decisions/0009-newer".into()));
        assert_eq!(
            parsed.relates_to,
            vec!["assessments/taps-report".to_string()]
        );
    }

    #[test]
    fn optional_fields_absent_when_none() {
        let text = serialize(&sample()).unwrap();
        for key in [
            "supersedes:",
            "superseded_by:",
            "relates_to:",
            "depends_on:",
            "refines:",
            "review_by:",
        ] {
            assert!(!text.contains(key), "clean page should omit {key}");
        }
    }

    #[test]
    fn slug_reference_round_trips() {
        let mut d = sample();
        d.reference = Some(AdrRef::Slug("20260601-adopt-x".into()));
        let text = serialize(&d).unwrap();
        assert!(text.contains("reference: 20260601-adopt-x"));
        let parsed = deserialize(&text).unwrap();
        assert_eq!(
            parsed.reference,
            Some(AdrRef::Slug("20260601-adopt-x".into()))
        );
    }

    /// Splice foreign key lines into a serialized page just before the
    /// closing delimiter — the position adroit's own writes re-emit them at.
    fn with_foreign_keys(text: &str, foreign: &str) -> String {
        let close = text.rfind("\n---\n").unwrap();
        let mut out = String::from(&text[..close + 1]);
        out.push_str(foreign);
        out.push_str(&text[close + 1..]);
        out
    }

    // `type:` is adroit-owned (stamped by serialize); foreign keys are
    // everything else the KB substrate hangs on a page.
    const FOREIGN: &str = "summary: one-line scope\ncitations:\n- evidence/transcripts/2026-07-20-kb-chat.md@abc123\nkb_meta:\n  confidence: 0.9\n  reviewed: true\n";

    #[test]
    fn unknown_keys_survive_a_parse_serialize_round_trip() {
        let text = with_foreign_keys(&serialize(&sample()).unwrap(), FOREIGN);
        let parsed = deserialize(&text).unwrap();
        assert_eq!(parsed.extra.len(), 4);
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
        let text = with_foreign_keys(&serialize(&sample()).unwrap(), FOREIGN);
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
        // Guards the serde(flatten) buffering path: the `with`-module dates
        // must still round-trip when a flattened map forces content buffering.
        let mut d = sample();
        d.supersedes = Some("older-id".into());
        d.superseded_by = Some("decisions/0002-newer".into());
        d.depends_on = vec!["decisions/0003-base".into()];
        d.review_by = Some("2026-07-01".parse().unwrap());
        let text = with_foreign_keys(&serialize(&d).unwrap(), FOREIGN);

        let parsed = deserialize(&text).unwrap();
        assert_eq!(parsed.supersedes, d.supersedes);
        assert_eq!(parsed.superseded_by, d.superseded_by);
        assert_eq!(parsed.depends_on, d.depends_on);
        assert_eq!(parsed.review_by, d.review_by);
        assert_eq!(parsed.created, d.created);
        assert_eq!(parsed.extra.len(), 4);
        assert_eq!(serialize(&parsed).unwrap(), text);
    }

    #[test]
    fn a_fresh_page_carries_exactly_the_stamped_type() {
        let d = sample();
        assert!(d.extra.is_empty());
        let parsed = deserialize(&serialize(&d).unwrap()).unwrap();
        assert_eq!(parsed.extra.len(), 1);
        assert_eq!(parsed.extra.get("type").unwrap().as_str(), Some("decision"));
    }

    #[test]
    fn review_by_round_trips() {
        let mut d = sample();
        d.review_by = Some("2026-07-01".parse().unwrap());
        let text = serialize(&d).unwrap();
        assert!(text.contains("review_by: 2026-07-01"));
        assert_eq!(deserialize(&text).unwrap().review_by, d.review_by);
    }

    #[test]
    fn decision_id_is_the_schema_pattern() {
        let id = DecisionId::new();
        let s = id.to_string();
        // ULID: 26 chars of Crockford base32, uppercase (the KB id pattern).
        assert_eq!(s.len(), 26);
        assert!(
            s.chars()
                .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
        );
        assert_eq!(DecisionId::parse(&s), Some(id));
        assert_eq!(DecisionId::parse(&id.slug()), Some(id));
        assert_eq!(id.slug(), s.to_lowercase());
    }

    #[test]
    fn created_display_is_rfc3339() {
        let s = Created::now().to_string();
        assert!(s.contains('T'));
        assert!(s.ends_with('Z'));
    }

    #[test]
    fn reference_display_and_parse_round_trip() {
        assert_eq!(display_reference(&AdrRef::Number(7)), "ADR-0007");
        assert_eq!(parse_reference("ADR-0007"), AdrRef::Number(7));
        assert_eq!(
            parse_reference("20260720-use-postgres"),
            AdrRef::Slug("20260720-use-postgres".into())
        );
    }
}
