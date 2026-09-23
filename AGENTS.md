# AGENTS.md

DeepPi is a Tauri v2 desktop host for the Pi Coding Agent and the DeepSeek Harness. The SvelteKit app in `src/` is the UI; the Rust crate in `src-tauri/` is the host that owns PTYs, processes, and the filesystem bridge. `src/` and `src-tauri/` are two halves of one product, not two contexts.

Commands live in `package.json` and `src-tauri/Cargo.toml`. Other docs worth knowing about: `DESIGN.md`, `DEVELOPMENT.md`, `REVIEW.md`, `RELEASING.md`.

## Agent skills

### Issue tracker

Issues and specs live as GitHub issues in `springkai66/deep-pi`, driven by the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles keep their default label strings, plus the `wayfinder:*` group. See `docs/agents/triage-labels.md`.

### Domain docs

single-context. See `docs/agents/domain.md`.
