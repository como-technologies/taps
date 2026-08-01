use dioxus::prelude::*;
use std::collections::HashMap;
use tuesday_core::{MonthlyReport, TimeAllocation};

#[component]
pub fn ReportView(report: MonthlyReport) -> Element {
    rsx! {
        div { class: "report",
            if !report.unallocated_prs.is_empty() {
                QualityControl {
                    unallocated_prs: report.unallocated_prs.clone(),
                    organization: report.organization.clone(),
                }
            }
            ReportSummary { report: report.clone() }
            CategoryBreakdown {
                categories: report.category_totals.clone(),
                total_hours: report.total_hours,
            }
            if !report.adr_totals.is_empty() {
                AdrBreakdown {
                    adr_totals: report.adr_totals.clone(),
                    total_hours: report.total_hours,
                }
            }
            HoursByPR {
                allocations: report.allocations.clone(),
                total_hours: report.total_hours,
            }
            DetailedTimeEntries { allocations: report.allocations.clone() }
        }
    }
}

#[component]
fn ReportSummary(report: MonthlyReport) -> Element {
    rsx! {
        div { class: "report-summary",
            h2 { "Report Summary" }
            div { class: "summary-grid",
                div { class: "summary-item",
                    span { class: "label", "Period:" }
                    span { class: "value", "{report.month} {report.year}" }
                }
                div { class: "summary-item",
                    span { class: "label", "Total Hours:" }
                    span { class: "value", "{report.total_hours:.1}" }
                }
                div { class: "summary-item",
                    span { class: "label", "Total Effort Points:" }
                    span { class: "value", "{report.total_effort_points}" }
                }
                div { class: "summary-item",
                    span { class: "label", "Hours per Point:" }
                    span { class: "value", "{report.hours_per_point:.2}" }
                }
                div { class: "summary-item",
                    span { class: "label", "Allocated Hours:" }
                    span { class: "value", "{(report.total_hours - report.unallocated_hours):.1}" }
                }
                div { class: "summary-item",
                    span { class: "label", "Unallocated Hours:" }
                    span { class: if report.unallocated_hours > 0.0 { "value warning" } else { "value" },
                        "{report.unallocated_hours:.1}"
                    }
                }
            }
        }
    }
}

#[component]
fn HoursByPR(allocations: Vec<TimeAllocation>, total_hours: f64) -> Element {
    use std::collections::HashMap;

    // Roll up allocations by PR number
    let mut pr_totals: HashMap<u32, (String, f64)> = HashMap::new();

    for allocation in &allocations {
        let entry = pr_totals
            .entry(allocation.pr_number)
            .or_insert((allocation.pr_title.clone(), 0.0));
        entry.1 += allocation.allocated_hours;
    }

    // Sort by hours descending
    let mut sorted_prs: Vec<(u32, String, f64)> = pr_totals
        .into_iter()
        .map(|(pr_num, (title, hours))| (pr_num, title, hours))
        .collect();
    sorted_prs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

    rsx! {
        div { class: "pr-breakdown",
            h3 { "Hours by Task" }
            div { class: "breakdown-list",
                for (pr_number , pr_title , hours) in sorted_prs {
                    div { class: "breakdown-item",
                        span { class: "pr-name", "#{pr_number}: {pr_title}" }
                        span { class: "pr-hours", "{hours:.1} hours" }
                        div { class: "bar-container",
                            div {
                                class: "bar",
                                style: "width: {(hours / total_hours * 100.0).min(100.0)}%",
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DetailedTimeEntries(allocations: Vec<TimeAllocation>) -> Element {
    rsx! {
        div { class: "allocation-table",
            h3 { "Detailed Time Entries" }
            table {
                thead {
                    tr {
                        th { "Task ID" }
                        th { "Description" }
                        th { "Effort" }
                        th { "Hours" }
                        th { "%" }
                        th { "Categories" }
                    }
                }
                tbody {
                    for allocation in allocations {
                        tr {
                            td { "#{allocation.pr_number}" }
                            td { class: "pr-title", "{allocation.pr_title}" }
                            td { "{allocation.effort_score.value()}" }
                            td { "{allocation.allocated_hours:.1}" }
                            td { "{allocation.percentage_of_total:.1}%" }
                            td {
                                for (i , (category , hours)) in allocation.categories.iter().enumerate() {
                                    if i > 0 {
                                        ", "
                                    }
                                    "{category} ({hours:.1}h)"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CategoryBreakdown(categories: HashMap<String, f64>, total_hours: f64) -> Element {
    let mut sorted_categories: Vec<_> = categories.into_iter().collect();
    sorted_categories.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    rsx! {
        div { class: "category-breakdown",
            h3 { "Hours by Category" }
            div { class: "breakdown-list",
                for (category , hours) in sorted_categories {
                    div { class: "breakdown-item",
                        span { class: "category-name", "{category}:" }
                        span { class: "category-hours", "{hours:.1} hours" }
                        div { class: "bar-container",
                            div {
                                class: "bar",
                                style: "width: {(hours / total_hours * 100.0).min(100.0)}%",
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AdrBreakdown(adr_totals: HashMap<String, f64>, total_hours: f64) -> Element {
    let mut sorted_adrs: Vec<_> = adr_totals.into_iter().collect();
    sorted_adrs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    rsx! {
        div { class: "category-breakdown adr-breakdown",
            h3 { "Hours by ADR" }
            div { class: "breakdown-list",
                for (adr , hours) in sorted_adrs {
                    div { class: "breakdown-item",
                        span { class: "category-name", "{adr}:" }
                        span { class: "category-hours", "{hours:.1} hours" }
                        div { class: "bar-container",
                            div {
                                class: "bar",
                                style: "width: {(hours / total_hours * 100.0).min(100.0)}%",
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn QualityControl(unallocated_prs: Vec<(String, u32, String)>, organization: String) -> Element {
    rsx! {
        div { class: "quality-control-banner warning-banner",
            div { class: "banner-header",
                h3 { "⚠️ Quality Control Alert" }
            }
            div { class: "banner-content",
                p { class: "warning-message",
                    strong { "{unallocated_prs.len()} PRs" }
                    " cannot be included in this analysis because they are missing effort score labels."
                }
                details { class: "affected-prs-details",
                    summary { "View Affected PRs ({unallocated_prs.len()})" }
                    div { class: "pr-list-container",
                        ul {
                            for (repo_name , pr_number , pr_title) in unallocated_prs {
                                li {
                                    a {
                                        href: "https://github.com/{organization}/{repo_name}/pull/{pr_number}",
                                        target: "_blank",
                                        "{repo_name}#{pr_number}"
                                    }
                                    ": {pr_title}"
                                }
                            }
                        }
                    }
                }
                div { class: "action-required",
                    "💡 Action: Add an effort score label (e.g., "
                    code { "effort:3-average" }
                    ") to these PRs to include them in future analysis."
                }
            }
        }
    }
}
