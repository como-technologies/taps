//! Work-item classes and their goal/sign-off lifecycle (portfolio ADR-0015).
//!
//! Three classes at three goal altitudes — `project` (executive terms,
//! Measure-verified), `story` (behavior specs), `task` (a signed test set) —
//! each a conduit-owned KB page class. The schemas ship with the tool
//! ([`SCHEMAS`]) and the doors register them on first contact, idempotently.
//!
//! The lifecycle is one enum for all three classes; what differs per class
//! is who may drive a transition and what it requires of the item's parent
//! and children. [`check_transition`] is the pure rule table:
//!
//! - **Sign-off flows downhill**: `draft -> ready` is a human seat, and a
//!   story/task cannot be signed off until its parent is signed off.
//! - **Done flows uphill as a precondition, not a cascade**: a task's `done`
//!   belongs to the mechanical merge door alone; a story closes only when
//!   every child is terminal (with at least one done); a project's close is
//!   a human seat again — outcome is gated at the top like intent was.
//! - **Cancel flows downhill like sign-off**: the cancel door cancels an
//!   item's non-terminal children in the same stroke (door behavior — this
//!   checker permits each child's cancel independently).
//! - **The bounce**: `ready/in-progress -> draft` is open to any actor. It
//!   only ever destroys an approval (the door strips the seal), never grants
//!   one, so the hash-mismatch bounce needs no privilege.
//!
//! Writing and stripping the approval block, and pinning the body hash, are
//! the doors' business (checklist item 3 of the rebuild) — this module only
//! decides whether a transition is legal.

use serde::{Deserialize, Serialize};

/// The wiki section conduit's work items live under.
pub const WORK_ROOT: &str = "work";

pub const PROJECT_SCHEMA: &str = include_str!("../schemas/project.json");
pub const STORY_SCHEMA: &str = include_str!("../schemas/story.json");
pub const TASK_SCHEMA: &str = include_str!("../schemas/task.json");

/// The classes the doors register on first contact, in registration order.
pub const SCHEMAS: [(&str, &str); 3] = [
    ("project", PROJECT_SCHEMA),
    ("story", STORY_SCHEMA),
    ("task", TASK_SCHEMA),
];

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Class {
    Project,
    Story,
    Task,
}

impl Class {
    pub const ALL: [Class; 3] = [Class::Project, Class::Story, Class::Task];

    /// The parent class this class hangs under (`None` for the root).
    pub fn parent(self) -> Option<Class> {
        match self {
            Class::Project => None,
            Class::Story => Some(Class::Project),
            Class::Task => Some(Class::Story),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Draft,
    Ready,
    InProgress,
    Done,
    Cancelled,
}

impl Status {
    pub const ALL: [Status; 5] = [
        Status::Draft,
        Status::Ready,
        Status::InProgress,
        Status::Done,
        Status::Cancelled,
    ];

    pub fn is_terminal(self) -> bool {
        matches!(self, Status::Done | Status::Cancelled)
    }

    /// Signed off and live: the altitudes below may build on it.
    pub fn is_signed_open(self) -> bool {
        matches!(self, Status::Ready | Status::InProgress)
    }
}

/// Who is driving the door. The doors know which seat invoked them; the
/// checker makes the privilege rules explicit and testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    /// A human at the seat (sign-off, project close).
    HumanSeat,
    /// A harness session through conduit's tools (PM or execution posture).
    Harness,
    /// The mechanical merge door — the only writer of a task's `done`.
    MergeDoor,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LifecycleError {
    #[error("already {0:?} — setting the current status again is a no-op, refused")]
    SameStatus(Status),
    #[error("{0:?} is terminal — a work item never leaves done/cancelled")]
    Terminal(Status),
    #[error(
        "only a human seat may make a {class:?} {to:?} — the harness neither grants sign-off nor declares a goal achieved"
    )]
    HumanSeatOnly { class: Class, to: Status },
    #[error(
        "a task becomes done only through the mechanical merge door (signed test set green + standing gates)"
    )]
    MergeDoorOnly,
    #[error("the merge door merges tasks — it does not close a {0:?}")]
    NotMergeDoorBusiness(Class),
    #[error(
        "sign-off flows downhill: the parent {parent_class:?} must be signed off (ready/in-progress) first, but is {parent:?}"
    )]
    ParentNotSignedOff {
        parent_class: Class,
        parent: Option<Status>,
    },
    #[error(
        "done flows uphill: {open} child item(s) are still open — close waits for every child to be terminal"
    )]
    ChildrenStillOpen { open: usize },
    #[error(
        "nothing landed: every child is cancelled (or there are none) — cancel this item instead of closing it"
    )]
    NoDoneChildren,
    #[error("invalid transition {from:?} -> {to:?} for a {class:?}")]
    Invalid {
        class: Class,
        from: Status,
        to: Status,
    },
}

