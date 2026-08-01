//! The ADR identity / naming **seam**.
//!
//! ALL scheme-specific logic lives here. The rest of the codebase depends only
//! on [`AdrRef`] + [`NamingScheme`] and never branches on the scheme — so adding
//! or changing a scheme means editing only this file (plus its tests), never the
//! ~12 consumer modules (store / query / format / index / surfaces).
//!
//! - [`AdrRef`] is the scheme-agnostic *display / reference* identity (the
//!   canonical UUID `adr::AdrId` is separate and unchanged).
//! - [`NamingScheme`] is the config enum; its methods encapsulate how each scheme
//!   assigns, parses, names, displays, links, and scopes ADR identifiers.

use std::path::Path;

use serde::{Deserialize, Serialize};
use time::Date;

/// A scheme-agnostic display / reference identity for an ADR.
///
/// `Number` backs the sequential and per-category schemes; `Slug` backs the
/// date (`YYYYMMDD-title`) and uuid schemes (its identity is the filename stem).
///
/// Serializes **untagged** so it round-trips cleanly in YAML frontmatter:
/// `Number(9)` ⇄ `9` (byte-identical with the old numeric fields), `Slug(s)` ⇄
/// the bare string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AdrRef {
    Number(u32),
    Slug(String),
}

impl AdrRef {
    pub fn as_number(&self) -> Option<u32> {
        match self {
            AdrRef::Number(n) => Some(*n),
            AdrRef::Slug(_) => None,
        }
    }

    pub fn as_slug(&self) -> Option<&str> {
        match self {
            AdrRef::Slug(s) => Some(s),
            AdrRef::Number(_) => None,
        }
    }

    /// The canonical **addressing** token — the string a user/URL passes to
    /// reach this ADR, which [`NamingScheme::parse_ref`] round-trips back: the
    /// bare number for numeric schemes, the slug/uuid for slug schemes. (Distinct
    /// from the *display* string, e.g. `ADR-0009` or a shortened uuid.)
    pub fn addr(&self) -> String {
        match self {
            AdrRef::Number(n) => n.to_string(),
            AdrRef::Slug(s) => s.clone(),
        }
    }
}

/// How ADR identifiers are formed. A config enum (serde / clap / strum) whose
/// methods own every scheme's behavior — re-exported by `config` for the
/// `Config`/`StoreOptions` fields.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumString,
    clap::ValueEnum,
)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
#[serde(rename_all = "snake_case")]
#[value(rename_all = "snake_case")]
pub enum NamingScheme {
    /// Global zero-padded `NNNN` (the default). Human-friendly; collision-prone
    /// across branches.
    #[default]
    Sequential,
    /// `YYYYMMDD-title` slug (log4brains-style). Collision-free; no number.
    Date,
    /// A persisted UUID. Collision-free; not human-sortable.
    Uuid,
}

impl NamingScheme {
    /// `true` when this scheme's identity is a single global sequential number
    /// — i.e. `adroit renumber`/`review` apply and the CLI accepts a bare number.
    pub fn is_numeric(&self) -> bool {
        matches!(self, NamingScheme::Sequential)
    }

    /// Assign a fresh identity for a new ADR, given the refs already present in
    /// the relevant scope. `today` / `id_slug` are passed in so this stays
    /// pure (and unit-testable); schemes that don't need them ignore them.
    pub fn assign(&self, existing: &[AdrRef], title: &str, today: Date, id_slug: &str) -> AdrRef {
        match self {
            // Sequential: global max + 1.
            NamingScheme::Sequential => {
                let max = existing
                    .iter()
                    .filter_map(AdrRef::as_number)
                    .max()
                    .unwrap_or(0);
                AdrRef::Number(max + 1)
            }
            NamingScheme::Date => {
                let base = format!("{}-{}", ymd(today), slugify(title));
                AdrRef::Slug(dedup(base, existing))
            }
            NamingScheme::Uuid => AdrRef::Slug(id_slug.to_string()),
        }
    }

