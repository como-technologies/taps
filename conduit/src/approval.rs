//! The approval seal: hash-pinned, door-only sign-off (portfolio ADR-0015
//! point 5, rebuild item 3).
//!
//! A work-item page is YAML frontmatter plus a markdown body, and the split
//! is the whole design: **the body is the contract, the frontmatter is
//! state**. The seal ([`Seal`]) lives in frontmatter under `approval:` and
//! pins the sha256 of the canonical body — so status transitions, claim
//! metadata, and telemetry never disturb it, while any edit to the signed
//! content provably breaks it.
//!
//! Every operation here is pure text -> text and preserves the body
//! byte-for-byte; only the frontmatter is rewritten. "Door-only" is a
//! layering rule, not a capability this module can grant: [`seal`] is
//! called by the sign-off door alone (a human seat), [`strip`] by the
//! bounce, and [`check`] by any door that requires an intact seal before
//! acting. [`check`] recomputes the hash from the body it is handed — a
//! forged or stale `content_sha256` cannot verify. The bounce itself
//! (forcing status back to draft when a seal is broken) is door business,
//! wired in with the doors (rebuild item 4).
//!
//! Canonical body = everything after the closing `---`, minus leading
//! newlines and trailing whitespace — the same canon the house page
//! round-trip uses, so editor/transport trailing-newline churn never breaks
//! a seal spuriously.

use serde::{Deserialize, Serialize};

use crate::item::{ItemError, assemble, canonical, split};

/// The frontmatter key the seal lives under (mirrored in the three
/// work-item schemas' `approval` property).
pub const APPROVAL_KEY: &str = "approval";

/// The sign-off seal, exactly as the schemas declare it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seal {
    /// The human seat that signed off.
    pub by: String,
    /// RFC 3339 timestamp of sign-off (supplied by the door).
    pub at: String,
    /// sha256 of the canonical body exactly as signed.
    pub content_sha256: String,
}

/// What [`check`] found on a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealState {
    /// A seal is present and the body still hashes to it.
    Intact(Seal),
    /// A seal is present but the body has changed since signing — the
    /// door's cue to bounce the item to draft and strip the seal.
    Broken { seal: Seal, actual_sha256: String },
    /// No seal on the page (a draft, or a bounced item).
    Unsealed,
}

#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    #[error(transparent)]
    Page(#[from] ItemError),
    #[error("invalid frontmatter YAML: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),
    #[error(
        "malformed approval block: {0} — only the sign-off door writes seals, so a malformed one means the page was edited by hand; bounce it"
    )]
    MalformedSeal(String),
    #[error(
        "page already carries an approval seal — re-approval goes through the bounce (strip, revise, sign off again), never an overwrite"
    )]
    AlreadySealed,
}

/// sha256 of the page's canonical body — what a seal pins.
pub fn body_sha256(page: &str) -> Result<String, ApprovalError> {
    let (_, body) = split(page)?;
    Ok(crate::hash::sha256_hex(canonical(body).as_bytes()))
}

/// Sign a page: pin the canonical body and write the seal into the
/// frontmatter. Refuses a page that already carries any seal, intact or
/// broken — an overwrite would let a stale approval launder new content.
pub fn seal(page: &str, by: &str, at: &str) -> Result<String, ApprovalError> {
    let (mut fm, body) = split(page)?;
    if fm.contains_key(APPROVAL_KEY) {
        return Err(ApprovalError::AlreadySealed);
    }
    let seal = Seal {
        by: by.to_string(),
        at: at.to_string(),
        content_sha256: crate::hash::sha256_hex(canonical(body).as_bytes()),
    };
    fm.insert(
        serde_yaml_ng::Value::String(APPROVAL_KEY.into()),
        serde_yaml_ng::to_value(&seal)?,
    );
    Ok(assemble(&fm, body)?)
}

/// Remove the seal (the bounce). Idempotent: a page without one comes back
/// unchanged — the bounce must always succeed.
pub fn strip(page: &str) -> Result<String, ApprovalError> {
    let (mut fm, body) = split(page)?;
    if fm
        .remove(serde_yaml_ng::Value::String(APPROVAL_KEY.into()))
        .is_none()
    {
        return Ok(page.to_string());
    }
    Ok(assemble(&fm, body)?)
}

/// Verify a page's seal against its body, recomputing the hash — the stored
/// value is never trusted on its own.
pub fn check(page: &str) -> Result<SealState, ApprovalError> {
    let (fm, body) = split(page)?;
    let Some(value) = fm.get(APPROVAL_KEY) else {
        return Ok(SealState::Unsealed);
    };
    let seal: Seal = serde_yaml_ng::from_value(value.clone())
        .map_err(|e| ApprovalError::MalformedSeal(e.to_string()))?;
    let actual = crate::hash::sha256_hex(canonical(body).as_bytes());
    if actual == seal.content_sha256 {
        Ok(SealState::Intact(seal))
    } else {
        Ok(SealState::Broken {
            seal,
            actual_sha256: actual,
        })
    }
}

