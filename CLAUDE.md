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

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->

## Worktree 规则

- 所有 worktree 必须创建在仓库根的 `.worktrees/` 目录下，禁止散落到仓库外或其他位置。
- 创建 worktree 后必须立即执行 `scripts/setup-worktree.sh`，把被 `.gitignore` 过滤的本地配置和参考目录软链到新 worktree。

## Token 获取规则

- 使用 `/Users/xsl/shencom/patrol-eye/scripts/get-token.sh` 获取 Token。

## Git 提交规则

- 当我说"提交代码"（或类似表述）时，必须按以下顺序执行：先把代码添加到暂存区（`git add`），然后再执行 commit，禁止直接用 `git commit -a` 一步完成，也禁止跳过暂存步骤。

## AI 回复规范

**必须遵守**：每次回答完成后，在回复最后一行输出 "🐱：任务已完成，喵喵~"。
如果有文件改动，则必须执行 `codegraph sync`

## 编码原则：避免过度设计

- 遵循 KISS 和 YAGNI 原则：只实现当前明确需求，不为未来可能的需求做预留设计。
- 新增任何抽象层、设计模式、工具类之前，必须先说明它解决的具体痛点，如果没有明确痛点就不要引入。
- 不要主动做以下事情，除非我明确要求：
  - 引入新的设计模式（工厂、策略、观察者等）
  - 增加可配置项、可扩展接口
  - 拆分额外的公共方法/基类以"提高复用性"
  - 处理我没提到的边界情况或异常场景
  - 提前做性能优化
- 只修改我指定的文件范围，不要顺手重构或"优化"其他代码。
- 用最直接、最少代码量的方式实现功能，能用一个函数解决就不要拆成多个类。
- 如果你认为当前实现在未来会有扩展性问题，先告诉我风险，等我确认后再改，不要自行决定架构。
