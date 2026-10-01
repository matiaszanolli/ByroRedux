# 4917: EXT-D1-2026-09-27-06: The spawner guard's stripper treats an out-of-line `#[cfg(test)] mod x;` as a block

labels: bug, low, terrain-exterior, test-gap
state: OPEN

**Severity**: LOW (latent guard hole)
**Dimension**: EXAL boundary discipline
**Tier Violated**: n/a
**Game Affected**: all
**Status**: NEW
**Location**:
`byroredux/src/material_translate.rs:2606-2618` (`strip_inline_test_modules`)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- The pattern `"\n#[cfg(test)]\nmod "` also matches `mod foo_tests;` and strips up to the next column-0 `}`.
- There are 34 such declarations in the spawner roots. Each swallows 1–43 following lines, all test code today.
- A production spawner placed after one would drop out of the scan. This is the #4302/#4856 failure mode.

## Suggested Fix
Strip only `mod <ident> {` blocks.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix

