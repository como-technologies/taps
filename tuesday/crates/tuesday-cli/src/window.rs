//! The `--from/--to` range window (ADR-0007): `YYYY-MM` parsing and
//! inclusive month enumeration — the window math behind multi-month
//! reports. Pure and source-free so the year-boundary rule is unit-testable.

use std::fmt;
use std::str::FromStr;

/// One calendar month — the unit of the canonical report (ADR-0004) and of
/// the `--from/--to` range. Ordering is chronological (year, then month).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct YearMonth {
    pub year: u32,
    /// 1-based calendar month, 1-12 (enforced by the `FromStr` parser).
    pub month: u32,
}

impl fmt::Display for YearMonth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

impl FromStr for YearMonth {
    type Err = String;

    /// Parse the strict `YYYY-MM` form (four-digit year, two-digit month):
    /// the one spelling the flags document, so a typo never silently
    /// becomes a different window.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || format!("invalid month `{s}`: expected YYYY-MM, e.g. 2026-03");
        let (y, m) = s.split_once('-').ok_or_else(err)?;
        if y.len() != 4 || m.len() != 2 {
            return Err(err());
        }
        let year: u32 = y.parse().map_err(|_| err())?;
        let month: u32 = m.parse().map_err(|_| err())?;
        if !(1..=12).contains(&month) {
            return Err(format!("invalid month `{s}`: the month must be 01-12"));
        }
        Ok(Self { year, month })
    }
}

/// Every month from `from` through `to` **inclusive**, crossing year
/// boundaries (2025-11..2026-02 is four months). An inverted range is an
/// error, never an empty report.
pub fn months_inclusive(from: YearMonth, to: YearMonth) -> Result<Vec<YearMonth>, String> {
    if from > to {
        return Err(format!(
            "--from {from} is after --to {to}: the range must run forward"
        ));
    }
    let mut months = Vec::new();
    let mut current = from;
    loop {
        months.push(current);
        if current == to {
            break;
        }
        current = if current.month == 12 {
            YearMonth {
                year: current.year + 1,
                month: 1,
            }
        } else {
            YearMonth {
                year: current.year,
                month: current.month + 1,
            }
        };
    }
    Ok(months)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ym(year: u32, month: u32) -> YearMonth {
        YearMonth { year, month }
    }

    #[test]
    fn parses_the_documented_yyyy_mm_form() {
        assert_eq!("2026-03".parse::<YearMonth>(), Ok(ym(2026, 3)));
        assert_eq!("2025-12".parse::<YearMonth>(), Ok(ym(2025, 12)));
        assert_eq!("2026-01".parse::<YearMonth>(), Ok(ym(2026, 1)));
    }

    #[test]
    fn display_round_trips_the_parsed_form() {
        for s in ["2026-03", "2025-12", "0001-01"] {
            assert_eq!(s.parse::<YearMonth>().unwrap().to_string(), s);
        }
    }

    #[test]
    fn rejects_malformed_spellings() {
        for bad in [
            "2026",     // no month
            "2026-",    // empty month
            "-03",      // empty year
            "26-03",    // two-digit year
            "2026-3",   // one-digit month
            "2026-003", // three-digit month
            "2026/03",  // wrong separator
            "month",    // not a date at all
            "",
        ] {
            let err = bad.parse::<YearMonth>().unwrap_err();
            assert!(err.contains("YYYY-MM"), "{bad}: {err}");
        }
    }

    #[test]
    fn rejects_out_of_range_months() {
        for bad in ["2026-00", "2026-13", "2026-99"] {
            let err = bad.parse::<YearMonth>().unwrap_err();
            assert!(err.contains("01-12"), "{bad}: {err}");
        }
    }

    #[test]
    fn ordering_is_chronological() {
        assert!(ym(2025, 12) < ym(2026, 1), "year beats month");
        assert!(ym(2026, 1) < ym(2026, 2));
        assert!(ym(2026, 3) == ym(2026, 3));
    }

    #[test]
    fn single_month_range_is_that_month() {
        assert_eq!(
            months_inclusive(ym(2026, 3), ym(2026, 3)),
            Ok(vec![ym(2026, 3)])
        );
    }

    #[test]
    fn in_year_range_is_inclusive_at_both_ends() {
        assert_eq!(
            months_inclusive(ym(2026, 2), ym(2026, 5)),
            Ok(vec![ym(2026, 2), ym(2026, 3), ym(2026, 4), ym(2026, 5)])
        );
    }

    #[test]
    fn range_crosses_the_year_boundary() {
        // The named year-boundary case: November through February.
        assert_eq!(
            months_inclusive(ym(2025, 11), ym(2026, 2)),
            Ok(vec![ym(2025, 11), ym(2025, 12), ym(2026, 1), ym(2026, 2)])
        );
    }

    #[test]
    fn range_spans_multiple_year_boundaries() {
        let months = months_inclusive(ym(2024, 12), ym(2026, 1)).unwrap();
        assert_eq!(months.len(), 14);
        assert_eq!(months[0], ym(2024, 12));
        assert_eq!(months[1], ym(2025, 1));
        assert_eq!(months[13], ym(2026, 1));
    }

    #[test]
    fn inverted_range_is_an_error_not_an_empty_report() {
        let err = months_inclusive(ym(2026, 2), ym(2025, 11)).unwrap_err();
        assert!(
            err.contains("2026-02") && err.contains("2025-11"),
            "names both ends: {err}"
        );
    }
}
