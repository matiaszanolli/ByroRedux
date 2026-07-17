# SAVE-D1-NEW-02: restore_world's release-mode insert_batch bound-check gap is real but currently dormant

**Labels**: low, ecs, bug

**Severity**: LOW (dormant — no reachable production path)
**Dimension**: Snapshot Completeness & Determinism
**Source**: `docs/audits/AUDIT_SAVE_2026-07-16.md`

## Location
`crates/core/src/ecs/world.rs:204-209` (`insert_batch`'s `debug_assert`, compiled out under `--release`); `crates/save/src/driver.rs:78-108` (`restore_world`)

## Description
If a decoded snapshot's `next_entity` were ever smaller than the highest entity id in one of its columns (a hand-tampered-but-CRC-valid file, or a hypothetical future `save_world` bug), `restore_world` would admit those rows silently in release builds — `insert_bulk` does no bounds check of its own, and the `entity < next_entity` guard is `debug_assert`-only.

Verified current: `insert_batch`'s `debug_assert!(entity < next_entity, ...)` is compiled out under `--release`; all non-test call sites of `restore_world` (byroredux/src/save_io.rs) are inside `#[cfg(test)]` modules — the live `load` path uses `restore_resources` + `apply_deltas` instead.

## Impact
None today. Every non-test call site of `restore_world` is inside `#[cfg(test)]` modules; the live `load` path uses `restore_resources` + `apply_deltas`, which only insert through a remap table of already-live entity ids. Defense-in-depth gap on a currently-unreachable path.

## Suggested Fix
If `restore_world` (or an equivalent raw-id restore) is ever wired to a live command — the crate's own docs anticipate a future "loose/exterior save" `load` variant — promote the check to a real `Result`-returning validation rather than relying on the debug-only assert.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (once/if `restore_world` gains a live call site)
