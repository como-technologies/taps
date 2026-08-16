# Step 3 — Assess

The loop opens with evidence: a structured maturity assessment of your
project, not a gut feeling. [amaker](../portfolio/loop/assess.html) is
the assessment tool — and the first tool in this guide that reaches your
KB the way every tool does: over the appliance's transport. amaker owns
two page classes, `assessment` and `assessment-report`. Your wiki has
never heard of them; it learns them the moment amaker first connects,
and only amaker writes them.

## Stand up amaker locally

You'll need an Anthropic API key — create one at
[platform.claude.com/settings/keys](https://platform.claude.com/settings/keys)
(sign-up is free; usage is pay-as-you-go).

```sh
cd ~/taps/assessments
cp .env.example .env        # put your ANTHROPIC_API_KEY in it
just run                    # builds, starts author/assess/analyze, opens the browser
```

Three services come up — author (`:3000`), respond (`:3001`), analyze
(`:3002`) — writing to a local `./data` directory. Ctrl-C stops them all.

> **In the clean room?** The container can't open your host's browser —
> `just run` says so and keeps serving, but it binds loopback, invisible
> from outside. Use `just run-exposed` instead (same stack, bound
> `0.0.0.0`) and browse from your machine: <http://walk.local:3000>,
> `:3001`, `:3002` — the mDNS baked in at
> [Step 0](./clean-room.md#create-the-container) does the naming.

> The hosted amaker instances are gated to Como's own organization —
> as a new user you run it locally, which also keeps your material on
> your machine.

## amaker already knows where the KB is

Nothing to configure: amaker reads the suite pair (`KB_URL` /
`KB_WIKI`) you wrote once in Step 2 (`~/.config/taps/env`), through
the same discovery order every taps tool uses. Its own `.env` holds
only what's amaker's — your API key. (Aiming this one tool at a
different appliance? Add the pair to that `.env`; a tool-side `.env`
outranks the user file.)

## Hand your session the tool

The three web apps are *your* seats — browser, human judgment. Your
session gets amaker another way: the `amaker` binary you built in
Step 1 serves its whole command surface over MCP (`amaker mcp`), so the
agent drives the same commands a terminal would — validate, import,
status, publish — as typed tools, never through a shell.

Your current terminal is busy holding the three apps up — leave it
running. Open a fresh one where you work (clean room: `incus exec
walk -- su - dev` from the host, same as Step 0); the rest of this
step lives there. Paste this block — it rewrites your workspace's
`.mcp.json` whole: the same `kb` entry you wrote in Step 2, plus the
new `amaker` one:

```sh
KB_ADDR=$(incus list kb -c4 -f csv | cut -d" " -f1)
cat > ~/kb-workspace/.mcp.json <<EOF
{
  "mcpServers": {
    "kb": { "type": "http", "url": "http://$KB_ADDR:8080/mcp" },
    "amaker": {
      "type": "stdio",
      "command": "bash",
      "args": ["-lc", "cd ~/taps/assessments && exec amaker mcp"]
    }
  }
}
EOF
```

(The `cd` matters: the server resolves `.env` — your API key, the
`KB_URL`/`KB_WIKI` pair — and its `./data` directory from where it
runs, exactly like the terminal commands do.)

## Grant the session its reach

From here on, your sessions read files on your side of the wall
(`~/taps`) and drive the appliance's tools without stopping between
calls. The kit's shipped settings are deliberately narrow; a tutorial
rig doesn't have to be. Pre-grant the walk's whole surface — this
overlay is yours (`settings.local.json`), the kit's own settings stay
untouched:

```sh
cat > ~/kb-workspace/.claude/settings.local.json <<'EOF'
{
  "enableAllProjectMcpServers": true,
  "permissions": {
    "additionalDirectories": ["~/taps"],
    "allow": ["mcp__kb", "mcp__amaker", "Edit(~/kb-workspace/**)"]
  }
}
EOF
```

`additionalDirectories` opens `~/taps` to the session; the `mcp__`
entries trust every tool the appliance and amaker serve; the `Edit`
grant covers every file-editing tool for the files this step's prompts
create, and reaches no further than the workspace itself (the
assessment draft lives here, not in the KB — it enters the wiki only
through amaker's publish). On a
production workspace you'd grant
narrowly and answer prompts as they come; this rig is a throwaway, and
pre-granting makes every session in the walk paste-and-go. (Prefer the
prompts? Skip this block — the pages still work, you'll just approve as
you watch.)

## Author your assessment

Authoring is judgment work you can delegate to your assistant — the
agent is the assist. It happens back in your **authoring workspace** —
the thin directory you made in Step 2. In your fresh terminal:

```sh
cd ~/kb-workspace
claude
```

Then paste:

```text
Author a maturity assessment of this project's delivery practice as an
amaker assessment file at ~/kb-workspace/assessment.yaml — this
workspace. Search the myproject wiki
first and ground the draft in anything you find — but expect it to be
empty (it's freshly created); where it's silent, draft from standard
delivery practice without stopping to ask, and we'll tailor on later
trips around the loop. Shape: 2-3 domains, each with 1-2 practices,
each practice with 2-4 yes/no questions — set each question's
polarity, and give negative findings a remediation and roles. Then
check it with amaker's validate tool and fix what it reports until the
file validates, and finish with amaker's import tool — tell me the
project_id it reports.
```

`import` is the headless authoring door: your drafted file becomes a
project with a published version, ready for a respondent. (Prefer
clicking? The author UI at `:3000` co-creates the same thing
conversationally — assistant drafts, you steer, publish a version when
it says what you mean.)

## Respond — this part is you

The respond app serves each assessment at its own address — a link a
respondent gets handed, not a site they browse. Yours is
`:3001/assess/<project-id>` (from the clean room:
`http://walk.local:3001/assess/<project-id>`), with the project id
your session reported at import. Open it and answer as the
respondent. The questions exist to collect *your* ground truth about
the project — that's the one seat in this loop that stays human on
purpose.

## Analyze and publish

The analyze app addresses the same way — `:3002/analyze/<project-id>`
— and shows the scorecard, gaps, and roadmap as you answer. When it reflects reality, land the result in your wiki — ask
your workspace session (amaker's `status` tool tells it your response
is in — `response.complete` — and `publish` lands it), or do it
yourself from a terminal:

```sh
cd ~/taps/assessments
amaker publish <project-id>
```

The report it prints is the whole story: both schemas registered
(`registered` on first contact, `unchanged` ever after), two pages
written — `assessments/<name>` and `assessments/<name>-report` — and
the admission gate's verdict, including how many pages the search index
actually picked up. Publishing is repeatable: re-answer, re-publish.

## Verify

Ask your workspace session:

```text
What does our assessment report say about the project? Cite pages.
```

It finds `assessments/<name>-report` by search and reads it — the wiki
tools read amaker's pages like any others; ownership is a *write*
boundary. Then look at what first contact did to your wiki's
vocabulary:

```sh
incus exec kb -- su - kb -c 'llm-wiki schema list --wiki myproject && llm-wiki schema show assessment --wiki myproject'
```

`assessment` and `assessment-report` are in the vocabulary now, and
the `show` output carries the proof — `"x-owner": "amaker"` — your
wiki learned amaker's classes when amaker showed up, not a moment
earlier. Step 4 picks the loop up from these pages.
