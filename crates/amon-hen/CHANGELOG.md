# Changelog

## 0.1.19

- Coalesce Claude streaming `text_delta` chunks into readable `assistant live:` snapshots instead of logging chopped mid-word fragments.
- Update Studio live assistant output in place per provider/role/iteration so one running answer stays readable while token/tool telemetry continues updating.

## 0.1.18

- Added `--gemini-approval-mode` so Gemini no longer has to run in read-only `plan` mode when executor/tool access is explicitly allowed.
- Wired Gemini approval mode through provider options, Studio settings, and persistent Studio profiles.
- Documented the safer `auto_edit` Gemini executor mode for runs that need tool execution without jumping straight to fully permissive `yolo`.

## 0.1.17

- Send Gemini prompts through stdin instead of argv so large Studio contexts and sub-agent prompts do not hit OS argument-length limits.
- Keep `-p` enabled for Gemini headless mode while avoiding the full prompt as a command-line argument.
- Classify non-`NotFound` spawn failures as `error` instead of incorrectly showing them as missing providers.

## 0.1.16

- Decode nested Claude `stream_event` records into readable assistant/tool messages.
- Suppress Claude `input_json_delta.partial_json` shards so Studio no longer floods with raw tool JSON while an agent call is being assembled.
- Prevent JSON-looking provider lines from falling back to raw log output when they are not visible user-facing messages.

## 0.1.15

- Added `--planner-mode parallel` so a lead/planner can start immediately beside Codex, Claude, and Gemini executors instead of blocking the whole run, even when handoff context is enabled.
- Kept `--planner-mode blocking` as the default when planner output should feed executor prompts.
- Exposed planner mode in Studio settings and persistent Studio profiles.

## 0.1.14

- Decode Claude, Codex, and Gemini JSON stream events into readable Studio messages instead of raw provider JSON.
- Suppress provider hook/session plumbing from the live run log while preserving visible assistant text, tool calls, results, errors, tokens, and progress.
- Render Codex command execution events as concise shell/tool summaries with output snippets.

## 0.1.13

- Keep Studio responsive while provider CLIs stream large outputs by reusing the terminal backend instead of rebuilding it every frame.
- Drain Studio job events in bounded batches so scrolling and typing are not blocked behind provider log bursts.
- Sample high-volume provider stream logs while preserving live token/tool telemetry and important tool/error/result events.

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
