# Contributing to Faderpunk

This is for external human contributors. If you're an AI coding agent (or a human directing one) working on this repo on the maintainer's behalf, see [`AGENTS.md`](AGENTS.md) instead — it's a separate, agent-facing policy doc; the two aren't the same audience and shouldn't be merged.

## Before you start

- **Adding a new app or fixing an existing one?** See the README's ["Creating a New App"](README.md) walkthrough for the pattern to follow.
- **Anything else** (firmware core, configurator, protocol/libfp, docs, CI): open a PR against `main` following the scope rules below.
- **A standalone tool, not an on-device app** (a preset editor, a diagnostics dashboard, etc.)? See "Standalone companion tools" below before opening a PR — it almost certainly doesn't belong in this repo.

## PR Scope Categories

<!-- KEEP IN SYNC WITH .github/scripts/pr-scope-check.sh — same rules, hand-mirrored. -->

Every PR is automatically classified by what it touches, not by what you say it is:

| Category | What it may touch |
|---|---|
| **New app** | One new file under `faderpunk/src/apps/`, the one-line registration in `faderpunk/src/apps/mod.rs`, and its manual entry (`configurator/src/components/ManualTab.tsx` / `manual/Apps.tsx`) |
| **App fix** | Only the existing app's own file(s) under `faderpunk/src/apps/` |
| **Firmware core** | Core firmware files outside `apps/` (e.g. `app.rs`, `layout.rs`, `tasks/*`, `memory.x`, `.cargo/config.toml`) |
| **Configurator** | Files under `configurator/` |
| **Protocol / libfp** | Shared `libfp/` types and generated bindings — always flagged for a manual look, since these ripple into both firmware and configurator |
| **Docs** | README, docs folder, etc. |
| **CI / tooling** | Workflow and build-tooling files — legitimate on their own, but not bundled with feature work |

**Automatically rejected** (once enforcement is on — see "Rollout status" below):
- Touching a file outside all the categories above — this usually means an unrelated standalone project got bundled into the PR.
- Editing `AGENTS.md` or `CLAUDE.md`. This is strict, with no exception for edits that look related to the same PR's change — if you need to update project/agent policy alongside a code change, please do it as a separate, small PR.
- Bundling `.github/workflows/**` changes together with unrelated feature/fix work.
- An app PR touching `ManualPage.tsx` or `Layout.tsx` (off-limits for app PRs — the manual system's shared page/layout components, not a per-app concern).
- **An app reaching a hardware task directly instead of through the `App<N>` API.** See "Safety boundary" below.

**Flagged for a human to glance at, never blocking:**
- An app PR that also touches shared `libfp/` code, or another app's files.
- A new app PR missing its `mod.rs` registration or a manual entry.
- A diff that spans multiple categories with no clear primary one.
- An unusually large diff (soft threshold only — a legitimate large change is never blocked on size alone).
- Commits mixing `fix:` and `feat:` conventional-commit types (see "One change per PR").
- A PR whose diff appears to significantly overlap another currently-open PR's diff (see "Don't stack PRs on other open PRs").

## Safety boundary: apps must go through the App API

Apps (`faderpunk/src/apps/*.rs`) run on Core 1 and must never import a Core-0 hardware task's internals directly — FRAM storage, the MAX11300 driver, MIDI, I2C, buttons, or the clock — only through the API `app.rs` provides (everything reachable via `use crate::app::{...}`).

Why: apps talk to Core-0 hardware tasks only through the channels/abstractions `app.rs` exposes. Reaching around that facade risks cross-core races, dropped or corrupted state, and (for storage) FRAM corruption.

This is enforced automatically and unconditionally — every one of these areas was individually verified against the codebase to have zero legitimate exceptions. If you need something from a hardware task that `app.rs` doesn't currently expose, that's a real gap: open an issue, or add a small facade method to `app.rs` as part of your PR (see the `LedMode` re-export in PR #635 for the pattern).

## One change per PR

A PR must not combine a bug fix with a new feature — submit them as two independent PRs, even if they touch the same file. This can't be fully verified from a diff alone, so it's a stated norm: declare a Type when opening your PR (Fix / Feature / Refactor / Docs / Chore), and if your commits mix `fix:`- and `feat:`-style messages, the automated check will flag it for a reviewer to double check whether it should be split.

## Don't stack PRs on other open PRs

Branch from `main`. If your app needs a capability that doesn't exist yet — a shared helper, a new facade method, an infra fix — submit that as its own separate PR first, rather than bundling it inline with the feature that needs it.

Why this matters: a stacked PR turns merge order into an undocumented dependency graph (nothing in the PR's description says "wait for #NNN first"), inflates the diff with changes unrelated to the PR's stated purpose, and means every PR stacked on the same base needs rework if that base changes. This is checked with a best-effort automated hint (shared-diff detection against other open PRs) but is fundamentally a discipline norm — the automation can't reliably distinguish deliberate stacking from two people independently touching the same file.

## Standalone companion tools

Tools that aren't part of the on-device app system — preset editors, diagnostic dashboards, and similar companion utilities — should be **hosted by their author in their own repository**, not merged into this monorepo. They bring their own dependency/build footprint and ongoing maintenance burden that doesn't belong here. We're happy to link to community tools like these from our docs so they stay discoverable, without taking on their maintenance.

Longer-term, optional/unofficial apps (as opposed to companion tools) may have a dedicated lower-barrier outlet in a separate, linked repo — not live yet; this section will be updated with a pointer once that's decided.

## Basic PR process

1. Fork the repo, branch off current `main`.
2. Make your change, following the scope rules above.
3. Fill in the PR template — the Category/Type checkboxes are reviewer aids, the scope check always infers category from the diff itself.
4. Make sure CI passes (see "Code quality gate" below).
5. Open the PR.

## What happens if your PR is flagged

- **Soft-flags never block anything.** They show up as an informational note for a reviewer, nothing more.
- **Hard-fails**, once enforcement is turned on, show as a failing check on your PR's Checks tab, with a summary explaining exactly which rule fired and which file(s) triggered it.
- **Rollout status**: this check currently runs in **informational mode only** — nothing is blocked yet, regardless of what it finds. This section will be updated when that changes.
- Dependabot PRs and the automated release PR are exempt from this check entirely.

## Code quality gate

All of the following must pass with **zero** warnings — not "mostly ok":

```bash
cargo fmt --all -- --check
cargo clippy --bin faderpunk --target thumbv8m.main-none-eabihf -- -D warnings
cargo clippy -p libfp -- -D warnings
cargo test --lib -p libfp
```

If you touched the configurator:

```bash
pnpm -C configurator lint
pnpm -C configurator build
```
