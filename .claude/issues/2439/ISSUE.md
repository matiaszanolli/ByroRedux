# NIFAL-D2-01: ESM-sourced lights never reach LightKind — three of four canonical LightSource producers hard-default to Point

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2439
**Finding ID**: NIFAL-D2-01 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 2 — NIFAL mapping shape
**Location**: `byroredux/src/cell_loader/spawn.rs:1734`, `byroredux/src/cell_loader/references/mod.rs:1224,1310` (producers); `crates/plugin/src/esm/cell/support.rs:85-133` + `crates/plugin/src/esm/cell/mod.rs:565-600` (dropped authored signal)
**Status**: NEW

## Description
`nifal.md` §2 marks "Lights — converged", true only for the NIF half. There is no `translate_light` boundary: the canonical `LightSource` is constructed at four independent sites, and only the direct-`NiPointLight` path populates `kind`/`direction`/`outer_angle`. The three ESM-LIGH-sourced producers hand-copy scalar fields and take `..Default::default()` → `LightKind::Point`. The authored spot signal is reachable — `LIGHT_FLAG_SHADOW_SPOTLIGHT` (0x400) survives into `LightSource.flags`, and the LIGH `DATA` cone angle at bytes 20-23 is named in the parser's own layout comment ("FOV (spot light)") but never read into `LightData`. Same shape as the already-fixed #2205, one tier up.

## Evidence
`render/lights.rs:200-231` correctly consumes `light.kind` (Spot → cone math) — the consumer is ready and unused. Confirmed directly: all three ESM-LIGH `LightSource { ... }` literals end with `..Default::default()` and never set `kind`; `LightSource::default()` (`crates/core/src/ecs/components/light.rs:93`) sets `kind: LightKind::Point`.

## Impact
Every ESM-placed spotlight in every supported game renders as a full omnidirectional point light over its authored radius — cone-directed lanterns, searchlights, FO4/Skyrim directed fixtures spill light backwards through their own housings.

## Related
#2205 (CLOSED, NIFAL-D3-01 — the direct-NIF half of this same shape).

## Suggested Fix
Read LIGH `DATA` bytes 20-23 into `LightData.fov_degrees` (and Starfield's `DAT2` equivalent); introduce a `translate_light(ld, game) -> LightSource` boundary beside `canonical_light_shadow_flags` deriving `kind` from the spot flag + FOV; collapse all four producers onto it.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Introduces a new single-producer boundary (`translate_light`) — all four existing producer sites must collapse onto it, not add a fifth
- [ ] **TESTS**: A regression test spawns an ESM-LIGH spotlight and asserts `LightSource.kind == LightKind::Spot` with the authored FOV
- [ ] **SIBLING**: Verify Starfield's `DAT2` cone-angle equivalent is handled the same way