// Page plumbing (canonical/split/assemble) is shared with the item model —
// see crate::item.

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "---\ntitle: Reject unsigned items\ntype: task\nstatus: draft\ncreated: 2026-08-16T20:10:00Z\nstory: 01JBLAKE0000000000000000AB\n---\n\n## Goal\n\nThe door refuses unsigned items.\n\n## Test set\n\n- unit: hash round-trip\n";

    fn signed() -> String {
        seal(PAGE, "mike@thesandmans.com", "2026-08-16T20:15:00Z").unwrap()
    }

    #[test]
    fn seal_then_check_is_intact_and_round_trips_the_seat() {
        let page = signed();
        match check(&page).unwrap() {
            SealState::Intact(s) => {
                assert_eq!(s.by, "mike@thesandmans.com");
                assert_eq!(s.at, "2026-08-16T20:15:00Z");
                assert_eq!(s.content_sha256, body_sha256(PAGE).unwrap());
            }
            other => panic!("expected intact, got {other:?}"),
        }
    }

    #[test]
    fn frontmatter_edits_never_break_the_seal() {
        // The whole design: status transitions and telemetry are state, not
        // contract. Flip the status and add claim metadata post-signing.
        let page = signed()
            .replace("status: draft", "status: in-progress")
            .replace(
                "story: 01JBLAKE0000000000000000AB",
                "story: 01JBLAKE0000000000000000AB\nbranch: work/reject-unsigned\nwork_ms: 640000",
            );
        assert!(
            matches!(check(&page).unwrap(), SealState::Intact(_)),
            "frontmatter is state — editing it must not break the seal"
        );
    }

    #[test]
    fn any_body_edit_breaks_the_seal() {
        let page = signed().replace("hash round-trip", "hash round-trips");
        match check(&page).unwrap() {
            SealState::Broken {
                seal,
                actual_sha256,
            } => {
                assert_ne!(seal.content_sha256, actual_sha256);
                assert_eq!(actual_sha256, body_sha256(&page).unwrap());
            }
            other => panic!("expected broken, got {other:?}"),
        }
    }

    #[test]
    fn trailing_newline_churn_is_not_an_edit() {
        let mut page = signed();
        page.push_str("\n\n");
        assert!(
            matches!(check(&page).unwrap(), SealState::Intact(_)),
            "transport/editor trailing whitespace must not break a seal"
        );
    }

    #[test]
    fn a_forged_hash_cannot_verify() {
        // check() recomputes from the body — a hand-written seal with a
        // wrong hash is Broken no matter what it claims.
        let page = PAGE.replace(
            "status: draft",
            "status: ready\napproval:\n  by: forger\n  at: 2026-08-16T00:00:00Z\n  content_sha256: 0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert!(matches!(check(&page).unwrap(), SealState::Broken { .. }));
    }

    #[test]
    fn sealing_refuses_an_already_sealed_page() {
        let page = signed();
        assert!(matches!(
            seal(&page, "other@seat", "2026-08-17T00:00:00Z"),
            Err(ApprovalError::AlreadySealed)
        ));
        // Even a BROKEN seal must be stripped, never overwritten — a fresh
        // seal over edited content would skip the bounce.
        let edited = page.replace("hash round-trip", "something else");
        assert!(matches!(
            seal(&edited, "other@seat", "2026-08-17T00:00:00Z"),
            Err(ApprovalError::AlreadySealed)
        ));
    }

    #[test]
    fn strip_removes_the_seal_and_is_idempotent() {
        let stripped = strip(&signed()).unwrap();
        assert_eq!(check(&stripped).unwrap(), SealState::Unsealed);
        assert!(!stripped.contains("approval"));
        // Idempotent: the bounce must always succeed.
        assert_eq!(strip(&stripped).unwrap(), stripped);
        assert_eq!(check(PAGE).unwrap(), SealState::Unsealed);
    }

    #[test]
    fn strip_then_reseal_is_the_re_approval_path() {
        let edited = signed().replace("hash round-trip", "hash + bounce round-trip");
        assert!(matches!(check(&edited).unwrap(), SealState::Broken { .. }));
        let bounced = strip(&edited).unwrap();
        let resealed = seal(&bounced, "mike@thesandmans.com", "2026-08-17T09:00:00Z").unwrap();
        assert!(matches!(check(&resealed).unwrap(), SealState::Intact(_)));
    }

    #[test]
    fn sealing_preserves_the_body_and_foreign_frontmatter() {
        let page = signed();
        // Body text intact, byte-for-byte from the canonical form.
        assert!(page.contains("## Goal\n\nThe door refuses unsigned items."));
        assert!(page.ends_with("- unit: hash round-trip\n"));
        // Every original frontmatter key survives, in order, seal appended.
        for key in ["title:", "type:", "status:", "created:", "story:"] {
            assert!(page.contains(key), "{key} lost in the rewrite");
        }
        let title_pos = page.find("title:").unwrap();
        let story_pos = page.find("story:").unwrap();
        let seal_pos = page.find("approval:").unwrap();
        assert!(title_pos < story_pos && story_pos < seal_pos);
    }

    #[test]
    fn malformed_seals_and_missing_frontmatter_are_typed_errors() {
        let bad = PAGE.replace("status: draft", "status: ready\napproval:\n  by: someone");
        assert!(matches!(check(&bad), Err(ApprovalError::MalformedSeal(_))));
        assert!(matches!(
            check("no frontmatter at all\n"),
            Err(ApprovalError::Page(
                crate::item::ItemError::MissingOpenDelimiter
            ))
        ));
        assert!(matches!(
            check("---\ntitle: t\nno close"),
            Err(ApprovalError::Page(
                crate::item::ItemError::MissingCloseDelimiter
            ))
        ));
    }

    #[test]
    fn the_hash_is_of_the_canonical_body_only() {
        // Same body under different frontmatter -> same hash.
        let other_fm = "---\ntitle: Different\ntype: task\nstatus: ready\ncreated: 2026-01-01T00:00:00Z\nstory: x\n---\n\n## Goal\n\nThe door refuses unsigned items.\n\n## Test set\n\n- unit: hash round-trip\n";
        assert_eq!(
            body_sha256(PAGE).unwrap(),
            body_sha256(other_fm).unwrap(),
            "frontmatter must not influence the body hash"
        );
    }
}
