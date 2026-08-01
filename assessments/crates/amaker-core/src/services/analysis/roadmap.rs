//! Roadmap view: gaps grouped by owner role, plus a flat priority
//! ordering.
//!
//! Priority v1 is a stand-in — `1.0 / effort_midpoint`, with a default
//! effort of 8 hours when unspecified. The M4 narrative layer can
//! re-rank semantically; this ordering at least puts low-effort items
//! first so the deterministic view has a reasonable shape.

use std::collections::BTreeMap;

use serde::Serialize;

use super::gaps::{Gap, GapInventory};

/// Synthetic role key used when a question has no `roles` specified.
const UNOWNED_ROLE: &str = "(unowned)";

/// Default effort in hours used when a gap's question has no
/// `EffortRange`. One work-week is a neutral placeholder that doesn't
/// dominate the priority heuristic in either direction.
const DEFAULT_EFFORT_HOURS: f32 = 40.0;

#[derive(Debug, Clone, Serialize)]
pub struct Roadmap {
    /// Role → gaps owned by that role, in priority order. A gap with
    /// multiple roles appears once under each role key.
    pub by_role: BTreeMap<String, Vec<RoadmapEntry>>,
    /// Every gap, priority-ordered (highest first).
    pub priority_ordered: Vec<RoadmapEntry>,
}

/// A single entry in the roadmap — a copy of the gap plus its
/// computed `priority_score`. Copying is fine here; `Gap` is cheap.
#[derive(Debug, Clone, Serialize)]
pub struct RoadmapEntry {
    pub gap: Gap,
    pub priority_score: f32,
}

pub fn compute_roadmap(inventory: &GapInventory) -> Roadmap {
    // Priority-ordered list (highest first). Stable tie-break on the
    // gap's original traversal order so repeated calls produce the
    // same output.
    let mut priority_ordered: Vec<RoadmapEntry> = inventory
        .gaps
        .iter()
        .cloned()
        .map(|gap| {
            let priority_score = priority_for(&gap);
            RoadmapEntry {
                gap,
                priority_score,
            }
        })
        .collect();
    priority_ordered.sort_by(|a, b| {
        b.priority_score
            .partial_cmp(&a.priority_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Group-by-role. Iterate the priority-ordered list so each role
    // bucket is already sorted.
    let mut by_role: BTreeMap<String, Vec<RoadmapEntry>> = BTreeMap::new();
    for entry in &priority_ordered {
        let keys: Vec<String> = if entry.gap.roles.is_empty() {
            vec![UNOWNED_ROLE.to_string()]
        } else {
            entry.gap.roles.clone()
        };
        for key in keys {
            by_role.entry(key).or_default().push(entry.clone());
        }
    }

    Roadmap {
        by_role,
        priority_ordered,
    }
}

fn priority_for(gap: &Gap) -> f32 {
    let hours = gap
        .effort
        .as_ref()
        .map(|e| e.midpoint())
        .filter(|h| *h > 0.0)
        .unwrap_or(DEFAULT_EFFORT_HOURS);
    // Simple inverse-effort ordering; the narrative layer later in M4
    // can weight with risk semantics.
    1.0 / hours
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Polarity;
    use crate::models::assessment::{Domain, Practice, Question};
    use crate::models::ids::RespondentId;
    use crate::models::{Answer, AnswerValue, Assessment, AssessmentResponse, EffortRange};

    fn gap_with(effort_midpoint: Option<f32>, roles: Vec<&str>) -> Gap {
        // Build a fake assessment with one failing question and read back
        // the gap, then patch the effort/roles to match the test input.
        let mut a = Assessment::new("A".into(), "d".into(), "g".into());
        let mut d = Domain::new("D".into(), "c".into(), "v".into(), "r".into());
        let mut p = Practice::new("P".into(), "c".into(), "v".into(), "r".into());
        let mut q = Question::new("Q".into());
        q.polarity = Polarity::Positive;
        q.roles = roles.into_iter().map(String::from).collect();
        q.effort = effort_midpoint.map(|m| EffortRange::new(m as u32, m as u32));
        p.questions.push(q);
        d.practices.push(p);
        a.domains.push(d);

        let mut r = AssessmentResponse::new(a.id, RespondentId::new(), "v1".to_string());
        let q_id = a.domains[0].practices[0].questions[0].id;
        r.upsert_answer(q_id, Answer::new(AnswerValue::No));
        let inv = super::super::gaps::compute_gaps(&a, &r);
        inv.gaps.into_iter().next().unwrap()
    }

    #[test]
    fn lower_effort_gets_higher_priority() {
        let quick = gap_with(Some(2.0), vec!["security"]);
        let slow = gap_with(Some(80.0), vec!["security"]);
        let inv = GapInventory {
            gaps: vec![slow, quick],
        };
        let rm = compute_roadmap(&inv);
        // The quick one should appear first.
        assert!(rm.priority_ordered[0].priority_score > rm.priority_ordered[1].priority_score);
        assert_eq!(
            rm.priority_ordered[0]
                .gap
                .effort
                .as_ref()
                .unwrap()
                .min_hours,
            2
        );
    }

    #[test]
    fn gaps_without_roles_go_to_unowned() {
        let orphan = gap_with(Some(4.0), vec![]);
        let inv = GapInventory { gaps: vec![orphan] };
        let rm = compute_roadmap(&inv);
        assert!(rm.by_role.contains_key(UNOWNED_ROLE));
        assert_eq!(rm.by_role[UNOWNED_ROLE].len(), 1);
    }

    #[test]
    fn multi_role_gap_appears_under_each() {
        let shared = gap_with(Some(4.0), vec!["security", "platform"]);
        let inv = GapInventory { gaps: vec![shared] };
        let rm = compute_roadmap(&inv);
        assert_eq!(rm.by_role["security"].len(), 1);
        assert_eq!(rm.by_role["platform"].len(), 1);
    }
}
