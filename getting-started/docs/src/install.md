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

One release build of the whole suite — expect a few minutes the first
time — and every product binary (`llm-wiki`, `adroit`, amaker, conduit,
tuesday, pulse) lands in `~/.cargo/bin`, which rustup already put on
your `PATH`. No exports, nothing to configure in new shells.

## Verify

```sh
just versions
```

One line per product binary — name and version — and a zero exit. The
set comes from the build itself, so this is always the current truth of
what you have installed; any binary missing from `PATH` or misbehaving
fails the command.

Later steps still lean on each product's own justfile for runtime
pieces (amaker's web assets, the demo forge) — but every binary is
built and on `PATH` now, so nothing later waits on a cold build.
