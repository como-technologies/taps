# Step 1 — Get the software

The suite is distributed as source, by decision: one repository, one Cargo
workspace, every product inside. A single clone is the whole thing.

Everything from here on happens where you chose to work — inside your
[clean room](./clean-room.md), or right on your machine.

## Prerequisites

One block, copy-paste and done. Ubuntu-compatible for now (other
platforms:
[issue 47](https://github.com/como-technologies/taps/issues/47)):

```sh
# System packages — compilers and libs the suite builds with.
sudo apt-get update && sudo apt-get install -y \
    git curl build-essential pkg-config libssl-dev

# Rust — the workspace pins its exact toolchain; rustup honors the pin
# automatically on first build.
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"

# just — the task runner behind every command in this guide.
cargo install --locked just
```

(Step 6's throwaway forge brings its own runtime requirement — that
step names it when you get there.)

> 🚧 **Unverified.** The walk runs this block on a fresh machine and
> trues up the package list.

## Clone

Already cloned from a local checkout in
[Step 0](./clean-room.md#working-from-a-local-checkout)? Skip the
`git clone` — you have `~/taps`.

```sh
git clone https://github.com/como-technologies/taps.git ~/taps
cd ~/taps
```

The workspace pins its Rust toolchain in `rust-toolchain.toml` — the first
`cargo` invocation fetches the right version automatically.

## Build and install

```sh
just install
```

One shared release build, and the suite's KB command-line pair —
`llm-wiki` (the knowledge base) and `adroit` (decision records) — lands
in `~/.cargo/bin`, which rustup already put on your `PATH`. No exports,
nothing to configure in new shells.

## Verify

```sh
llm-wiki --version
adroit --version
```

Both should print a version and exit cleanly.

The remaining products — amaker (Step 4), conduit (Step 6), tuesday and
pulse (Step 7) — build via their own justfiles at the steps that use
them, so nothing blocks you here.
