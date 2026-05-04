# Amon Hen CLI

Amon Hen is a native Rust command center for Codex, Claude, Gemini, and Linear delivery loops.

## Install

From crates.io:

```bash
cargo install amon-hen --version 0.1.20 --force
amon-hen --help
```

From this checkout:

```bash
cargo install --path crates/amon-hen
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

Studio gives you selectable panes for settings, agents, auth, Linear, files, tools, provider capabilities, token usage, command logs, and help. It supports role changes after launch, manual provider auth method selection, browser-tab social login handoff, file tagging, command tagging, per-provider capability overrides, readable provider stream decoding, and double-Ctrl+C exit.

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
  --planner claude \
  --lead claude \
  --handoff \
  --iterations 2 \
  "Design, implement, verify, and summarize the next safe change"
```

Keep the lead/planner in a serial review chain with executors:

```bash
amon-hen \
  --studio \
  --members codex,claude,gemini \
  --planner claude \
  --planner-mode review-chain \
  --lead claude \
  --summarizer claude \
  --handoff \
  --iterations 10 \
  --team-work 2 \
  --codex-sub-agents 3 \
  --claude-sub-agents 0 \
  --gemini-sub-agents 3 \
  "Plan, implement, verify, and reconcile this project"
```

Planner modes:

- `--planner-mode blocking` waits for the planner output before executor prompts are built. This is the default and is best when you want a true planner handoff first.
- `--planner-mode parallel` starts the planner/lead in the same iteration as the executors. This is best when Claude leads/plans but Codex and Gemini should not sit queued.
- `--planner-mode review-chain` runs members serially in planner/lead order. Each provider reviews the previous provider's handoff and the current repo state before making deliberate deltas, which is the safer mode for production VPS work.

Fan out real same-provider sub-agents:

```bash
amon-hen \
  --members codex,claude,gemini \
  --planner claude \
  --planner-mode review-chain \
  --lead claude \
  --team-work 2 \
  "Review each prior agent handoff, implement safely, and reconcile the final patch"
```

## Provider Controls

Set model and effort per provider:

```bash
amon-hen \
  --members codex,claude,gemini \
  --codex-model gpt-5.5 \
  --codex-effort xhigh \
  --claude-model opus \
  --claude-effort max \
  --gemini-model gemini-3.1-pro-preview \
  --gemini-effort high \
  "Compare implementation options and choose one"
```

Model values are passed through to the provider CLI. If Claude, Codex, or Gemini rejects a model value, replace it with a model name your installed CLI/account supports.

Set permissions and execution policy:

```bash
amon-hen \
  --members codex,claude,gemini \
  --codex-sandbox workspace-write \
  --claude-permission-mode acceptEdits \
  --gemini-approval-mode auto_edit \
  "Make the change, run tests, and report exactly what changed"
```

Gemini approval defaults to `plan`, which is intentionally read-only. Use `--gemini-approval-mode auto_edit` for executor runs that need tool/edit access, and reserve `--gemini-approval-mode yolo` for deliberately permissive sessions.

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
  --planner claude \
  --planner-mode parallel \
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

Emit live NDJSON progress while providers are still running:

```bash
amon-hen \
  --json-stream \
  --members codex,claude,gemini \
  --planner claude \
  --planner-mode review-chain \
  --lead claude \
  --handoff \
  "Show live provider status, tokens, tools, and final result"
```

Use verbose output when you want to see provider commands and telemetry in a plain terminal run:

```bash
amon-hen \
  --verbose \
  --members codex,claude,gemini \
  --cmd "git status --short" \
  "Explain the current repo state"
```

Provider stream decoding:

- Claude nested `stream_event` messages are decoded into readable assistant text and tool starts.
- Claude `input_json_delta.partial_json`, hook/session events, hidden thinking/signatures, and provider plumbing are suppressed in Studio logs.
- Codex command events and Gemini text/function-call events are summarized into readable tool or assistant lines.

## Recorded Studio Runs

Use `script` when you want an audit trail of an interactive Studio session:

```bash
export AMON_HEN_RUN_DIR="$HOME/amon-hen-runs/$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$AMON_HEN_RUN_DIR"

cat > "$AMON_HEN_RUN_DIR/prompt.txt" <<'PROMPT'
Orchestrator:
Use as many subagents as useful, but split ownership to avoid merge conflicts. Keep the main agent responsible for integration and final judgment. All proposed changes must pass tests and be integrated deliberately.

Review the repository, implement the highest-impact safe patch, run tests, and report changed files, commands, failures, blockers, and next steps.
PROMPT

script -q -f "$AMON_HEN_RUN_DIR/studio.typescript" -c "amon-hen \
  --studio \
  --cwd /path/to/repo \
  --members codex,claude,gemini \
  --planner claude \
  --planner-mode review-chain \
  --lead claude \
  --summarizer claude \
  --handoff \
  --iterations 10 \
  --team-work 2 \
  --codex-sub-agents 3 \
  --claude-sub-agents 0 \
  --gemini-sub-agents 3 \
  --codex-model gpt-5.5 \
  --claude-model opus \
  --gemini-model gemini-3.1-pro-preview \
  --codex-sandbox workspace-write \
  --claude-permission-mode acceptEdits \
  --gemini-approval-mode auto_edit \
  --codex-effort xhigh \
  --claude-effort max \
  --gemini-effort high \
  --timeout 7200 \
  --max-member-chars 140000 \
  --cmd 'pwd && hostname && uptime' \
  --cmd 'git status -sb' \
  --cmd 'git log --oneline -5' \
  \"\$(cat \"$AMON_HEN_RUN_DIR/prompt.txt\")\""
```

The terminal recording is written to `$AMON_HEN_RUN_DIR/studio.typescript`.

## Development Checks

```bash
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --locked -- -D warnings
```
