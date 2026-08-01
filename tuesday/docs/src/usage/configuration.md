# Configuration

tuesday is configured in the web UI; every setting maps onto one field of the
`ReportConfig` struct, which is also the request body of the
[JSON export endpoint](./json-export.md).

| Setting | `ReportConfig` field | Meaning |
|---|---|---|
| Forge | `source` | Which forge provider to read merged PRs from: `github` (default) or `gitea` |
| Base URL | `base_url` | Gitea only — the instance URL (default `http://localhost:3000`, conduit's dogfood forge); GitHub's API base is fixed |
| Monthly hours | `monthly_hours` | The month's total team-hours budget to allocate (default `360.0`) |
| Organization | `organization` | Organization (owner) to read |
| Repositories | `repositories` | One or more repository names within the organization |
| API token | `token` | Forge API token — required for GitHub, optional for Gitea (anonymous read) |
| Year / Month | `year`, `month` | The calendar month window; only PRs **merged** inside it are counted |
| Scaling series | `scaling_series` | How effort scores 1–5 translate into points (below) |

## Authentication

The forge is picked per report (the **Forge** selector on the Reports page);
credentials live on the Settings page, one integration card per forge.

GitHub — two ways in:

- **Personal access token** — paste a token into the GitHub integration card.
- **GitHub App OAuth** — the `src/auth/` flow; sign in and tuesday uses the
  resulting token.

Gitea — exactly one way in, matching the CLI's token handling (no OAuth — a
decided non-feature, ADR-0010):

- **Instance URL + API token** — enter both on the Gitea integration card
  (Gitea: Settings → Applications → Generate New Token; read access
  suffices, Measure never writes). The token may be omitted only for
  anonymous-readable repositories.

In the interactive UI the **browser** calls the forge API directly, so a
self-hosted Gitea must send CORS headers for the app's origin; the headless
[JSON export](./json-export.md) endpoint fetches server-side instead and
needs no CORS.

Report generation dispatches on `source` through the read-only `PrSource`
trait, so the GitHub and Gitea providers sit behind the same seam — see
[Architecture Decisions](../dev/decisions.md) and the
[Dogfood Contract](../dogfood-contract.md).

## Scaling series

Effort scores are relative; the scaling series decides how steeply a "5"
outweighs a "1" when hours are allocated:

| Series | Points for scores 1–5 |
|---|---|
| `Linear` (default) | 1, 2, 3, 4, 5 |
| `Doubling` | 1, 2, 4, 8, 16 |
| `Fibonacci` | 1, 2, 3, 5, 8 |
| `Exponential` | 1, 3, 9, 27, 81 |
| `TShirtSizes` | 1, 3, 5, 8, 13 |
| `Square` | 1, 4, 9, 16, 25 |

## Running the app

```sh
just serve        # dx serve — requires a dioxus-cli matching dioxus 0.7
```

The headless build (no web UI assets) compiles with:

```sh
just build-server # cargo build --no-default-features --features server
```
