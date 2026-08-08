# Step 3 — Seed starter content

A blank page is a bad day one. The kit ships
[starter content](../portfolio/starter-content.html) — a glossary and a
working guide, typed and wikilinked — so your fresh space has something
to react to: edit it down, supersede it, or delete it as your own
content arrives.

This step is also your first real authoring pass, and it works the way
everything works from here on: content enters the space only through
the appliance's tools and gates. The starter files sit on *your* side
of the wall (in `~/taps`); watch them enter the only way anything does.

In a hurry, or a purist? Skip to [Step 4](./assess.md) — nothing later
depends on the starter set.

## Grant the session its reach

The seed asks your session to read files from `~/taps` — your side of
the wall — and to drive the appliance's tools without stopping between
pages. The kit's shipped settings are deliberately narrow; a tutorial
rig doesn't have to be. Pre-grant the walk's whole surface — this
overlay is yours (`settings.local.json`), the kit's own settings stay
untouched:

```sh
cat > ~/kb-workspace/.claude/settings.local.json <<'EOF'
{
  "enableAllProjectMcpServers": true,
  "permissions": {
    "additionalDirectories": ["~/taps"],
    "allow": ["mcp__kb"]
  }
}
EOF
```

`additionalDirectories` opens `~/taps` to the session (the starter set
lives there); `mcp__kb` trusts every tool the appliance serves — the
content tools the kit already lists, and the schema operations later
steps lean on. On a production workspace you'd grant narrowly and
answer prompts as they come; this rig is a throwaway, and pre-granting
makes every session in the walk paste-and-go. (Prefer the prompts?
Skip this block — the page still works, you'll just approve as you
watch.)

## Seed

Paste this into your workspace session:

```text
Seed this space from the starter set: read each page under
~/taps/llm-wiki/kit/starter/wiki/ (glossary/ and guides/) and author it
into the myproject space through the wiki tools — wiki_content_write
each file's full content to its matching URI (glossary/<slug>,
guides/<slug>), then wiki_ingest them and run wiki_lint, fixing
anything you introduced until it's clean. This is a seed, not a
rewrite — don't change the pages' content.
```

(Skipped the grant block? Your harness will ask before reading
`~/taps` — that's your side of the wall, so approving is fine.)

Watch what happens: every page passes the same admission gate your own
writes will face — strict frontmatter validation, typed schemas, link
checks. When a gate fails it names its rule, and the kit's instructions
are to fix the page, never the process.

## Verify

```sh
incus exec kb -- su - kb -c 'llm-wiki lint --wiki myproject'
```

Zero errors is the bar. Ask your session for the space's stats while
you're at it — pages indexed, no orphans — and you've watched the gates
pass on real content. Step 4 adds the first artifact that's genuinely
yours: the assessment.
