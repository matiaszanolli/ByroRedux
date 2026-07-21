# D22-3: spawn.rs duplicates attach_light_flicker_if_needed's body instead of calling it

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2121
**Labels**: bug, animation, low, tech-debt

**Severity**: LOW (tech-debt)
**Dimension**: Light Animation (audit-renderer Dimension 22)
**Location**: `byroredux/src/cell_loader/spawn.rs` (~lines 1417-1436)
**Status**: NEW

**Description**: The meshed-placement path in `spawn.rs` re-implements the exact period-fallback (`> 0.0 else 0.5`) + `phase_offset_secs` (`entity.wrapping_mul(2654435761) ...`) + `LightFlicker` construction that `attach_light_flicker_if_needed` (`byroredux/src/cell_loader/references/attach.rs`) already provides. The two copies must be kept in lockstep by hand (e.g., the 0.5s period default). Not a live bug today — both receive already-canonicalized `light_animation_flags` — but a drift hazard per the standing "improve existing code rather than duplicate logic" guidance.

**Evidence**:
```rust
// byroredux/src/cell_loader/spawn.rs, ~line 1417
if light_animation_flags != 0 {
    ...
    LightFlicker {
        animation_flags: light_animation_flags,
        ...
    }
}
```
Duplicates the body of `attach_light_flicker_if_needed` in `references/attach.rs`.

**Impact**: No functional bug currently. Risk is future edits to one copy (e.g. tuning the default period) silently not propagating to the other.

**Related**: Same audit dimension as #2118 and #2120.

**Suggested Fix**: Have `spawn_placed_instances` call `attach_light_flicker_if_needed(world, entity, ld, ref_pos, light_animation_flags)` rather than inlining the body.

## Completeness Checks
- [ ] **TESTS**: Existing tests for both call sites should still pass unchanged after de-duplication (behavior-preserving refactor)

Filed from `docs/audits/AUDIT_RENDERER_2026-07-20.md`.
