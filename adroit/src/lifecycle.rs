//! Decision lifecycle rules — which status transitions `set-status` may make.
//!
//! The lifecycle is deliberately narrow: a proposal is decided
//! (`proposed` → `accepted` / `rejected`), and an accepted decision can age
//! out (`accepted` → `deprecated`). `superseded` is never set directly —
//! `supersede` is the only door, because it links both sides' frontmatter in
//! the same stroke and a lone status flip would leave a dangling lifecycle.
//! Terminal states (`rejected`, `deprecated`, `superseded`) don't come back:
//! reopening a decision is a *new* decision that `--relates` to the old one.

use crate::page::Status;

/// Why a requested transition is refused.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleError {
    #[error("a decision becomes superseded through `supersede <new> <old>`, not set-status")]
    UseSupersede,
    #[error("cannot move a decision from {from} to {to}: {hint}")]
    Invalid {
        from: Status,
        to: Status,
        hint: &'static str,
    },
}

/// Check a `set-status` transition. Setting the current status again is a
/// no-op and allowed (idempotent retries must not fail).
pub fn check_transition(from: Status, to: Status) -> Result<(), LifecycleError> {
    if from == to {
        return Ok(());
    }
    if to == Status::Superseded {
        return Err(LifecycleError::UseSupersede);
    }
    match (from, to) {
        (Status::Proposed, Status::Accepted | Status::Rejected) => Ok(()),
        (Status::Accepted, Status::Deprecated) => Ok(()),
        (Status::Proposed, Status::Deprecated) => Err(LifecycleError::Invalid {
            from,
            to,
            hint: "a proposal is decided first (accept or reject it)",
        }),
        (Status::Rejected | Status::Deprecated | Status::Superseded, _) => {
            Err(LifecycleError::Invalid {
                from,
                to,
                hint: "terminal states don't come back — record a new decision \
                       that `--relates` to this one",
            })
        }
        (Status::Accepted, _) => Err(LifecycleError::Invalid {
            from,
            to,
            hint: "an accepted decision can only be deprecated (or superseded \
                   via `supersede`)",
        }),
        // `from == to` and `to == Superseded` both returned above.
        (Status::Proposed, Status::Proposed | Status::Superseded) => {
            unreachable!("handled by the early returns")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Status::*;

    #[test]
    fn the_full_transition_matrix() {
        for from in Status::ALL {
            for to in Status::ALL {
                let allowed = from == to
                    || matches!(
                        (from, to),
                        (Proposed, Accepted | Rejected) | (Accepted, Deprecated)
                    );
                assert_eq!(
                    check_transition(from, to).is_ok(),
                    allowed,
                    "{from} -> {to}"
                );
            }
        }
    }

    #[test]
    fn superseded_is_supersedes_only() {
        for from in [Proposed, Accepted, Rejected, Deprecated] {
            assert_eq!(
                check_transition(from, Superseded),
                Err(LifecycleError::UseSupersede)
            );
        }
        // …except the idempotent no-op.
        assert!(check_transition(Superseded, Superseded).is_ok());
    }
}
