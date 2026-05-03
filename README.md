# Amon Hen

Rust-native orchestration for Codex, Claude, and Gemini.

Amon Hen is named after the Seat of Seeing: one place to bring multiple agents into view, let them plan, execute, hand off, and converge on a deliverable result. The shipped binary is still `council` for compatibility, but the project direction is now Amon Hen: a native Rust control plane for provider CLIs, autonomous Linear delivery, and an interactive terminal studio.

This repository is a ground-up Rust rewrite. The old npm CLI core is gone from the tracked project. Node is used only for the separate Astro marketing site under `web/`.

## What It Does

- Consults Codex, Claude, and Gemini from their authenticated local CLIs.
- Lets you choose a lead model, planner model, executors, handoff behavior, and iteration count.
- Supports provider-specific model, effort, auth, permission, sandbox, MCP, Skills, and tool configuration.
- Runs real same-provider sub-agent fanout with `--team-work` and per-provider team sizing.
- Shows prompt file tags, prompt command usage, provider tool usage, sub-agent activity, and token telemetry.
- Provides a native Rust Studio TUI with editable settings, movable panes, auth onboarding, Linear setup/status, provider capabilities, and double-Ctrl+C exit.
- Runs Linear project or epic delivery loops with isolated issue workspaces, retries, reconciliation, observability, comments, media attachments, and review or CI completion gates.

## Repository Layout

- [`crates/council/`](./crates/council) - the Rust crate and `council` binary.
- [`web/`](./web) - the Amon Hen site deployed as a Cloudflare Worker at [amonhen.legit.place](https://amonhen.legit.place).

## Requirements

- Rust stable.
- At least one provider CLI installed and authenticated: `codex`, `claude`, or `gemini`.
- Node `>=22` only when developing or deploying the website.

The CLI shells out to provider tools already installed on your machine. If a provider CLI is missing or unauthenticated, Amon Hen reports that state instead of inventing a result.

## Quick Start

Run from a checkout:

```bash
cargo run -p council -- --members codex,claude,gemini "Inspect this repo and propose the cleanest next patch"
```

Install the local binary:

```bash
cargo install --path crates/council
council --members codex,claude,gemini "Compare these implementation options"
```

Launch the native Studio:

```bash
council --studio --members codex,claude,gemini
```

Check provider auth and capability status:

```bash
council --auth-status --capabilities-status
```

## Provider Control

Amon Hen keeps provider-native behavior available while making it visible and scriptable.

```bash
council \
  --members codex,claude,gemini \
  --planner codex \
  --lead claude \
  --handoff \
  --iterations 2 \
  --team-work 2 \
  --codex-model gpt-5.2 \
  --codex-effort high \
  --codex-sandbox workspace-write \
  --claude-model sonnet \
  --claude-effort max \
  --claude-permission-mode acceptEdits \
  --gemini-model gemini-pro \
  --gemini-effort high \
  "Implement the task, run tests, and summarize tradeoffs"
```

Provider capability flags include:

```bash
--codex-config
--codex-mcp-profile
--claude-mcp-config
--claude-allowed-tools
--claude-disallowed-tools
--gemini-settings
--gemini-tools-profile
```

## Linear Delivery

Amon Hen can run a long-lived Linear delivery loop against selected projects or epics. Each issue gets an isolated workspace, phase-specific provider prompts, retries, reconciliation state, and observability output.

```bash
council \
  --deliver-linear \
  --linear-project ENG \
  --linear-until-complete \
  --linear-completion-gate review-or-ci \
  --members codex,claude,gemini \
  --planner codex \
  --lead claude \
  --team-work 2
```

The loop is designed to keep going until each selected task reaches a human review gate or a GitHub CI gate, depending on the configured completion policy.

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

## Maintainer Branch Posture

The fork's `main` branch is the active delivery branch. Do not open upstream pull requests from `main`; create a task branch if external review is needed. This checkout is configured so pushes go to the fork, with the upstream repository kept as a non-push reference.

GitHub does not expose a repository setting that makes a public fork's `main` branch impossible to select as a pull request source in every UI. The practical guardrails are: close upstream PRs, delete PR source branches, push only to the fork's `main`, and keep upstream remotes fetch-only.

## Credits

Amon Hen was inspired by the original [seeARMS/council](https://github.com/seeARMS/council) idea by Colin Armstrong. This repository has been rewritten from the ground up as a Rust-native project, with the original npm CLI core removed from the tracked implementation.

## Contributors

- [Dviros](https://github.com/Dviros)

## Security

Do not commit provider tokens, Cloudflare tokens, Linear tokens, local absolute paths, or command history containing secrets. If a token has been pasted into a terminal, chat, or deploy log, rotate it after use.

See [SECURITY.md](./SECURITY.md).

## License

[MIT](./LICENSE)
