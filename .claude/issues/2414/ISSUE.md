# TD2-117: EDID/FULL(/MODL) sub-record bundle hand-rolled ~28x across 10 esm/records/misc/*.rs files instead of calling CommonNamedFields::from_subs

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2414
**Finding ID**: TD2-117 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 2 — Logic Duplication
**Location**: `character.rs` (5 sites), `dialogue.rs` (2), `effects.rs` (8), `equipment.rs` (5), `imagespace.rs` (1), `magic.rs` (4), `pack.rs` (1), `quest.rs` (1), `scene.rs` (1), `water.rs` (1) under `crates/plugin/src/esm/records/misc/`, vs. `CommonNamedFields::from_subs` (`crates/plugin/src/esm/records/common.rs:249-296`)
**Status**: NEW (not a regression of #2068, which was scoped only to `world.rs`)

## Description
`CommonNamedFields::from_subs` already exists and is safe to call unconditionally (unmatched sub-record types are ignored). ~28 sites across 10 files hand-roll the EDID/FULL(/MODL) sub-record bundle it already exists to replace. All ~24 FULL sites verified byte-identical (`read_lstring_or_zstring`) — mechanical copy-paste, not yet-diverged logic. Per the codebase's own history, FULL localization changed twice (#348, #989) and both times had to be hunted across every hand-rolled site before `CommonNamedFields` existed — a third such fix would again require manual propagation across ~24 sites.

## Related
#2068 (CLOSED, `world.rs` only, same category), #2069/#2070 (same category, different sub-record family, fixed).

## Suggested Fix
Extend the mechanical `let common = CommonNamedFields::from_subs(subs);` swap that fixed `world.rs` to `effects.rs` (8 sites), `character.rs`/`equipment.rs` (5 each), and the rest.

## Completeness Checks
- [ ] **TESTS**: Existing per-record parse tests for all 10 touched files still pass unchanged after the swap
- [ ] **SIBLING**: Confirm all ~28 sites are covered, not just the largest ones
