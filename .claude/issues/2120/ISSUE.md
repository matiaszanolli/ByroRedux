# D22-2: fxlight/fxlightrays/fxfog LIGH placements never get a LightFlicker attached

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2120
**Labels**: bug, animation, import-pipeline, low

**Severity**: LOW
**Dimension**: Light Animation (audit-renderer Dimension 22)
**Location**: `byroredux/src/cell_loader/references/mod.rs`, the `fxlightrays`/`fxlight`/`fxfog` effect-mesh branch (~lines 987-1008)
**Status**: NEW

**Description**: The effect-mesh branch spawns a real `LightSource` (with `flags: ld.flags`) but — unlike the light-only branch (calls `attach_light_flicker_if_needed` with `canonical_light_animation_flags`) and the meshed-placement branch (passes canonicalized flags into `spawn_placed_instances`) — it never computes canonical animation flags and never attaches a `LightFlicker`. Confirmed by direct read: the branch spawns the entity, inserts `Transform`/`GlobalTransform`/`LightSource`, and returns, with no flicker-related call at all.

**Evidence**:
```rust
// byroredux/src/cell_loader/references/mod.rs, ~line 987
if model_lower.contains("fxlightrays") || model_lower.contains("fxlight") || model_lower.contains("fxfog") {
    if let Some(ref ld) = stat.light_data {
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(ref_pos));
        world.insert(entity, GlobalTransform::new(ref_pos, Quat::IDENTITY, 1.0));
        world.insert(entity, LightSource { radius: ..., color: ld.color, flags: ld.flags, falloff_exponent: ..., ..Default::default() });
        accum.entity_count += 1;
    }
    return;
}
```
No `canonical_light_animation_flags` call, no `attach_light_flicker_if_needed` call.

**Impact**: Effect-halo lights (torch glow sprites, fog-light fx, etc.) with authored flicker/pulse render as steady lights instead. This does NOT reintroduce the raw-flag decode bug (no animation runs at all here) — it's a completeness gap, not a correctness regression. Narrow, visual-only.

**Related**: Same dimension as #2118 (LIGHT_FLAG_PULSE_SLOW mis-assignment) and the spawn.rs duplication issue filed alongside this one.

**Suggested Fix**: Mirror the light-only branch: `let af = canonical_light_animation_flags(game, ld.flags); attach_light_flicker_if_needed(world, entity, ld, ref_pos, af);`.

## Completeness Checks
- [ ] **TESTS**: A regression test confirms an fx-light branch with authored flicker/pulse flags gets a `LightFlicker` component after the fix

Filed from `docs/audits/AUDIT_RENDERER_2026-07-20.md`.
