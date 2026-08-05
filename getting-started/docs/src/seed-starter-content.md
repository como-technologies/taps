# Step 3 — Seed starter content

A blank page is a bad day one. The kit ships
[starter content](../portfolio/starter-content.html) — a real decision
corpus plus a glossary and guides — so your fresh space starts with
working content to react to: edit it down, supersede it, or delete it as
your own content arrives.

In a hurry, or a purist? Skip to [Step 4](./assess.md) — nothing later
depends on the starter set.

## Seed

```sh
adroit seed --from ~/taps/llm-wiki/kit/starter/decisions --dir ~/myproject-kb
cp -r ~/taps/llm-wiki/kit/starter/wiki/. ~/myproject-kb/wiki/
cd ~/myproject-kb
llm-wiki ingest . --wiki myproject      # strict admission gate
```

`seed` bootstraps the kit's legacy-format decision corpus into typed KB
pages with fresh identities; the ingest then runs every page through the
same admission gate your own writes will face.

## Verify

```sh
adroit check --dir ~/myproject-kb    # the decision corpus is semantically sound
llm-wiki lint --wiki myproject       # zero errors is the bar
```

> 🚧 **Unverified.** The walk confirms these commands and the zero-error
> bar on a fresh seed.

Both clean means the space holds validated, typed, working content — and
you've watched the gates pass on real pages. Step 4 adds the first
artifact that's genuinely yours: the assessment.
