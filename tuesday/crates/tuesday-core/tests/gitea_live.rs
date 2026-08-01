//! Live integration test against conduit's throwaway dogfood forge.
//!
//! Skipped unless `TUESDAY_E2E_GITEA=1`. Run it with (from your conduit
//! checkout — `${COMO_CONDUIT_DIR:-../conduit}`):
//!
//! ```sh
//! cd ../conduit && just forge-up   # gitea/gitea:1.24 on localhost:3000
//! TUESDAY_E2E_GITEA=1 cargo test -p tuesday-core --test gitea_live
//! cd ../conduit && just forge-down # destroys container AND volume
//! ```
//!
//! `forge-up` provisions org `como` / repo `conduit-dogfood` and the
//! contract labels, and mints tokens into conduit's gitignored
//! `.secrets/`. tuesday reads with the reviewer token (Measure is
//! read-only; the path is documented by docs/src/dogfood-contract.md:
//! `COMO_CONDUIT_DIR` names the conduit checkout, the sibling is the
//! default — the token itself is a runtime secret, never cloned).
//!
//! A fresh volume has NO merged PRs. Seed the contract-shaped pair this
//! test expects (one merged, one closed-without-merging) from the conduit
//! repo root — these are the exact commands used to seed the recorded
//! fixtures in tests/fixtures/gitea_*.json:
//!
//! ```sh
//! BOT=$(cat .secrets/conduit-bot.token)
//! REV=$(cat .secrets/reviewer.token)
//! API="http://localhost:3000/api/v1/repos/como/conduit-dogfood"
//!
//! # 1. the adr:* label (forge-up pre-creates only effort:* and conduit:*)
//! curl -fsS -X POST -H "Authorization: token $BOT" -H 'Content-Type: application/json' \
//!   -d '{"name":"adr:ADR-0001","color":"006b75","description":"implements ADR-0001"}' \
//!   "$API/labels"
//!
//! # 2. branch + commit via the contents API (contract-shaped message)
//! curl -fsS -X POST -H "Authorization: token $BOT" -H 'Content-Type: application/json' \
//!   -d "{\"content\":\"$(printf 'tuesday e2e seed file\n' | base64)\",
//!        \"new_branch\":\"conduit/adr-0001/tuesday-e2e-seed\",
//!        \"message\":\"[ADR-0001] Seed merged PR for tuesday e2e\n\nAdr-Reference: ADR-0001\"}" \
//!   "$API/contents/demo/tuesday-e2e-seed.md"
//!
//! # 3. the PR, title-prefixed and trailer-carrying  -> note its number N
//! curl -fsS -X POST -H "Authorization: token $BOT" -H 'Content-Type: application/json' \
//!   -d '{"title":"[ADR-0001] Seed merged PR for tuesday e2e",
//!        "head":"conduit/adr-0001/tuesday-e2e-seed","base":"main",
//!        "body":"Seeds one contract-shaped merged PR so tuesday can measure it.\n\nAdr-Reference: ADR-0001"}' \
//!   "$API/pulls"
//!
//! # 4. label it (PRs are issues; POST appends, keeping existing labels) —
//! #    label ids from: curl -fsS -H "Authorization: token $BOT" "$API/labels?limit=50"
//! curl -fsS -X POST -H "Authorization: token $BOT" -H 'Content-Type: application/json' \
//!   -d '{"labels":[<id of effort:1-super-quick>, <id of adr:ADR-0001>]}' \
//!   "$API/issues/N/labels"
//!
//! # 5. merge as the reviewer (the human gate)
//! curl -fsS -X POST -H "Authorization: token $REV" -H 'Content-Type: application/json' \
//!   -d '{"Do":"merge"}' "$API/pulls/N/merge"
//!
//! # 6. (negative case) repeat 2-3 on another branch, then close unmerged:
//! curl -fsS -X PATCH -H "Authorization: token $BOT" -H 'Content-Type: application/json' \
//!   -d '{"state":"closed"}' "$API/pulls/M"
//! ```

use chrono::Datelike;
use tuesday_core::{GiteaSource, PrSource};

const BASE_URL: &str = "http://localhost:3000";
const OWNER: &str = "como";
const REPO: &str = "conduit-dogfood";

/// The reviewer token, at the dogfood contract's documented path:
/// `${COMO_CONDUIT_DIR:-../conduit}/.secrets/reviewer.token` — the env
/// knob first (the suite resolution convention), then the sibling
/// checkout resolved against `CARGO_MANIFEST_DIR`.
fn reviewer_token() -> String {
    let path = match std::env::var("COMO_CONDUIT_DIR") {
        Ok(dir) if !dir.trim().is_empty() => std::path::Path::new(&dir)
            .join(".secrets")
            .join("reviewer.token"),
        _ => std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../conduit/.secrets/reviewer.token"
        )),
    };
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| {
            panic!(
                "reviewer token at {} (run `just forge-up` in the conduit checkout, \
                 or point COMO_CONDUIT_DIR at it): {e}",
                path.display()
            )
        })
        .trim()
        .to_string()
}

#[tokio::test(flavor = "current_thread")]
async fn live_forge_merged_pr_comes_through_with_labels_and_body() {
    if std::env::var("TUESDAY_E2E_GITEA").as_deref() != Ok("1") {
        eprintln!("skipping live Gitea test: set TUESDAY_E2E_GITEA=1 to run");
        return;
    }

    let source = GiteaSource::new(BASE_URL, Some(reviewer_token()));

    // The reviewer is a repo collaborator, not an org member, so
    // list_orgs() legitimately returns [] for this identity — assert it
    // answers, not what it contains.
    source.list_orgs().await.expect("GET /user/orgs answers");

    let repos = source
        .list_repos(OWNER)
        .await
        .expect("GET /orgs/{org}/repos");
    assert!(
        repos.iter().any(|r| r == REPO),
        "org {OWNER} should expose {REPO}, got {repos:?}"
    );

    // The seed PR was merged moments after forge-up, so the current UTC
    // month is the right window (the dogfood contract's month-boundary
    // caveat is why this is explicit, never a default).
    let now = chrono::Utc::now();
    let merged = source
        .fetch_merged_prs(OWNER, REPO, now.year() as u32, now.month())
        .await
        .expect("GET /repos/{owner}/{repo}/pulls?state=closed");

    let seed = merged
        .iter()
        .find(|pr| pr.title == "[ADR-0001] Seed merged PR for tuesday e2e")
        .unwrap_or_else(|| panic!("seeded merged PR not in window, got {merged:?}"));

    // Labels arrive intact for the calculator (effort:* / adr:*).
    assert!(
        seed.labels.iter().any(|l| l == "effort:1-super-quick"),
        "effort label missing: {:?}",
        seed.labels
    );
    assert!(
        seed.labels.iter().any(|l| l == "adr:ADR-0001"),
        "adr label missing: {:?}",
        seed.labels
    );

    // Body intact, trailer and all (the adr-label fallback path).
    let body = seed.body.as_deref().expect("PR body present");
    assert!(
        body.ends_with("Adr-Reference: ADR-0001"),
        "Adr-Reference trailer mangled: {body:?}"
    );

    assert!(seed.url.starts_with(BASE_URL), "url is the forge html_url");

    // The closed-but-unmerged "[ADR-0001] Abandoned spike" PR must NOT be
    // reported: state=closed includes it, into_merged drops it.
    assert!(
        merged
            .iter()
            .all(|pr| !pr.title.contains("Abandoned spike")),
        "closed-unmerged PR leaked into the merged set: {merged:?}"
    );
}
