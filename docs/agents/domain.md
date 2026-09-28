# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Layout: single-context

This is a **single-context** repo: one repository, one shared domain vocabulary.

```
/
├── CONTEXT.md          ← the domain glossary, at the repo root
├── docs/adr/           ← architecture decision records
└── src/
```

There is no `CONTEXT-MAP.md` and no per-subproject `CONTEXT.md`. `src/` (SvelteKit front end) and `src-tauri/` (Rust host) are not separate contexts; they share this one glossary.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root: the shared glossary for the whole repo.
- **`docs/adr/`**: read ADRs that touch the area you're about to work in.

If these files don't exist yet, **proceed silently**. Don't flag their absence; don't suggest creating them upfront. The `/domain-modeling` skill (reached via `/grill-with-docs` and `/improve-codebase-architecture`) creates them lazily when terms or decisions actually get resolved.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a test name), use the term as defined in `CONTEXT.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal: either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0007 (event-sourced orders), but worth reopening because…_
