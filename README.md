<div align="center">
  <img src="web/public/amonhen.svg" width="92" alt="Amon Hen mark" />
  <h1>Amon Hen</h1>
  <p><strong>The Seat of Seeing for AI engineering work.</strong></p>
  <p>
    A Rust-native command center for Codex, Claude, Gemini, and Linear delivery loops.
  </p>
  <p>
    <a href="https://amonhen.legit.place">Website</a>
    ·
    <a href="crates/council/README.md">CLI docs</a>
    ·
    <a href="https://github.com/Dviros/Amon-Hen/actions">CI</a>
    ·
    <a href="SECURITY.md">Security</a>
  </p>
</div>

![Amon Hen Studio](docs/screenshots/studio.svg)

## What This Is

Amon Hen turns local AI coding CLIs into a coordinated delivery team. Codex can plan, Claude can lead, Gemini can execute, and each provider can spawn its own same-provider sub-agents when the task needs more hands. You get one native terminal surface for roles, handoffs, iterations, auth, provider capability overrides, token telemetry, tool logs, local files, command context, and Linear delivery.

The shipped binary remains `council` for compatibility. The project identity is Amon Hen.

This is a ground-up Rust rewrite. The original npm CLI core is gone from the tracked implementation; Node is used only for the separate Astro website under [`web/`](web/).

## Why It Feels Different

- It runs the provider CLIs you already authenticate locally: `codex`, `claude`, and `gemini`.
- It lets you choose a planner, lead, executors, handoff mode, iteration count, and provider-specific effort.
- It exposes provider-native config instead of flattening everything into a fake common denominator.
- It can watch Linear projects or epics until each issue lands at human review or a GitHub CI gate.
- It shows the uncomfortable-but-useful stuff: token usage, tool commands, prompt commands, file context, retries, and reconciliation state.
- It ships a native Rust Studio TUI for interactive work, not a static ASCII status dump.

## Install

```bash
cargo install --path crates/council
```

From a checkout:

```bash
cargo run -p council -- --help
```

Provider binary paths can be overridden when needed:

```bash
COUNCIL_CODEX_BIN=/path/to/codex \
COUNCIL_CLAUDE_BIN=/path/to/claude \
COUNCIL_GEMINI_BIN=/path/to/gemini \
council --auth-status --capabilities-status
```

## Command Cookbook

Open the interactive Studio:

```bash
council --studio --members codex,claude,gemini
```

Ask all providers and synthesize one answer:

```bash
council \
  --members codex,claude,gemini \
  "Inspect this repo and propose the cleanest next patch"
```

Pick roles, handoff, iterations, and same-provider sub-agents:

```bash
council \
  --members codex,claude,gemini \
  --planner codex \
  --lead claude \
  --handoff \
  --iterations 2 \
  --team-work 2 \
  "Design and implement the next safe change"
```

Control model and effort per provider:

```bash
council \
  --members codex,claude,gemini \
  --codex-model gpt-5.2 \
  --codex-effort high \
  --claude-model sonnet \
  --claude-effort max \
  --gemini-model gemini-pro \
  --gemini-effort high \
  "Compare implementation options and choose one"
```

Override provider permissions and capability surfaces:

```bash
council \
  --members codex,claude,gemini \
  --codex-sandbox workspace-write \
  --codex-config ~/.codex/config.toml \
  --codex-mcp-profile repo \
  --claude-permission-mode acceptEdits \
  --claude-mcp-config .claude/mcp.json \
  --claude-allowed-tools Edit,Bash,Read \
  --claude-disallowed-tools WebFetch \
  --gemini-settings .gemini/settings.json \
  --gemini-tools-profile repo \
  "Make the patch, run tests, and report exactly what changed"
```

Launch provider social login flows:

```bash
council \
  --auth-login \
  --auth-login-providers codex,claude,gemini
```

Attach local files and command output to the prompt:

