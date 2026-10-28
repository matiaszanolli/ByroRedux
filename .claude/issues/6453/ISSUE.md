# EXT-D6-2026-10-27-03: terrain_lod_albedo_gain's legacy rows are unreachable, because the synth consumer re-gates on `combined`

Labels: low, terrain-exterior, bug, game:oblivion, game:fo3, game:fnv
Source: docs/audits/AUDIT_EXTERIOR_2026-10-27.md

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-27.md` finding `EXT-D6-2026-10-27-03` (Dimension 6, consumer of the Dimension 1 table) · Severity LOW (latent; every legacy row is 1.0 today) · Status NEW · Tier violated: single-boundary.

**Location (current)**: `byroredux/src/cell_loader/terrain_lod.rs:1323` (`if translated_lod.is_some() && combined { material.diffuse_color = [terrain_lod_albedo_gain(game); 3]; }`); table `byroredux/src/env_translate/lod.rs` (`terrain_lod_albedo_gain`, ~:72-77).

## Description
Every legacy LOD block (Oblivion, FO3, FNV) is a synth block wearing its authored baked quad (`translated_lod = translate_terrain_lod_textures(..)`). That is exactly the case the code comment says takes the gain ("a block wearing its quad's baked diffuse"). The table documents FNV as "1.0, measured" and Oblivion as "unmeasured, keep until measured". Because of the `&& combined` term, a future measured value for those games would never reach a block. The tiled-LTEX fallback is already excluded by `translated_lod.is_some()`.

This is a consumer-side game-family decision that shadows the EXAL table's legacy rows.

## Evidence
```rust
// A block wearing its quad's baked diffuse takes the same gain as the
// prebaked `.btr` blocks beside it ...
if translated_lod.is_some() && combined {
    material.diffuse_color = [crate::env_translate::terrain_lod_albedo_gain(game); 3];
}
```
```rust
pub(crate) const fn terrain_lod_albedo_gain(game: GameKind) -> f32 {
    match game { GameKind::Skyrim => 0.36, _ => 1.0 }
}
```

## Impact
No effect today (all legacy rows are 1.0). A future per-game measurement for Oblivion/FO3/FNV silently does nothing, and the table reads as authoritative when it is not.

## Related
#6407 (single predicate precedent), EXAL docs `docs/engine/exal.md`.

## Suggested Fix
Drop `&& combined`, or state in the table's doc that it applies to the Combined family only.

## Completeness Checks
- [ ] **SIBLING**: Other consumers of `terrain_lod_albedo_gain` (the `.btr` path in `terrain_lod_btr.rs`) checked for the same re-gate
- [ ] **CANONICAL-BOUNDARY**: Per-game decision stays in the EXAL table, not re-derived by the consumer
- [ ] **TESTS**: A pin asserts the gain reaches a legacy synth block when the table row is non-1.0

