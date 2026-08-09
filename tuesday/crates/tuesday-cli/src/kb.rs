//! `--kb`: emit each month's report as a `measure-report` typed page into a
//! wiki (portfolio#7 wave 4; the class ships in llm-wiki's Como
//! schema library). tuesday is a structured writer in the kb-spec sense: it
//! writes the typed page into `<content_root>/measures/` and stops —
//! validation is the wiki's admission gate (`llm-wiki ingest`), and
//! committing stays with the caller. The page is **deterministic**: same
//! forge data and arguments, byte-identical bytes (maps render sorted; no
//! emission timestamp), so a re-run converges instead of churning history.

use std::path::{Path, PathBuf};

use crate::window::YearMonth;
use tuesday_core::MonthlyReport;

/// Render the `measure-report` page for one month. Pure and deterministic.
pub fn page(period: YearMonth, report: &MonthlyReport, source: &str, repos: &[String]) -> String {
    let owner = &report.organization;
    let allocated = report.total_hours - report.unallocated_hours;
    let mut adr: Vec<(&String, &f64)> = report.adr_totals.iter().collect();
    adr.sort_by(|a, b| a.0.cmp(b.0));
    let mut categories: Vec<(&String, &f64)> = report.category_totals.iter().collect();
    categories.sort_by(|a, b| a.0.cmp(b.0));

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!(
        "title: \"Capacity report — {} {}\"\n",
        yaml_safe(owner),
        period
    ));
    out.push_str("type: measure-report\n");
    out.push_str("status: active\n");
    out.push_str(&format!(
        "summary: \"{:.1} of {:.1} team hours allocated across {} merged PR(s) in {}; {} decision(s) attributed.\"\n",
        allocated,
        report.total_hours,
        report.allocations.len(),
        period,
        adr.len(),
    ));
    out.push_str(&format!("period: \"{period}\"\n"));
    out.push_str("instrument: tuesday\n");
    out.push_str(&format!("source: \"{}:{}\"\n", source, yaml_safe(owner)));
    out.push_str(&format!(
        "repos: [{}]\n",
        repos
            .iter()
            .map(|r| format!("\"{}\"", yaml_safe(r)))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.push_str(&format!("total_hours: {:.1}\n", report.total_hours));
    out.push_str(&format!(
        "unallocated_hours: {:.1}\n",
        report.unallocated_hours
    ));
    if !adr.is_empty() {
        out.push_str("adr_hours:\n");
        for (reference, hours) in &adr {
            out.push_str(&format!("  {reference}: {hours:.1}\n"));
        }
    }
    out.push_str("---\n\n");

    out.push_str(&format!(
        "Mechanical capacity report emitted by tuesday from merged-PR effort\n\
         (source `{}:{}`, repos {}). Attribution follows the allocation\n\
         ruling: a merged PR's **full** allocated hours are credited to the\n\
         decision named by its `adr:*` label — attribution answers \"what did\n\
         this decision cost?\", so it is never split the way categories are.\n",
        source,
        owner,
        repos
            .iter()
            .map(|r| format!("`{r}`"))
            .collect::<Vec<_>>()
            .join(", "),
    ));

    out.push_str("\n## By decision\n\n");
    if adr.is_empty() {
        out.push_str("No merged PR carried an `adr:*` label this month.\n");
    } else {
        out.push_str("| Decision | Hours |\n|---|---|\n");
        for (reference, hours) in &adr {
            out.push_str(&format!("| {reference} | {hours:.1} |\n"));
        }
    }

    out.push_str("\n## By category\n\n");
    if categories.is_empty() {
        out.push_str("No merged PR carried a category label this month.\n");
    } else {
        out.push_str("| Category | Hours |\n|---|---|\n");
        for (category, hours) in &categories {
            out.push_str(&format!("| {category} | {hours:.1} |\n"));
        }
    }

    out.push_str(&format!(
        "\n## Unallocated\n\n{:.1} hour(s) from {} merged PR(s) carrying no effort score.\n",
        report.unallocated_hours,
        report.unallocated_prs.len(),
    ));
    out
}

/// Write one page per month into `<wiki>/<content_root>/measures/<owner>-<YYYY-MM>.md`.
/// The target must be a wiki (a directory holding `wiki.toml`) — the
/// same hard-error convention adroit uses, naming the bootstrap. The
/// content directory comes from `wiki.toml`'s `content_root` (default
/// `content`).
pub fn write_pages(
    wiki: &Path,
    owner: &str,
    pages: &[(YearMonth, String)],
) -> Result<Vec<PathBuf>, String> {
    let wiki_toml = wiki.join("wiki.toml");
    if !wiki_toml.is_file() {
        return Err(format!(
            "{} is not a wiki (no wiki.toml): create one with `llm-wiki admin create` \
             (or scaffold wiki.toml + content/) and re-run",
            wiki.display()
        ));
    }
    let content_root = std::fs::read_to_string(&wiki_toml)
        .ok()
        .and_then(|raw| raw.parse::<toml::Table>().ok())
        .and_then(|t| {
            t.get("content_root")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .unwrap_or_else(|| "content".into());
    let measures = wiki.join(content_root).join("measures");
    std::fs::create_dir_all(&measures)
        .map_err(|e| format!("creating {}: {e}", measures.display()))?;
    let mut written = Vec::with_capacity(pages.len());
    for (period, content) in pages {
        let path = measures.join(format!("{owner}-{period}.md"));
        std::fs::write(&path, content).map_err(|e| format!("writing {}: {e}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

/// Minimal YAML double-quoted-string safety for identifiers (owners, repo
/// names): escape backslashes and quotes. Everything tuesday interpolates
/// into quoted scalars routes through here.
fn yaml_safe(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn report() -> MonthlyReport {
        let mut r = MonthlyReport::new("June".into(), 2026, 360.0, "como".into());
        r.adr_totals = HashMap::from([("ADR-0003".into(), 132.5), ("ADR-0001".into(), 40.0)]);
        r.category_totals = HashMap::from([("feature".into(), 100.0)]);
        r.unallocated_hours = 12.0;
        r
    }

    fn period() -> YearMonth {
        YearMonth {
            year: 2026,
            month: 6,
        }
    }

    #[test]
    fn page_is_typed_sorted_and_carries_the_attribution() {
        let p = page(period(), &report(), "gitea", &["conduit-dogfood".into()]);
        assert!(p.starts_with("---\n"), "{p}");
        assert!(p.contains("type: measure-report\n"), "{p}");
        assert!(p.contains("period: \"2026-06\"\n"), "{p}");
        assert!(p.contains("instrument: tuesday\n"), "{p}");
        assert!(p.contains("  ADR-0003: 132.5\n"), "{p}");
        assert!(p.contains("| ADR-0003 | 132.5 |"), "{p}");
        // Sorted: ADR-0001 renders before ADR-0003, in map and table alike.
        assert!(
            p.find("ADR-0001").unwrap() < p.find("ADR-0003").unwrap(),
            "{p}"
        );
        // No emission timestamp: the page depends only on its inputs.
        assert!(!p.contains("last_updated"), "{p}");
    }

    #[test]
    fn page_render_is_byte_deterministic() {
        // HashMap iteration order must never leak into the page.
        let a = page(period(), &report(), "gitea", &["r".into()]);
        for _ in 0..16 {
            assert_eq!(a, page(period(), &report(), "gitea", &["r".into()]));
        }
    }

    #[test]
    fn write_pages_requires_a_wiki_and_converges() {
        let tmp = tempfile::tempdir().unwrap();
        // Not a wiki: hard error naming the bootstrap.
        let err = write_pages(tmp.path(), "como", &[]).unwrap_err();
        assert!(err.contains("not a wiki"), "{err}");
        assert!(err.contains("llm-wiki admin create"), "{err}");

        std::fs::write(tmp.path().join("wiki.toml"), "name = \"t\"\n").unwrap();
        let body = page(period(), &report(), "gitea", &["r".into()]);
        let pages = vec![(period(), body.clone())];
        let written = write_pages(tmp.path(), "como", &pages).unwrap();
        assert_eq!(written.len(), 1);
        assert!(written[0].ends_with("content/measures/como-2026-06.md"));
        assert_eq!(std::fs::read_to_string(&written[0]).unwrap(), body);
        // Re-run: byte-identical, no churn.
        write_pages(tmp.path(), "como", &pages).unwrap();
        assert_eq!(std::fs::read_to_string(&written[0]).unwrap(), body);
    }

    #[test]
    fn write_pages_honors_custom_content_root() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("wiki.toml"),
            "name = \"t\"\ncontent_root = \"pages\"\n",
        )
        .unwrap();
        let body = page(period(), &report(), "gitea", &["r".into()]);
        let written = write_pages(tmp.path(), "como", &[(period(), body)]).unwrap();
        assert!(written[0].ends_with("pages/measures/como-2026-06.md"));
    }
}
