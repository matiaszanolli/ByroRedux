# SAVE-D6-02: save and load have no non-console entry point

**Issue**: #3026
**Severity**: MEDIUM
**Dimension**: 6 — engine integration
**Labels**: `medium,tech-debt,enhancement`
**Source report**: `docs/audits/AUDIT_SAVE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAVE_2026-08-16.md` (Dimension 6 — engine integration).

**Location**: `byroredux/src/commands/mod.rs`:113-115 (the only registration of `SaveCommand` / `SaveInfoCommand` / `LoadCommand`) · `byroredux/src/interaction.rs`:51-64 (`InputAction` — no Save/Load/Quicksave variant) · `byroredux/src/cli_args.rs` (no `--load`)

## Description

Save and load have **no non-console entry point** — no action binding, no CLI flag, no menu item. The only way to save or load is to attach `byro-dbg` and type a command.

## Evidence

Re-verified 2026-08-17:
- `InputAction` has no `Save`, `Load` or `Quicksave` variant
- `grep -c "\-\-load" byroredux/src/cli_args.rs` → **0**
- The three commands are registered only in `commands/mod.rs`:113-115
- The native game menu (`crates/debug-ui`, `GameMenuPage::{Pause, Settings, Inventory}`) has no save/load page

## Impact

M45/M45.1 shipped a full-ECS snapshot with atomic write, a ring buffer and live load-apply — and none of it is reachable while actually playing. `docs/engine/playable-vertical-slice.md` gate 5 requires save/reload continuity, which cannot be exercised through any player-facing path.

This is a missing integration rather than a defect in the save system itself, which is why it is MEDIUM and typed as an enhancement.

## Suggested Fix

Add a `Quicksave` / `Quickload` `InputAction` with default bindings, a `--load <slot>` CLI flag, and a save/load entry in the native game menu — the menu already exists and has an Inventory page to model against.

## Related

- #3009 (RT-2026-08-16-10 — the same "no runtime surface" gap for inventory/settings)
- #2975 (TD3-2026-08-16-01 — the native menu the save entry would live in)

## Completeness Checks
- [ ] **SIBLING**: All three surfaces (binding, CLI, menu) considered — not only the quickest one
- [ ] **GATE-5**: `playable-vertical-slice.md` gate 5 becomes exercisable afterwards
- [ ] **RING-BUFFER**: The player-facing path respects the existing ring/slot semantics rather than inventing new ones
- [ ] **TESTS**: A smoke gate saves and reloads through the non-console path

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3026 --json state` when live state is needed.*
