# Step 2 — Create your knowledge base

Everything the loop produces — the assessment's findings, the decisions,
the plans, the measurements — lands in one place:
[the knowledge base](../portfolio/knowledge-base.html). It lives in an
**appliance**: a small container whose only job is holding your spaces
and answering through `llm-wiki serve`. Your AI harness — and, as the
loop grows, every taps tool — reaches a space only through that
server's tools. Nothing else touches a space's files. The validation
gates aren't a convention in this setup; they're the topology.

## Launch the appliance

These incus commands run wherever you are — inside the Step 0 clean
room (the appliance nests inside it) or straight on your machine. No
incus yet? Run [Step 0's Host minimum](./clean-room.md#host-minimum)
first — three lines.

```sh
# the appliance: a container whose only job is your knowledge bases
incus launch images:ubuntu/24.04 kb

# wait for boot: an IP means the network is up, then let systemd settle
until incus list kb -c4 -f csv | grep -q '\.'; do sleep 1; done
incus exec kb -- systemctl is-system-running --wait >/dev/null 2>&1 || true

# an unprivileged user to own the spaces
incus exec kb -- adduser --disabled-password --gecos "" kb

# git: the engine embeds its own git for versioning spaces, but page
# history (wiki_history) shells out to the real thing
incus exec kb -- sh -c 'apt-get update -qq && apt-get install -y -qq git'

# install the engine: the llm-wiki you built in Step 1
incus file push ~/.cargo/bin/llm-wiki kb/usr/local/bin/llm-wiki --mode 0755

# the standing door: serve the HTTP transport as a service. Every
# client — your sessions, tools, a second session inspecting alongside —
# dials this one engine. --any-host: clients dial the appliance by its
# network address, so the localhost-only Host check must be off.
incus exec kb -- sh -c 'cat > /etc/systemd/system/llm-wiki.service <<UNIT
[Unit]
Description=llm-wiki KB appliance
After=network.target

[Service]
User=kb
ExecStart=/usr/local/bin/llm-wiki serve --http --any-host
Restart=on-failure

[Install]
WantedBy=multi-user.target
UNIT
systemctl enable --now llm-wiki'
```

That's the whole appliance: one unprivileged user, one binary (plus
git, its one external dependency), your spaces. It's deliberately boring — `llm-wiki serve` runs identically as
a bare terminal process, a systemd unit, or a pod; a container is just
the deployment this guide walks. The kit's
[README](https://github.com/como-technologies/taps/tree/main/llm-wiki/kit)
covers the variants, including the team-shared HTTP endpoint.

## Create the space

One command, run on the appliance — the operator's console:

```sh
incus exec kb -- su - kb -c 'llm-wiki spaces create ~/spaces/myproject --name myproject --set-default'
```

That provisions the whole thing: the engine's content-class schemas,
strict validation, admission hooks, and search weights. No flags to
remember — a fresh space is born ready. (Born knowing only *content*
classes, deliberately: tools bring their own page classes with them,
registered the first time each one connects.)

## Make your workspace

Your sessions run in an **authoring workspace** — a thin directory
holding only harness config from the shipped kit. No corpus lives
here; one workspace reaches every space the appliance hosts.

```sh
cp -r ~/taps/llm-wiki/kit/workspace ~/kb-workspace
cp -r ~/taps/llm-wiki/kit/skills ~/kb-workspace/.claude/skills

# point the workspace at the appliance's standing door
KB_ADDR=$(incus list kb -c4 -f csv | cut -d' ' -f1)
cat > ~/kb-workspace/.mcp.json <<EOF
{
  "mcpServers": {
    "kb": { "type": "http", "url": "http://$KB_ADDR:8080/mcp" }
  }
}
EOF
```

No harness on this machine yet? (A Step 0 clean room won't have one.)
Claude Code installs in one line, and its first launch handles a
browserless box gracefully — it prints a login URL; open that on any
machine with a browser, sign in, and paste the code back:

```sh
curl -fsSL https://claude.ai/install.sh | bash
```

## Connect and talk

```sh
cd ~/kb-workspace
claude
```

`/mcp` should show the `kb` server connected — over the appliance's
HTTP door. One engine serves every client, so a second session (or a
teammate, or a tool) can work alongside without stepping on this one.
Then talk: search, draft, ask it what spaces it can reach. (The kit
also documents a stdio variant that spawns a private engine per
session — the no-appliance solo path; this guide walks the door.)

One thing you'll notice the workspace *won't* do is shell into the
appliance — the kit's settings deny it. Every write goes through the
tool surface and its admission gates, regardless of who — or what —
authored it. That's not a guardrail bolted on afterwards; it's the
design.

## Verify

```sh
incus exec kb -- su - kb -c 'llm-wiki spaces list && llm-wiki lint --wiki myproject'
```

Your space: registered, default; an empty lint is fine — zero errors is
the bar. (Same answers through the other door: ask your session to list
its spaces and lint `myproject`.)

Provisioned and validated — a blank canvas. Step 3 gives it something
to react to.
