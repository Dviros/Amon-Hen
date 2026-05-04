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
    <a href="crates/amon-hen/README.md">CLI docs</a>
    ·
    <a href="https://github.com/Dviros/Amon-Hen/actions">CI</a>
    ·
    <a href="SECURITY.md">Security</a>
  </p>
</div>

![Amon Hen Studio](docs/screenshots/studio.svg)

## What This Is

Amon Hen turns local AI coding CLIs into a coordinated delivery team. Codex can plan, Claude can lead, Gemini can execute, and each provider can spawn its own same-provider sub-agents when the task needs more hands. You get one native terminal surface for roles, handoffs, iterations, auth, provider capability overrides, token telemetry, tool logs, local files, command context, and Linear delivery.

This is a ground-up Rust implementation. The CLI and delivery runtime live in the Cargo workspace.

## Why It Feels Different

- It runs the provider CLIs you already authenticate locally: `codex`, `claude`, and `gemini`.
- It lets you choose a planner, lead, executors, handoff mode, iteration count, and provider-specific effort.
- It exposes provider-native config instead of flattening everything into a fake common denominator.
- It can watch Linear projects or epics until each issue lands at human review or a GitHub CI gate.
- It shows the uncomfortable-but-useful stuff: token usage, tool commands, prompt commands, file context, retries, and reconciliation state.
- It ships a native Rust Studio TUI for interactive work, not a static ASCII status dump.

## Install

```bash
cargo install amon-hen --version 0.1.19 --force
```

From a checkout:

```bash
cargo install --path crates/amon-hen
cargo run -p amon-hen -- --help
```

Provider binary paths can be overridden when needed:

```bash
AMON_HEN_CODEX_BIN=/path/to/codex \
AMON_HEN_CLAUDE_BIN=/path/to/claude \
AMON_HEN_GEMINI_BIN=/path/to/gemini \
amon-hen --auth-status --capabilities-status
```

## Command Cookbook

Open the interactive Studio:

```bash
amon-hen --studio --members codex,claude,gemini
```

Ask all providers and synthesize one answer:

```bash
amon-hen \
  --members codex,claude,gemini \
  "Inspect this repo and propose the cleanest next patch"
```

Run Claude as lead/planner without blocking Codex and Gemini:

```bash
amon-hen \
  --members codex,claude,gemini \
  --planner claude \
  --planner-mode parallel \
  --lead claude \
  --summarizer claude \
  --handoff \
  --iterations 10 \
  --team-work 2 \
  --codex-sub-agents 3 \
  --claude-sub-agents 0 \
  --gemini-sub-agents 3 \
  "Design, implement, verify, and summarize the next safe change"
```

Use `--planner-mode blocking` when executor prompts should wait for a planner handoff first. Use `--planner-mode parallel` when the planner/lead should run alongside the executors in the same iteration.

Control model and effort per provider:

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

Model names are passed through to the underlying provider CLI. If a provider rejects a model name, choose one that your local CLI/account accepts.

Override provider permissions and capability surfaces:

```bash
amon-hen \
  --members codex,claude,gemini \
  --codex-sandbox workspace-write \
  --codex-config ~/.codex/config.toml \
  --codex-mcp-profile repo \
  --claude-permission-mode acceptEdits \
  --claude-mcp-config .claude/mcp.json \
  --claude-allowed-tools Edit,Bash,Read \
  --claude-disallowed-tools WebFetch \
  --gemini-approval-mode auto_edit \
  --gemini-settings .gemini/settings.json \
  --gemini-tools-profile repo \
  "Make the patch, run tests, and report exactly what changed"
```

Use `--gemini-approval-mode plan` for read-only Gemini analysis, `auto_edit` for executor runs that need approved edits/tools, or `yolo` only when you intentionally want Gemini's CLI to run without approval prompts.

Launch provider social login flows:

```bash
amon-hen \
  --auth-login \
  --auth-login-providers codex,claude,gemini
```

Attach local files and command output to the prompt:

```bash
amon-hen \
  --members codex,claude,gemini \
  --file crates/amon-hen/src/lib.rs \
  --cmd "cargo test --workspace --locked" \
  --cmd "cargo clippy --workspace --locked -- -D warnings" \
  "Review this change and identify the next fix"
```

Run a long-lived Linear delivery loop:

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

Emit machine-readable telemetry:

```bash
amon-hen \
  --json \
  --members codex,claude,gemini \
  --team-work 1 \
  "Summarize tool usage, tokens, and final recommendation"
```

Stream live NDJSON progress for dashboards and automation:

```bash
amon-hen \
  --json-stream \
  --members codex,claude,gemini \
  --planner claude \
  --planner-mode parallel \
  --lead claude \
  --handoff \
  "Show provider progress while the run is still active"
```

Record an interactive Studio run with terminal playback:

```bash
export AMON_HEN_RUN_DIR="$HOME/amon-hen-runs/$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$AMON_HEN_RUN_DIR"
printf '%s\n' "Inspect this repository, implement the safest high-impact patch, run tests, and report evidence." > "$AMON_HEN_RUN_DIR/prompt.txt"

script -q -f "$AMON_HEN_RUN_DIR/studio.typescript" -c "amon-hen \
  --studio \
  --cwd /path/to/repo \
  --members codex,claude,gemini \
  --planner claude \
  --planner-mode parallel \
  --lead claude \
  --summarizer claude \
  --handoff \
  --iterations 10 \
  --team-work 2 \
  --codex-sub-agents 3 \
  --claude-sub-agents 0 \
  --gemini-sub-agents 3 \
  --timeout 7200 \
  --max-member-chars 140000 \
  --cmd 'pwd && hostname && uptime' \
  --cmd 'git status -sb' \
  \"\$(cat \"$AMON_HEN_RUN_DIR/prompt.txt\")\""
```

![Amon Hen command line](docs/screenshots/terminal-run.svg)

## Studio

Studio is the native TUI for live work:

- movable panes for settings, agents, results, Linear, auth, files, tools, capabilities, and help
- manual auth method selection per provider
- browser-tab social login handoff with code paste or deeplink support
- lead/planner/executor role changes after launch
- `blocking` or `parallel` planner mode
- per-provider model, effort, sandbox, permissions, and capability settings
- provider Skills, MCP, and tools inherit/override toggles
- token, sub-agent, prompt-command, provider-stream, and tool-command telemetry
- readable Claude, Codex, and Gemini stream decoding instead of raw provider JSON
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

## Project Layout

```text
.
├── crates/amon-hen/      # Rust crate and amon-hen binary
├── web/                 # Product site
├── docs/screenshots/    # README visuals
└── .github/workflows/   # CI and release automation
```

## Development

CLI:

```bash
cargo fmt --all --check
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --locked -- -D warnings
```

## Contributors

- [Dviros](https://github.com/Dviros)

## Security

Do not commit provider tokens, Cloudflare tokens, Linear tokens, local absolute paths, or command history containing secrets. If a token has been pasted into a terminal, chat, or deploy log, rotate it after use.

See [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)