    /// Parse an ADR's identity from its file path. `None` if it can't be
    /// determined under this scheme. (Identity is filename-shaped — the KB
    /// page's `reference:` frontmatter mirrors it; document *content* is never
    /// scanned, so prose mentioning another ADR can't hijack a file's
    /// identity.)
    pub fn parse(&self, path: &Path) -> Option<AdrRef> {
        match self {
            NamingScheme::Sequential => leading_number(path).map(AdrRef::Number),
            // Date identity is the whole filename stem (`YYYYMMDD-title`).
            NamingScheme::Date => stem(path).map(AdrRef::Slug),
            // Uuid identity is just the leading uuid (the filename appends a
            // human title slug after it); split it back off so the parsed ref
            // matches what `assign` produced.
            NamingScheme::Uuid => stem(path)
                .map(|s| s.split('-').next().unwrap_or(&s).to_string())
                .map(AdrRef::Slug),
        }
    }

    /// Parse a user-supplied CLI identifier into a ref under this scheme.
    ///
    /// Numeric schemes accept `9`, `0009`, or `ADR-0009`; slug schemes accept
    /// the filename stem (date) or the uuid / its prefix (uuid), with a trailing
    /// `.md` tolerated. `None` if the input can't be a ref for this scheme.
    pub fn parse_ref(&self, input: &str) -> Option<AdrRef> {
        let t = input.trim();
        if t.is_empty() {
            return None;
        }
        if self.is_numeric() {
            let digits = t
                .strip_prefix("ADR-")
                .or_else(|| t.strip_prefix("adr-"))
                .unwrap_or(t);
            leading_digits(digits).map(AdrRef::Number)
        } else {
            let stem = t.strip_suffix(".md").unwrap_or(t);
            Some(AdrRef::Slug(stem.to_string()))
        }
    }

    /// Whether a stored ref satisfies a query ref (for `find_path_by_ref`).
    /// Exact for every scheme except uuid, where a unique leading prefix of the
    /// uuid is accepted (so the displayed `ADR-<short>` can be typed back).
    pub fn ref_matches(&self, stored: &AdrRef, query: &AdrRef) -> bool {
        match (self, stored, query) {
            (NamingScheme::Uuid, AdrRef::Slug(s), AdrRef::Slug(q)) => {
                !q.is_empty() && s.starts_with(q.as_str())
            }
            _ => stored == query,
        }
    }

    /// The on-disk filename for an ADR with this ref and title.
    pub fn filename(&self, r: &AdrRef, title: &str) -> String {
        match (self, r) {
            (NamingScheme::Date, AdrRef::Slug(s)) => format!("{s}.md"),
            (NamingScheme::Uuid, AdrRef::Slug(s)) => format!("{s}-{}.md", slugify(title)),
            (_, AdrRef::Number(n)) => format!("{n:04}-{}.md", slugify(title)),
            // Defensive: ref/scheme mismatch — name by the slug.
            (_, AdrRef::Slug(s)) => format!("{s}.md"),
        }
    }

    /// How the ref is shown to humans (lists, headings, `adroit show`).
    pub fn display(&self, r: &AdrRef) -> String {
        match r {
            AdrRef::Number(n) => format!("ADR-{n:04}"),
            // Uuid is long; show a short prefix. Date slug is already readable.
            // Take the first 8 *chars* (not bytes) so a crafted non-hex slug can't
            // panic by slicing inside a multibyte char — a real uuid is ASCII hex,
            // so this stays byte-identical for it.
            AdrRef::Slug(s) if matches!(self, NamingScheme::Uuid) => {
                let short: String = s.chars().take(8).collect();
                format!("ADR-{short}")
            }
            AdrRef::Slug(s) => s.clone(),
        }
    }

