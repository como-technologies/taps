# Getting started

This is the hands-on guide to the Como TAPS suite: a step-by-step
tutorial that takes you from a fresh machine to a first trip around the
Assess → Prescribe → Adopt → Measure loop on your own project — software
installed, a knowledge base standing, an assessment run, decisions
recorded, a pull request landed, and the cost measured.

The _why_ behind the loop — the story of the portfolio and how the
stages fit together — is the
[portfolio book](../portfolio/introduction.html). You don't need it to
walk these steps, but it's the ten-minute read that makes them make
sense.

> 🚧 **Dogfood status.** This guide is being verified the honest way: we
> follow it ourselves, step by step, and fix whatever breaks — in the
> guide or in the products. Steps marked 🚧 haven't survived a clean
> walkthrough yet. The markers disappear as the walks complete.

## The journey

| Step                                                  | You will                                             | Tool                          |
| ----------------------------------------------------- | ---------------------------------------------------- | ----------------------------- |
| [0 — A clean room (optional)](./clean-room.md)         | Stand up a disposable container to walk in           | incus                         |
| [1 — Get the software](./install.md)                   | Clone the suite, install the tools                   | `just`                        |
| [2 — Create your knowledge base](./create-your-kb.md)  | Stand up the space every stage writes to             | `llm-wiki`, the authoring kit |
| [3 — Seed starter content](./seed-starter-content.md)  | Give the fresh space working content to react to     | `adroit`, `llm-wiki`          |
| [4 — Assess](./assess.md)                              | Turn what you know into a structured assessment      | amaker                        |
| [5 — Prescribe](./prescribe.md)                        | Seed and accept the decisions the assessment implies | `adroit`                      |
| [6 — Adopt](./adopt.md)                                | Turn an accepted decision into a reviewed, merged PR | conduit                       |
| [7 — Measure](./measure.md)                            | Price the decision and read the team's pulse         | tuesday, pulse                |
| [Around again](./around-again.md)                      | Ask the KB what it all cost; re-assess               | everything                    |

## What you need

Pick where you'll work — dealer's choice:

- **A clean room** ([Step 0](./clean-room.md)) — a disposable incus
  container that snapshots before you start and restores to
  factory-fresh in one command. Your host needs only incus.
- **Straight on your machine** — skip Step 0 and live a little.

Either way, [Step 1](./install.md) installs everything with one
copy-paste block, run where you chose to work. Beyond that, two
optional pieces, deferred to the steps that use them: an **AI
harness** (Claude Code or any MCP client — Step 2's conversational door
to the KB), and an **Anthropic API key or local
[ollama](https://ollama.com)** for the AI-assisted lanes (Step 4 —
every mechanical gate in the tutorial works without a model).

## Conventions

Every command is paste-and-go with two concrete paths:

- **`~/taps`** — where Step 1 clones the suite
- **`~/myproject-kb`** — your knowledge base space, born in Step 2

Prefer different locations or a real project name? Substitute as you
paste — the paths appear literally in every command, so there's nothing
to set up first and nothing to forget.
