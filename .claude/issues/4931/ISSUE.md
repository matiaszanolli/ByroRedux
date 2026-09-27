# EXT-D5-2026-09-27-06: #4734's regression guard was deleted by #4727's commit; the dead-default check is a game-agnostic exact-90° test

**Issue**: #4931
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,bug,water,test-gap

**Severity**: LOW
**Dimension**: Water translation (WATAL)
**Tier Violated**: no-fabrication (latent)
**Game Affected**: FO3/FNV (guarded case); Skyrim/FO4/Starfield (check scope)
**Status**: Regression of #4734 (the guard)
**Location**:
`byroredux/src/env_translate.rs:903-911`; `crates/plugin/src/esm/records/misc/water.rs:315-324`
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

> **Regression**: the guard added by #4734 (closed) was deleted by #4727's commit `a6a210eb9`.

## Description
- `dead_default_wind_direction_yields_no_physics_flow_for_named_creeks` was added by `5f73dae24` and removed by `a6a210eb9`. `git log -S` shows only those two commits.
- `wind_direction_is_dead_default` now has no test.
- The equality check applies to every game; FO4 authors exactly 90.0 on two layers, with no vanilla misfire today.
- A missing or short DNAM's 0.0 default is treated as authored, which is the #3185 class.

## Suggested Fix
- Restore the test with post-#4727 expectations.
- Replace the equality check with a parse-side "authored" flag on the FO3/FNV/Oblivion arm.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