```bash
council \
  --members codex,claude,gemini \
  --file crates/council/src/lib.rs \
  --file web/src/pages/index.astro \
  --cmd "cargo test --workspace --locked" \
  --cmd "npm --prefix web run build" \
  "Review this change and identify the next fix"
```

Run a long-lived Linear delivery loop:

```bash
council \
  --deliver-linear \
  --linear-project ENG \
  --linear-until-complete \
  --linear-completion-gate review-or-ci \
  --linear-limit 4 \
  --linear-max-attempts 3 \
  --members codex,claude,gemini \
  --planner codex \
  --lead claude \
  --team-work 2
```

Emit machine-readable telemetry:

```bash
council \
  --json \
  --members codex,claude,gemini \
  --team-work 1 \
  "Summarize tool usage, tokens, and final recommendation"
```

![Amon Hen command line](docs/screenshots/terminal-run.svg)

## Studio

Studio is the native TUI for live work:

- movable panes for settings, agents, results, Linear, auth, files, tools, capabilities, and help
- manual auth method selection per provider
- browser-tab social login handoff with code paste or deeplink support
- lead/planner/executor role changes after launch
- per-provider model, effort, sandbox, permissions, and capability settings
- provider Skills, MCP, and tools inherit/override toggles
- token, sub-agent, prompt-command, and tool-command telemetry
- double-Ctrl+C exit so one accidental interrupt does not kill a long run

## Linear Delivery

Amon Hen can treat Linear as the work queue, not just a ticket reference.

![Amon Hen Linear delivery loop](docs/screenshots/linear-delivery.svg)

The delivery loop can:

- poll targeted projects, epics, teams, states, assignees, or explicit issues
- create isolated workspaces per issue
- run planner, execution, verification, reconciliation, and reporting phases
- retry with backoff and persist issue state
- attach generated media and command outputs back to Linear
- post progress comments and optional review-state updates
- wait for GitHub CI or hand work to human review

## Repository Layout

```text
.
├── crates/council/      # Rust crate and council binary
├── web/                 # Astro site and Cloudflare Worker config
├── docs/screenshots/    # README visuals
└── .github/workflows/   # CI, release, and site deployment
```

## Development

CLI:

```bash
cargo fmt --all --check
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --locked -- -D warnings
```

Site:

```bash
cd web
npm install
npm run build
```

## Site Deployment

The canonical site is [amonhen.legit.place](https://amonhen.legit.place).

The production path is Cloudflare Workers:

- Cloudflare Worker config in [`web/wrangler.jsonc`](web/wrangler.jsonc)
- Custom domain route for `amonhen.legit.place`
- `workers_dev` disabled so the Worker is not exposed on the account `workers.dev` subdomain
- Host guard middleware that returns `404` for any non-`amonhen.legit.place` host
- Static security headers in [`web/public/_headers`](web/public/_headers)

The GitHub Pages workflow remains available as a static preview path for CI-built artifacts, but it does not own the production custom domain.

## Standalone Status

Local git now points only at `https://github.com/Dviros/Amon-Hen`.

GitHub still reports the hosted repository as a fork until the fork network is detached in GitHub. GitHub's supported options are the Settings -> Danger Zone -> Leave fork network flow when eligible, or the delete/recreate/mirror-push process described in GitHub's detach-fork documentation. Both are permanent repository-level operations, so do them only with a mirror backup and after accepting that GitHub metadata such as pull requests and settings may be lost.

## Credits

Amon Hen was inspired by the original [seeARMS/council](https://github.com/seeARMS/council) idea by Colin Armstrong. This repository has been rewritten from the ground up as a Rust-native project, with the original npm CLI core removed from the tracked implementation.

## Contributors

- [Dviros](https://github.com/Dviros)

## Security

Do not commit provider tokens, Cloudflare tokens, Linear tokens, local absolute paths, or command history containing secrets. If a token has been pasted into a terminal, chat, or deploy log, rotate it after use.

See [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)
