# Amon Hen CLI

Amon Hen is a native Rust command center for Codex, Claude, Gemini, and Linear delivery loops.

## Install

From this checkout:

```bash
cargo install --path crates/amon-hen
amon-hen --help
```

For development:

```bash
cargo run -p amon-hen -- --help
```

Amon Hen shells out to provider CLIs that are already authenticated on your machine:

- `codex`
- `claude`
- `gemini`

Override binary paths when your CLIs are not on `PATH`:

```bash
AMON_HEN_CODEX_BIN=/path/to/codex \
AMON_HEN_CLAUDE_BIN=/path/to/claude \
AMON_HEN_GEMINI_BIN=/path/to/gemini \
amon-hen --auth-status --capabilities-status
```

## Studio

Open the native interactive Studio:

```bash
amon-hen --studio --members codex,claude,gemini
```

Studio gives you selectable panes for settings, agents, auth, Linear, files, tools, provider capabilities, token usage, command logs, and help. It supports role changes after launch, manual provider auth method selection, browser-tab social login handoff, file tagging, command tagging, per-provider capability overrides, and double-Ctrl+C exit.

## Basic Runs

Ask all providers and synthesize once:

```bash
amon-hen \
  --members codex,claude,gemini \
  "Inspect this repo and propose the cleanest next patch"
```

Pick roles and let providers hand work to each other:

```bash
amon-hen \
  --members codex,claude,gemini \
  --planner codex \
  --lead claude \
  --handoff \
  --iterations 2 \
  "Design, implement, verify, and summarize the next safe change"
```

Fan out real same-provider sub-agents:

```bash
amon-hen \
  --members codex,claude,gemini \
  --planner codex \
  --lead claude \
  --team-work 2 \
  "Split this task into parallel work and reconcile the final patch"
```

## Provider Controls

Set model and effort per provider:

```bash
amon-hen \
  --members codex,claude,gemini \
  --codex-model gpt-5.2 \
  --codex-effort high \
  --claude-model sonnet \
  --claude-effort max \
  --gemini-model gemini-pro \
  --gemini-effort high \
  "Compare implementation options and choose one"
```

Set permissions and execution policy:

```bash
amon-hen \
  --members codex,claude,gemini \
  --codex-sandbox workspace-write \
  --claude-permission-mode acceptEdits \
  "Make the change, run tests, and report exactly what changed"
```

Inherit or override provider-native capability surfaces:

```bash
amon-hen \
  --members codex,claude,gemini \
  --codex-config ~/.codex/config.toml \
  --codex-mcp-profile repo \
  --claude-mcp-config .claude/mcp.json \
  --claude-allowed-tools Edit,Bash,Read \
  --claude-disallowed-tools WebFetch \
  --gemini-settings .gemini/settings.json \
  --gemini-tools-profile repo \
  "Use the configured MCP/tools surface and implement the patch"
```

## Auth

Check local provider status:

```bash
amon-hen --auth-status --capabilities-status
```

Launch social login flows for provider CLIs:

```bash
amon-hen \
  --auth-login \
  --auth-login-providers codex,claude,gemini
```

The auth flow can open browser tabs and return through code paste or provider deeplink, depending on what the underlying CLI supports.

## Prompt Context

Attach local files:

```bash
amon-hen \
  --members codex,claude,gemini \
  --file crates/amon-hen/src/lib.rs \
  --file crates/amon-hen/src/linear_delivery.rs \
  "Review these files and propose the next patch"
```

Attach command output and show the commands as tool usage:

```bash
amon-hen \
  --members codex,claude,gemini \
  --cmd "cargo test --workspace --locked" \
  --cmd "cargo clippy --workspace --locked -- -D warnings" \
  "Use this command output while deciding what to fix"
```

## Linear Delivery

Run a long-lived Linear loop against a project:

```bash
amon-hen \
  --deliver-linear \
  --linear-project ENG \
  --linear-until-complete \
  --linear-completion-gate review-or-ci \
  --linear-max-polls 12 \
  --linear-max-concurrency 2 \
  --linear-workflow-file docs/linear-workflow.md \
  --linear-limit 4 \
  --linear-max-attempts 3 \
  --members codex,claude,gemini \
  --planner codex \
  --lead claude \
  --team-work 2
```

Target epics, teams, states, assignees, labels, or explicit issues when you need a tighter queue. The delivery worker creates isolated issue workspaces, runs planner/executor/verification phases with bounded issue concurrency, persists retry and reconciliation state, comments progress back to Linear, attaches generated media or command output, and stops only at the configured human-review or CI gate.

## Output And Telemetry

Emit JSON for automation:

```bash
amon-hen \
  --json \
  --members codex,claude,gemini \
  --team-work 1 \
  "Summarize tool usage, token usage, and final recommendation"
```

Use verbose output when you want to see provider commands and telemetry in a plain terminal run:

```bash
amon-hen \
  --verbose \
  --members codex,claude,gemini \
  --cmd "git status --short" \
  "Explain the current repo state"
```

## Development Checks

```bash
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --locked -- -D warnings
```
