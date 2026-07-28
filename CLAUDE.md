# Repository Guidelines

## Project Structure & Module Organization

ShenDesk is a Tauri 2 desktop application. The main application lives in `apps/desktop`:

- `src/` contains the React 19 + TypeScript UI. Put Tauri IPC wrappers in `src/services/`, reusable UI primitives in `src/components/ui/`, and shared client helpers in `src/lib/`.
- `src-tauri/src/` is the Rust backend, organized by `app/`, `application/`, `domain/`, `infrastructure/`, and `commands/`.
- `tests/` contains frontend Node tests; Rust tests live beside the code they exercise.
- `docs/` records architecture and operational behavior. Update the relevant document when changing IPC, plugins, updates, persistence, or security boundaries.

## Build, Test, and Development Commands

Use Node 22.12+ and pnpm (the committed `pnpm-lock.yaml` is authoritative).

```bash
pnpm install --frozen-lockfile  # install locked JS dependencies
pnpm dev                        # start the Vite UI
pnpm tauri -- dev               # run the desktop app
pnpm build                      # type-check and build the UI
pnpm test                       # Node tests plus release/build-script tests
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --all-features
```

Before submitting Rust changes, run `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check` and `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`.

## Coding Style & Naming Conventions

Use TypeScript with 2-space indentation, semicolons, `PascalCase` React components, and `camelCase` functions and variables. Keep Tauri invokes behind typed service modules rather than calling them directly from components. Use Tailwind utilities and existing shadcn/ui components before creating new CSS abstractions.

Follow standard Rust formatting (`rustfmt`), `snake_case` modules/functions, and `PascalCase` types. Preserve the backend layering: commands adapt IPC, application coordinates use cases, domain holds business types, and infrastructure owns external concerns.

## Testing Guidelines

Name Node tests `*.test.ts` and use Node's built-in test runner. Cover error mapping and service behavior when adding or changing IPC. Add Rust unit tests near the affected module, and run the commands above before opening a PR.

## Commit & Pull Request Guidelines

Follow the existing Conventional Commit style: `feat:`, `fix:`, `docs:`, `ci:`, `chore:`, `refactor:`, `perf:`, or `security:`. Keep commits focused and imperative (for example, `fix: redact updater errors`). PRs should explain the user-visible change, link the issue when applicable, note platform impact, include UI screenshots for visual changes, and pass CI checks.