    /// The H1 heading line for an ADR — also identity-shaped, so it lives here
    /// (consumers / templates route through this instead of hardcoding
    /// `# ADR-NNNN:`). Numeric schemes get `# ADR-NNNN: Title`; slug schemes
    /// (date/uuid) get a plain `# Title` (log4brains-style, identity in the
    /// filename).
    pub fn heading(&self, r: &AdrRef, title: &str) -> String {
        match r {
            AdrRef::Number(n) => format!("# ADR-{n:04}: {title}"),
            AdrRef::Slug(_) => format!("# {title}"),
        }
    }

    /// The label used inside a cross-ADR markdown link `[label](target)`.
    pub fn link_label(&self, r: &AdrRef) -> String {
        self.display(r)
    }

    /// Resolve a supersession reference from the fragment that follows
    /// `Superseded by` / `Supersedes` in a markdown `## Status` region — either a
    /// `[label](target)` link (resolved from the target via [`ref_in_link`]) or a
    /// bare token (`ADR-0009` or a slug, resolved via [`parse_ref`]).
    ///
    /// [`ref_in_link`]: Self::ref_in_link
    /// [`parse_ref`]: Self::parse_ref
    pub fn ref_in_note(&self, fragment: &str) -> Option<AdrRef> {
        if let Some(open) = fragment.find("](") {
            let after = &fragment[open + 2..];
            let target = after.split(')').next().unwrap_or(after);
            return self.ref_in_link(target);
        }
        let token = fragment
            .trim()
            .trim_start_matches('[')
            .split([']', ' ', ')', ','])
            .next()
            .unwrap_or("")
            .trim();
        if token.is_empty() {
            None
        } else {
            self.parse_ref(token)
        }
    }

    /// Extract the ADR ref a relative link target points at (filename-based), so
    /// relink/check can match links to ADRs without knowing the scheme.
    pub fn ref_in_link(&self, target: &str) -> Option<AdrRef> {
        let file = target.split('#').next().unwrap_or(target);
        let name = file.rsplit('/').next().unwrap_or(file);
        let stem = name.strip_suffix(".md").unwrap_or(name);
        if stem.is_empty() {
            return None;
        }
        if self.is_numeric() {
            leading_digits(stem).map(AdrRef::Number)
        } else if matches!(self, NamingScheme::Uuid) {
            // A uuid ADR's identity is the bare uuid; its filename appends a title
            // slug (`{uuid}-{slug}.md`). Split it back off so a link resolves to
            // the ADR (mirrors `parse`). Without this, supersession links never
            // resolve and `check` reports them as broken.
            Some(AdrRef::Slug(
                stem.split('-').next().unwrap_or(stem).to_string(),
            ))
        } else {
            // Date scheme: the whole stem (`YYYYMMDD-title`) is the identity.
            Some(AdrRef::Slug(stem.to_string()))
        }
    }
}

// --- shared helpers (scheme-agnostic) --------------------------------------

/// Kebab-case a title: lowercase, non-alphanumerics → spaces, words joined by
/// `-`. Mirrors the original `store::filename` slug logic.
pub fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// `YYYYMMDD` for a date.
fn ymd(d: Date) -> String {
    format!("{}{:02}{:02}", d.year(), u8::from(d.month()), d.day())
}

/// Make `base` unique among `existing` Slug refs by appending `-2`, `-3`, …
fn dedup(base: String, existing: &[AdrRef]) -> String {
    let taken = |s: &str| existing.iter().any(|r| r.as_slug() == Some(s));
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|i| format!("{base}-{i}"))
        .find(|s| !taken(s))
        .unwrap()
}

/// Leading zero-padded number from a path's filename (`0006-foo.md` → 6).
fn leading_number(path: &Path) -> Option<u32> {
    leading_digits(path.file_name()?.to_str()?)
}

