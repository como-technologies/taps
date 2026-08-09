//! The decision identity / naming seam.
//!
//! All scheme-specific logic lives here. The rest of the crate depends only
//! on [`AdrRef`] + [`NamingScheme`] and never branches on the scheme.
//!
//! - [`AdrRef`] is the scheme-agnostic *display / reference* identity — for
//!   humans. Pages address by slug and id (the ULID routing identity, which
//!   is separate and unchanged).
//! - [`NamingScheme`] is the naming default (config: `ADROIT_NAMING`); its
//!   methods encapsulate how each scheme assigns, parses, and displays
//!   references. The substrate only stores the formatted `reference:` —
//!   adroit allocates and writes it.

use time::Date;

/// A scheme-agnostic display / reference identity for a decision.
///
/// `Number` backs the sequential scheme (`ADR-0007`, allocated next-number:
/// max existing in the wiki + 1); `Slug` backs the date (`YYYYMMDD-title`)
/// and uuid schemes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
}

/// How display references are formed — the naming default. A config enum
/// whose methods own every scheme's behavior.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
    strum::Display,
    strum::EnumString,
    clap::ValueEnum,
)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
#[serde(rename_all = "snake_case")]
#[value(rename_all = "snake_case")]
pub enum NamingScheme {
    /// Global zero-padded `ADR-NNNN` (the default). Human-friendly.
    #[default]
    Sequential,
    /// `YYYYMMDD-title` slug (log4brains-style). Collision-free; no number.
    Date,
    /// The page's ULID id as the reference. Collision-free; not human-sortable.
    Uuid,
}

impl NamingScheme {
    /// `true` when this scheme's identity is a single global sequential
    /// number — i.e. the CLI accepts a bare number.
    pub fn is_numeric(&self) -> bool {
        matches!(self, NamingScheme::Sequential)
    }

    /// Assign a fresh reference for a new decision, given the references
    /// already present in the wiki. `today` / `id_slug` are passed in so
    /// this stays pure (and unit-testable); schemes that don't need them
    /// ignore them.
    pub fn assign(&self, existing: &[AdrRef], title: &str, today: Date, id_slug: &str) -> AdrRef {
        match self {
            // Sequential: max existing in the wiki + 1.
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

    /// Parse a user-supplied identifier into a ref under this scheme.
    ///
    /// Numeric schemes accept `9`, `0009`, or `ADR-0009`; slug schemes accept
    /// the reference slug (or the uuid / its prefix). `None` if the input
    /// can't be a ref for this scheme.
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
            Some(AdrRef::Slug(t.to_string()))
        }
    }

    /// Whether a stored ref satisfies a query ref. Exact for every scheme
    /// except uuid, where a unique leading prefix of the uuid is accepted
    /// (so the displayed `ADR-<short>` can be typed back).
    pub fn ref_matches(&self, stored: &AdrRef, query: &AdrRef) -> bool {
        match (self, stored, query) {
            (NamingScheme::Uuid, AdrRef::Slug(s), AdrRef::Slug(q)) => {
                !q.is_empty() && s.starts_with(q.as_str())
            }
            _ => stored == query,
        }
    }

    /// The page-slug stem for a decision with this ref and title (the page
    /// lives at `decisions/<stem>`).
    pub fn stem(&self, r: &AdrRef, title: &str) -> String {
        match (self, r) {
            (NamingScheme::Date, AdrRef::Slug(s)) => s.clone(),
            (NamingScheme::Uuid, AdrRef::Slug(s)) => format!("{s}-{}", slugify(title)),
            (_, AdrRef::Number(n)) => format!("{n:04}-{}", slugify(title)),
            // Defensive: ref/scheme mismatch — name by the slug.
            (_, AdrRef::Slug(s)) => s.clone(),
        }
    }

    /// How the ref is shown to humans (lists, `show`).
    pub fn display(&self, r: &AdrRef) -> String {
        match r {
            AdrRef::Number(n) => format!("ADR-{n:04}"),
            // Uuid is long; show a short prefix. Take the first 8 *chars*
            // (not bytes) so a crafted non-hex slug can't panic by slicing
            // inside a multibyte char. Date slugs are already readable.
            AdrRef::Slug(s) if matches!(self, NamingScheme::Uuid) => {
                let short: String = s.chars().take(8).collect();
                format!("ADR-{short}")
            }
            AdrRef::Slug(s) => s.clone(),
        }
    }
}

// --- shared helpers (scheme-agnostic) --------------------------------------

/// Kebab-case a title: lowercase, non-alphanumerics → spaces, words joined
/// by `-`.
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

fn leading_digits(s: &str) -> Option<u32> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
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
    fn sequential_allocates_next_number() {
        let s = NamingScheme::Sequential;
        let existing = [AdrRef::Number(1), AdrRef::Number(4)];
        assert_eq!(
            s.assign(&existing, "x", date(2026, Month::June, 1), id_slug()),
            AdrRef::Number(5)
        );
        assert_eq!(
            s.assign(&[], "x", date(2026, Month::June, 1), id_slug()),
            AdrRef::Number(1)
        );
        let r = AdrRef::Number(9);
        assert_eq!(s.stem(&r, "Adopt Crossplane!"), "0009-adopt-crossplane");
        assert_eq!(s.display(&r), "ADR-0009");
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
        assert_eq!(s.stem(&r, "ignored"), "20260601-adopt-crossplane");
        assert_eq!(s.display(&r), "20260601-adopt-crossplane");
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
            s.stem(&r, "Adopt Crossplane"),
            format!("{}-adopt-crossplane", id_slug())
        );
        assert_eq!(s.display(&r), "ADR-01hz2x3v"); // short prefix
        // Addressable by a unique leading prefix of the uuid.
        assert!(s.ref_matches(&r, &AdrRef::Slug("01hz2x3v".into())));
        assert!(!s.ref_matches(&r, &AdrRef::Slug("ffff".into())));
    }

    #[test]
    fn uuid_display_tolerates_multibyte_slug() {
        // `display` takes the first 8 chars, not bytes: a crafted non-hex
        // slug must not panic by slicing inside a multibyte char.
        let s = NamingScheme::Uuid;
        assert_eq!(
            s.display(&AdrRef::Slug("123456789abcdef0".into())),
            "ADR-12345678"
        );
        let _ = s.display(&AdrRef::Slug("a𐀀𐀀".into()));
        assert_eq!(s.display(&AdrRef::Slug("éè".into())), "ADR-éè");
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
    }

    #[test]
    fn slugify_flattens_punctuation() {
        assert_eq!(slugify("Adopt Crossplane!"), "adopt-crossplane");
        assert_eq!(slugify("KB: the state store"), "kb-the-state-store");
    }
}
