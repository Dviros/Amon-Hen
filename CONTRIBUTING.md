# Contributing

The product runtime is the native Rust crate under `crates/amon-hen/`. It ships the `amon-hen` binary.

## Setup

CLI:

```bash
cargo fmt --all --check
cargo build --workspace
cargo test --workspace
```

## Development guidelines

- Keep the CLI scriptable first. Primary output belongs on stdout; progress and diagnostics belong on stderr.
- Preserve safe defaults. Amon Hen should make autonomous behavior explicit and observable.
- Add tests with behavior changes, especially around parsing, failure isolation, output modes, and exit codes.
- Avoid runtime dependencies unless they materially improve portability or maintainability.

## Releases

The published CLI artifact is the Rust crate in `crates/amon-hen/`.

- Versioning, changelog updates, Git tags, and GitHub releases are managed by `release-please`.
- The release workflow is defined in [release-please.yml](./.github/workflows/release-please.yml).
- The release metadata files live at [release-please-config.json](./release-please-config.json) and [.release-please-manifest.json](./.release-please-manifest.json).
- Changelog entries are written to the Rust crate changelog.
- Release notes are derived from Conventional Commit messages, so squash-merge titles and direct commits to `main` should follow that format when possible.
- crates.io publishing assumes trusted publishing or a scoped Cargo registry token has been configured for `.github/workflows/release-please.yml`.

Before merging release-affecting changes, confirm:

- CLI tests still pass with `cargo test --workspace`
- the CLI still builds with `cargo build --workspace`
- README examples and Cargo metadata still match the current implementation
