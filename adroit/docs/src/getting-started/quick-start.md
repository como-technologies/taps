# Quick Start

adroit stores decisions as **KB decision pages** — YAML frontmatter over a
prose body, one flat `decisions/` directory — inside a **KB space** (a
directory carrying a `wiki.toml`; ADR-0020). See
[ADR Format](../reference/adr-format.md) for the page shape.

## Create a space and your first ADR

Point `--dir` at a KB space. If you don't have one yet, scaffold the minimal
shape (or use `llm-wiki spaces create`):

```sh
mkdir -p my-decisions/wiki/decisions
printf 'name = "my-decisions"\n' > my-decisions/wiki.toml

adroit --dir my-decisions new "Use PostgreSQL for primary datastore"
```

This assigns the next sequential number, scaffolds the page from the `madr`
template, writes it to
`my-decisions/wiki/decisions/0001-use-postgresql-for-primary-datastore.md`,
and opens it in your editor. Use `--no-edit` to skip the editor. A `--dir`
that isn't a space fails loudly with the bootstrap instructions (see
[Using adroit with Your Repo](../usage/your-repo.md)); set `ADROIT_DIR` in a
`.env` to skip `--dir` on every command.

## List decisions

```sh
adroit list
```

## View a decision

```sh
adroit show 1
```

## Accept a decision

```sh
adroit set-status 1 accepted
```

This rewrites the page's `status:` frontmatter in place — the rest of the file
is left byte-identical. (Reading the status back is `adroit status 1`, which
prints `accepted`.)

## Edit a decision

```sh
adroit edit 1
```

## Launch the TUI

Run `adroit` with no subcommand to open the interactive interface (browse,
triage, in-terminal editing):

```sh
adroit
```

Press `?` for the keybinding cheat-sheet, `:` for the fuzzy command palette (every
action by name), and `Enter` to focus the preview. See
[Interactive TUI](../usage/tui.md) for the full keymap, themes, the vi editor, and
the in-TUI AI assists.

## Next steps

- Walk a decision end-to-end — local-only, AI-assisted, or forge — in
  [The ADR Workflow](../usage/workflow.md#worked-workflows).
- Point adroit at a real team repo: [Using adroit with Your Repo](../usage/your-repo.md).
