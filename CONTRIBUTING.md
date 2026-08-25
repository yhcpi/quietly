# Contributing to Quietly

Thanks for your interest in Quietly! This document covers how to set up a
development environment, the project's coding conventions, and the pull
request process.

## Prerequisites

- **Node.js 18+** and **pnpm** (https://pnpm.io)
- **Rust 1.78+** with the `rustfmt` and `clippy` components
- **Microsoft C++ Build Tools** (Desktop development with C++ workload).
  `scripts/cargo-msvc.bat` shows the env setup this repo was last built on.

## Setup

```bash
pnpm install
cd src-tauri && cargo fetch
```

## Run tests

```bash
pnpm vitest run                          # frontend unit tests
cd src-tauri && cargo test --tests       # Rust integration + unit tests
```

## Development workflow

```bash
pnpm tauri dev                           # hot-reloading dev build
pnpm tauri build --no-bundle             # release binary
scripts\_package_only.bat                # portable ZIP in dist-portable/
```

## Code style

- **Rust:** `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings`
  must pass. Prefer the smallest change that fixes the issue; this codebase
  values surgical edits over speculative abstraction.
- **TypeScript / React:** `tsconfig.json` has `strict`, `noUnusedLocals`,
  and `noUnusedParameters` enabled. Avoid `any`. Use CSS classes instead of
  inline `style={{...}}` (the existing convention).
- **No `pub` for binary-internal items.** This is a single-binary crate;
  anything only consumed from inside `src-tauri/src` should be `pub(crate)`.

## Branch and commit conventions

- Branch off `main`. Use `feat/<topic>`, `fix/<topic>`, or `chore/<topic>`.
- Commit messages follow Conventional Commits (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`).
- One logical change per commit; keep PRs small enough to review in 15 minutes.

## Pull request checklist

- [ ] `pnpm vitest run` passes
- [ ] `cargo test --tests` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] Added tests for any new behavior
- [ ] Updated README / inline docs if behavior or contract changed
- [ ] No dead code, unused imports, or commented-out code introduced

## Reporting bugs / proposing features

Use the GitHub issue templates under `.github/ISSUE_TEMPLATE/`.

## Security

Please report security vulnerabilities privately — see `SECURITY.md`.

## Code of conduct

By participating you agree to abide by `CODE_OF_CONDUCT.md`.