/// Check one transition. `parent` is the parent item's status (`None` for a
/// project — and for a story/task whose parent is missing, which can never
/// sign off). `children` are the statuses of every child item (empty for a
/// task, and for a story/project with none yet).
pub fn check_transition(
    class: Class,
    actor: Actor,
    from: Status,
    to: Status,
    parent: Option<Status>,
    children: &[Status],
) -> Result<(), LifecycleError> {
    use Status::*;

    if from == to {
        return Err(LifecycleError::SameStatus(from));
    }
    if from.is_terminal() {
        return Err(LifecycleError::Terminal(from));
    }

    match to {
        // Cancel: any actor, from any non-terminal state. The downhill
        // cascade (cancelling children) is the door's stroke, not a rule
        // here — each child's own cancel passes this same check.
        Cancelled => Ok(()),

        // The bounce: destroys an approval, never grants one — unprivileged.
        Draft => match from {
            Ready | InProgress => Ok(()),
            _ => Err(LifecycleError::Invalid { class, from, to }),
        },

        // Sign-off: human seat, and the parent must already be signed off.
        Ready => {
            if from != Draft {
                return Err(LifecycleError::Invalid { class, from, to });
            }
            if actor != Actor::HumanSeat {
                return Err(LifecycleError::HumanSeatOnly { class, to });
            }
            if let Some(parent_class) = class.parent()
                && !parent.is_some_and(Status::is_signed_open)
            {
                return Err(LifecycleError::ParentNotSignedOff {
                    parent_class,
                    parent,
                });
            }
            Ok(())
        }

        // Claim: any actor, from ready only.
        InProgress => match from {
            Ready => Ok(()),
            _ => Err(LifecycleError::Invalid { class, from, to }),
        },

        // Close: the altitude decides who and on what evidence.
        Done => match class {
            // A task is done when the merge door proves the gate — from
            // in-progress only (it must have been claimed and executed).
            Class::Task => {
                if from != InProgress {
                    return Err(LifecycleError::Invalid { class, from, to });
                }
                if actor != Actor::MergeDoor {
                    return Err(LifecycleError::MergeDoorOnly);
                }
                Ok(())
            }
            // A story/project closes from ready or in-progress once every
            // child is terminal and at least one landed.
            Class::Story | Class::Project => {
                if from != Ready && from != InProgress {
                    return Err(LifecycleError::Invalid { class, from, to });
                }
                match (class, actor) {
                    (Class::Project, a) if a != Actor::HumanSeat => {
                        return Err(LifecycleError::HumanSeatOnly { class, to });
                    }
                    (Class::Story, Actor::MergeDoor) => {
                        return Err(LifecycleError::NotMergeDoorBusiness(class));
                    }
                    _ => {}
                }
                let open = children.iter().filter(|s| !s.is_terminal()).count();
                if open > 0 {
                    return Err(LifecycleError::ChildrenStillOpen { open });
                }
                if !children.contains(&Done) {
                    return Err(LifecycleError::NoDoneChildren);
                }
                Ok(())
            }
        },
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use Actor::*;
    use Class::*;
    use Status::*;

    /// A context under which a transition has the best chance of passing:
    /// signed-off parent, one landed child.
    fn permissive(
        class: Class,
        actor: Actor,
        from: Status,
        to: Status,
    ) -> Result<(), LifecycleError> {
        check_transition(class, actor, from, to, Some(Ready), &[Done])
    }

    #[test]
    fn the_full_allowed_set_and_nothing_else() {
        // Exhaustive: every (class, actor, from, to) under the permissive
        // context, against the exact allowed list.
        let mut allowed = Vec::new();
        for class in Class::ALL {
            for actor in [HumanSeat, Harness, MergeDoor] {
                for from in Status::ALL {
                    for to in Status::ALL {
                        if permissive(class, actor, from, to).is_ok() {
                            allowed.push((class, actor, from, to));
                        }
                    }
                }
            }
        }
        for (class, actor, from, to) in &allowed {
            let ok = match to {
                Cancelled => !from.is_terminal(),
                Draft => matches!(from, Ready | InProgress),
                Ready => *from == Draft && *actor == HumanSeat,
                InProgress => *from == Ready,
                Done => match class {
                    Task => *from == InProgress && *actor == MergeDoor,
                    Story => matches!(from, Ready | InProgress) && *actor != MergeDoor,
                    Project => matches!(from, Ready | InProgress) && *actor == HumanSeat,
                },
            };
            assert!(
                ok,
                "unexpectedly allowed: {class:?} {actor:?} {from:?} -> {to:?}"
            );
        }
        // 3 classes × (cancel: 3 froms × 3 actors) + bounce (2×3) + sign-off
        // (1×1) + claim (1×3) = 3×(9+6+1+3)=57, plus done: task 1, story 2×2,
        // project 2×1 = 7. Total 64 — pinned so a rule change is a loud diff.
        assert_eq!(allowed.len(), 64, "the allowed set changed size");
    }

    #[test]
    fn sign_off_is_a_human_seat() {
        for class in Class::ALL {
            for actor in [Harness, MergeDoor] {
                assert_eq!(
                    permissive(class, actor, Draft, Ready),
                    Err(LifecycleError::HumanSeatOnly { class, to: Ready }),
                    "{class:?} sign-off must refuse {actor:?}"
                );
            }
            assert!(permissive(class, HumanSeat, Draft, Ready).is_ok());
        }
    }

    #[test]
    fn sign_off_flows_downhill() {
        // A story/task cannot sign off under an unsigned, missing, or
        // terminal parent; a project has no parent to wait for.
        for class in [Story, Task] {
            for parent in [None, Some(Draft), Some(Done), Some(Cancelled)] {
                assert_eq!(
                    check_transition(class, HumanSeat, Draft, Ready, parent, &[]),
                    Err(LifecycleError::ParentNotSignedOff {
                        parent_class: class.parent().unwrap(),
                        parent,
                    }),
                    "{class:?} under parent {parent:?}"
                );
            }
            for parent in [Some(Ready), Some(InProgress)] {
                assert!(check_transition(class, HumanSeat, Draft, Ready, parent, &[]).is_ok());
            }
        }
        assert!(
            check_transition(Project, HumanSeat, Draft, Ready, None, &[]).is_ok(),
            "a project signs off with no parent"
        );
    }

    #[test]
    fn task_done_is_the_merge_doors_alone() {
        assert!(permissive(Task, MergeDoor, InProgress, Done).is_ok());
        for actor in [HumanSeat, Harness] {
            assert_eq!(
                permissive(Task, actor, InProgress, Done),
                Err(LifecycleError::MergeDoorOnly)
            );
        }
        // And only from in-progress — an unclaimed task never merges.
        assert_eq!(
            permissive(Task, MergeDoor, Ready, Done),
            Err(LifecycleError::Invalid {
                class: Task,
                from: Ready,
                to: Done
            })
        );
    }

    #[test]
    fn done_flows_uphill_as_a_precondition() {
        // Open children hold the close; all-cancelled children refuse it.
        for class in [Story, Project] {
            assert_eq!(
                check_transition(class, HumanSeat, Ready, Done, None, &[Done, InProgress]),
                Err(LifecycleError::ChildrenStillOpen { open: 1 })
            );
            for children in [&[] as &[Status], &[Cancelled, Cancelled]] {
                assert_eq!(
                    check_transition(class, HumanSeat, Ready, Done, None, children),
                    Err(LifecycleError::NoDoneChildren),
                    "{class:?} with children {children:?}"
                );
            }
            assert!(
                check_transition(class, HumanSeat, Ready, Done, None, &[Done, Cancelled]).is_ok()
            );
        }
    }

    #[test]
    fn project_close_is_a_human_seat_story_close_is_not_the_merge_doors() {
        assert_eq!(
            permissive(Project, Harness, InProgress, Done),
            Err(LifecycleError::HumanSeatOnly {
                class: Project,
                to: Done
            })
        );
        assert!(permissive(Story, Harness, InProgress, Done).is_ok());
        assert_eq!(
            permissive(Story, MergeDoor, InProgress, Done),
            Err(LifecycleError::NotMergeDoorBusiness(Story))
        );
    }

    #[test]
    fn the_bounce_is_unprivileged_and_terminal_states_are_frozen() {
        for class in Class::ALL {
            for actor in [HumanSeat, Harness, MergeDoor] {
                assert!(permissive(class, actor, Ready, Draft).is_ok());
                assert!(permissive(class, actor, InProgress, Draft).is_ok());
            }
            for from in [Done, Cancelled] {
                for to in Status::ALL {
                    if from == to {
                        continue;
                    }
                    assert_eq!(
                        permissive(class, HumanSeat, from, to),
                        Err(LifecycleError::Terminal(from))
                    );
                }
            }
            assert_eq!(
                permissive(class, HumanSeat, Draft, Draft),
                Err(LifecycleError::SameStatus(Draft))
            );
        }
    }

    // ── The shipped schemas stay in sync with this module ──────────────────

    #[test]
    fn schemas_parse_declare_conduit_ownership_and_their_class() {
        for (name, schema) in SCHEMAS {
            let v: serde_json::Value = serde_json::from_str(schema)
                .unwrap_or_else(|e| panic!("schemas/{name}.json is not valid JSON: {e}"));
            assert_eq!(v["x-owner"], "conduit", "{name}: x-owner");
            assert!(
                v["x-wiki-types"][name].is_string(),
                "{name}: x-wiki-types must declare the class"
            );
            assert_eq!(v["additionalProperties"], false, "{name}: closed schema");
        }
    }

    #[test]
    fn schema_status_enums_match_this_lifecycle() {
        let expected: Vec<serde_json::Value> = Status::ALL
            .iter()
            .map(|s| serde_json::to_value(s).unwrap())
            .collect();
        for (name, schema) in SCHEMAS {
            let v: serde_json::Value = serde_json::from_str(schema).unwrap();
            assert_eq!(
                v["properties"]["status"]["enum"].as_array().unwrap(),
                &expected,
                "{name}: status enum drifted from workitem::Status"
            );
        }
    }

    #[test]
    fn task_effort_enum_is_the_tuesday_closed_set() {
        let v: serde_json::Value = serde_json::from_str(TASK_SCHEMA).unwrap();
        let schema_efforts: Vec<&str> = v["properties"]["effort"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap())
            .collect();
        let contract_efforts: Vec<&str> = como_contract::tuesday::EFFORT_LABELS
            .iter()
            .map(|l| l.strip_prefix("effort:").unwrap())
            .collect();
        assert_eq!(
            schema_efforts, contract_efforts,
            "task effort enum drifted from como-contract EFFORT_LABELS"
        );
    }

    #[test]
    fn parent_chain_is_project_story_task() {
        assert_eq!(Project.parent(), None);
        assert_eq!(Story.parent(), Some(Project));
        assert_eq!(Task.parent(), Some(Story));
    }
}
