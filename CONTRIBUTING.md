# Contributing to ImRule

Thank you for your interest in improving ImRule! This document describes how to
check out the project, run tests, and submit changes.

## Development environment

- **Rust toolchain**: 1.85 or newer (see `rust-version` in `Cargo.toml`).
- **OS**: macOS, Linux, and Windows are supported for development.
- **Git**: any recent version.

Clone the repository and verify your environment:

```bash
git clone https://github.com/soapbird/imrule.git
cd imrule
make
```

`make` runs formatting checks, Clippy lints, the full test suite, and a release
build.

## Project structure

ImRule follows a hexagonal / clean-architecture layout:

- `src/domain/` — pure business logic, no I/O, no external dependencies.
- `src/application/` — use cases and port traits.
- `src/infrastructure/` — concrete I/O adapters.
- `src/interface/` — CLI adapter (`clap`).
- `tests/` — integration and contract tests.

There are no unit tests inside `src/`. All behavior is verified through the
contract tests in `tests/`.

## Making changes

1. Open an issue or discussion first for large features or breaking changes.
2. Create a feature branch from `main`.
3. Make focused, minimal changes.
4. Add or update contract tests in `tests/` for any changed behavior.
5. Run `make` before pushing.

## Commit messages

We use [Conventional Commits](https://www.conventionalcommits.org/) to generate
the changelog automatically. Prefix your commits with one of:

- `feat:` — new feature
- `fix:` — bug fix
- `docs:` — documentation only
- `style:` — formatting, no logic change
- `refactor:` — code change that neither fixes a bug nor adds a feature
- `perf:` — performance improvement
- `test:` — adding or updating tests
- `chore:` — build, CI, or tooling changes

Example:

```
feat: add support for Zed agent
```

## Code style

- Run `cargo fmt` before committing.
- Keep Clippy clean: `cargo clippy -- -D warnings`.
- Do not import domain or infrastructure modules directly from `main.rs`.
- Preserve the existing architecture contract enforced by
  `tests/architecture_contract.rs`.

## Testing

```bash
make test          # integration/contract tests
make test-e2e      # shell-based end-to-end tests
make coverage      # generate an lcov coverage report
```

## Pull request checklist

- [ ] `make` passes locally.
- [ ] New behavior is covered by a contract test.
- [ ] Documentation (`README.md`, `.imrule/AGENTS.md`) is updated if needed.
- [ ] `CHANGELOG.md` is updated if the change is user-facing.

## Getting help

Open a [GitHub Discussion](https://github.com/soapbird/imrule/discussions) or
comment on an existing issue. Maintainers will respond as time allows.