fn leading_digits(s: &str) -> Option<u32> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Filename without the `.md` extension.
fn stem(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    Some(name.strip_suffix(".md").unwrap_or(name).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn date(y: i32, m: Month, d: u8) -> Date {
        Date::from_calendar_date(y, m, d).unwrap()
    }
    fn id_slug() -> &'static str {
        "01hz2x3v4w5t6s7r8q9p0n1m2k"
    }

    #[test]
    fn is_numeric() {
        assert!(NamingScheme::Sequential.is_numeric());
        assert!(!NamingScheme::Date.is_numeric());
        assert!(!NamingScheme::Uuid.is_numeric());
    }

    #[test]
    fn sequential_round_trip() {
        let s = NamingScheme::Sequential;
        let existing = [AdrRef::Number(1), AdrRef::Number(4)];
        assert_eq!(
            s.assign(&existing, "x", date(2026, Month::June, 1), id_slug()),
            AdrRef::Number(5)
        );
        let r = AdrRef::Number(9);
        assert_eq!(
            s.filename(&r, "Adopt Crossplane!"),
            "0009-adopt-crossplane.md"
        );
        assert_eq!(s.display(&r), "ADR-0009");
        assert_eq!(s.link_label(&r), "ADR-0009");
        assert_eq!(
            s.ref_in_link("../accepted/0009-adopt-crossplane.md"),
            Some(AdrRef::Number(9))
        );
        assert_eq!(
            s.parse(Path::new("decisions/0009-x.md")),
            Some(AdrRef::Number(9))
        );
        assert_eq!(s.parse(Path::new("0007-x.md")), Some(AdrRef::Number(7)));
    }

    #[test]
    fn date_scheme_is_collision_free_and_dedups() {
        let s = NamingScheme::Date;
        let r = s.assign(
            &[],
            "Adopt Crossplane",
            date(2026, Month::June, 1),
            id_slug(),
        );
        assert_eq!(r, AdrRef::Slug("20260601-adopt-crossplane".into()));
        assert_eq!(s.filename(&r, "ignored"), "20260601-adopt-crossplane.md");
        assert_eq!(s.display(&r), "20260601-adopt-crossplane");
        assert_eq!(
            s.ref_in_link("../accepted/20260601-adopt-crossplane.md"),
            Some(AdrRef::Slug("20260601-adopt-crossplane".into()))
        );
        assert_eq!(
            s.parse(Path::new("x/20260601-adopt-crossplane.md")),
            Some(AdrRef::Slug("20260601-adopt-crossplane".into()))
        );
        // Same day + title → suffixed, so it never collides.
        let dup = s.assign(
            &[r],
            "Adopt Crossplane",
            date(2026, Month::June, 1),
            id_slug(),
        );
        assert_eq!(dup, AdrRef::Slug("20260601-adopt-crossplane-2".into()));
    }

    #[test]
    fn uuid_scheme() {
        let s = NamingScheme::Uuid;
        let r = s.assign(
            &[],
            "Adopt Crossplane",
            date(2026, Month::June, 1),
            id_slug(),
        );
        assert_eq!(r, AdrRef::Slug(id_slug().into()));
        assert_eq!(
            s.filename(&r, "Adopt Crossplane"),
            format!("{}-adopt-crossplane.md", id_slug())
        );
        assert_eq!(s.display(&r), "ADR-01hz2x3v"); // short prefix
        // Parsing the written filename recovers the *bare* uuid (the title slug
        // after it is dropped), so the parsed ref equals what `assign` produced.
        assert_eq!(
            s.parse(Path::new(
                "x/01hz2x3v4w5t6s7r8q9p0n1m2k-adopt-crossplane.md"
            )),
            Some(r.clone())
        );
        // Addressable by a unique leading prefix of the uuid.
        assert!(s.ref_matches(&r, &AdrRef::Slug("01hz2x3v".into())));
        assert!(!s.ref_matches(&r, &AdrRef::Slug("ffff".into())));
        // A supersession/cross link carries the full `{uuid}-{title}.md` filename;
        // `ref_in_link` must recover the bare uuid identity so the link resolves
        // (otherwise `check` flags the supersession as broken).
        assert_eq!(
            s.ref_in_link("../accepted/01hz2x3v4w5t6s7r8q9p0n1m2k-adopt-crossplane.md"),
            Some(r.clone())
        );
    }

    #[test]
    fn uuid_display_tolerates_multibyte_slug() {
        // Regression (hardening blitz parser fuzz): `display` shortened a uuid slug
        // by slicing the first 8 *bytes*, which panics when byte 8 lands inside a
        // multibyte char (a crafted id / filename). It now takes 8 chars instead.
        let s = NamingScheme::Uuid;
        // A real (hex) uuid still shortens to exactly 8 chars, byte-identical.
        assert_eq!(
            s.display(&AdrRef::Slug("123456789abcdef0".into())),
            "ADR-12345678"
        );
        // A non-hex / multibyte slug must not panic.
        let _ = s.display(&AdrRef::Slug("a𐀀𐀀".into()));
        assert_eq!(s.display(&AdrRef::Slug("éè".into())), "ADR-éè");
    }

    #[test]
    fn ref_in_note_resolves_link_and_bare_token() {
        let seq = NamingScheme::Sequential;
        assert_eq!(
            seq.ref_in_note(" [ADR-0006](../accepted/0006-adopt-adrs.md)"),
            Some(AdrRef::Number(6))
        );
        assert_eq!(seq.ref_in_note(" ADR-0006"), Some(AdrRef::Number(6)));

        let date = NamingScheme::Date;
        assert_eq!(
            date.ref_in_note(" [20260601-x](../accepted/20260601-x.md)"),
            Some(AdrRef::Slug("20260601-x".into()))
        );
        assert_eq!(
            date.ref_in_note(" 20260601-x"),
            Some(AdrRef::Slug("20260601-x".into()))
        );
    }

    #[test]
    fn addr_round_trips_through_parse_ref() {
        let seq = NamingScheme::Sequential;
        let r = AdrRef::Number(9);
        assert_eq!(r.addr(), "9");
        assert_eq!(seq.parse_ref(&r.addr()), Some(r));

        let date = NamingScheme::Date;
        let r = AdrRef::Slug("20260601-x".into());
        assert_eq!(r.addr(), "20260601-x");
        assert_eq!(date.parse_ref(&r.addr()), Some(r));
    }

    #[test]
    fn parse_ref_accepts_human_input() {
        let seq = NamingScheme::Sequential;
        assert_eq!(seq.parse_ref("9"), Some(AdrRef::Number(9)));
        assert_eq!(seq.parse_ref("0009"), Some(AdrRef::Number(9)));
        assert_eq!(seq.parse_ref("ADR-0009"), Some(AdrRef::Number(9)));
        assert_eq!(seq.parse_ref("  12 "), Some(AdrRef::Number(12)));
        assert_eq!(seq.parse_ref("nope"), None);

        let date = NamingScheme::Date;
        assert_eq!(
            date.parse_ref("20260601-adopt-x"),
            Some(AdrRef::Slug("20260601-adopt-x".into()))
        );
        // A trailing `.md` (e.g. tab-completed) is tolerated.
        assert_eq!(
            date.parse_ref("20260601-adopt-x.md"),
            Some(AdrRef::Slug("20260601-adopt-x".into()))
        );
    }

    #[test]
    fn heading_is_identity_shaped() {
        assert_eq!(
            NamingScheme::Sequential.heading(&AdrRef::Number(9), "Adopt X"),
            "# ADR-0009: Adopt X"
        );
        // Slug schemes carry identity in the filename, so the heading is plain.
        assert_eq!(
            NamingScheme::Date.heading(&AdrRef::Slug("20260601-adopt-x".into()), "Adopt X"),
            "# Adopt X"
        );
    }
}
