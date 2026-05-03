# Changelog

## 0.1.12

- Stream provider stdout/stderr into Studio and `--json-stream` while provider CLIs are still running.
- Update live Studio token/tool/sub-agent counters from progress events instead of waiting for final results.
- Surface visible provider output snippets and observed tool calls in the run log, while keeping hidden model reasoning hidden.

## 0.1.11

- Added a real runtime event bus and true `--json-stream` NDJSON progress events, including ordered provider, token/tool, iteration, and final result events.
- Preserved full iteration and sub-agent timelines in the result model while keeping final member and summary fields compatible.
- Converted Studio auth, capability, Linear status, and Linear delivery actions into in-dashboard async jobs with cancellation plumbing, persistent profiles, and provider health/onboarding details.
- Strengthened Linear delivery with deterministic gates for repo changes, secret/local-path scans, validation command capture, PR/CI evidence, blocked/no-op outcomes, and retry reconciliation.
- Added CLI regression coverage proving planner, lead, executor, provider settings, iterations, handoff, and sub-agent roles reach the correct provider options and prompts.

## 0.1.10

- Reworked Studio runs into an in-dashboard async job loop instead of leaving alternate-screen mode and blocking the terminal.
- Added structured runtime progress events so provider cards and the execution log update while context commands, providers, and sub-agents run.
- Added provider CLI preflight so missing Codex/Claude/Gemini executables fail once with a clear diagnostic instead of spawning a wall of missing sub-agents.

## 0.1.9

- Added live verbose runner progress so long provider/team runs print start, spawn, heartbeat, and completion lines instead of appearing frozen behind the banner.
- Updated Studio run actions to announce the external live run and force verbose progress while the dashboard temporarily yields the terminal.

## 0.1.8

- Rebuilt Studio on Ratatui with a color dashboard, provider cards, live configuration panes, token/tool telemetry, and a compact fallback for smaller terminals.
- Added direct prompt editing from Studio and kept session context path-safe by showing the repo name instead of local absolute paths.

## 0.1.7

- Redrew Studio with absolute cursor positioning so terminal auto-wrap cannot turn panes into diagonal borders.
- Replaced raw clap output with a curated grouped CLI help page.

## 0.1.6

- Fixed native Studio pane rendering so rows clip to the active terminal width instead of wrapping into diagonal borders.
- Added Studio regression coverage for clipped panes, prompt rendering, and two-press Ctrl+C quit behavior.

## 0.1.5

- Native Rust CLI migration baseline.
