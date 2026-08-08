# Starter content

**What remains here is adroit's decision seed, in transit.** The kit
used to ship wiki starter content too — an ADR-lifecycle glossary and a
review-process guide. It was accurate, well-linked, and owned by the
wrong product: a fresh space's first conversation was about a tool the
reader hadn't met. Content now follows the same ownership boundary as
schemas — each tool contributes its own documentation to a space when it
integrates, and the engine contributes only what every KB benefits from.
The wiki starter set is gone; a fresh space is a blank canvas until its
tools teach it.

## What's here

```
decisions/            legacy-format ADR corpus, seeded via `adroit seed`
  accepted/           5 worked engineering decisions (trunk-based
                      development, ADR governance, dependency pinning
                      and audit, a shared glossary, automated testing —
                      3 carrying stored implementation plans)
  proposed/           an 11-record starter backlog (CI, code reviews,
                      refactoring, testing, monitoring, incident
                      management, feature flags, runbooks, secrets,
                      library versioning)
```

This corpus is adroit's curriculum and relocates to adroit's tree with
the ownership work; it lives here until that lands. Distilled from a
delivered client engagement (content written fresh; no client
material). The starter decisions are examples a team keeps, supersedes,
or replaces — superseding a starter opinion *is* the process working.
Nothing in them assumes a vendor, cloud, language, or CI system.

## Seed a space with it

```sh
adroit seed --from kit/starter/decisions --dir "$DIR"   # fresh ULIDs, healed links
adroit check --dir "$DIR"                               # the semantic gate
```

Decisions ship in adroit's legacy bootstrap format on purpose: `seed`
allocates each space its own page identities and heals the corpus's
internal links, so two teams' spaces never share baked-in IDs.
