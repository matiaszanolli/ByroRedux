# MAT-D1-NEW-02: draw_command_eligible_for_tlas excludes MATERIAL_KIND_EFFECT_SHADER but not MATERIAL_KIND_FIRE_REFRACTION

Source: `docs/audits/AUDIT_NIFAL_2026-08-03.md`

**Severity**: LOW
**Dimension**: Material · **Tier Violated**: no-render-time-fallback (defense-in-depth)
**Location**: `draw_command_eligible_for_tlas`, `crates/renderer/src/vulkan/acceleration/predicates.rs:437-441`
**Status**: NEW

## Description

`draw_command_eligible_for_tlas` excludes `MATERIAL_KIND_EFFECT_SHADER` from
the TLAS but not `MATERIAL_KIND_FIRE_REFRACTION`, despite the latter's own
constant doc requiring the same exclusion. A sibling predicate elsewhere in
the same file (`predicates.rs:610`, a different eligibility check) *does*
exclude `MATERIAL_KIND_FIRE_REFRACTION`, confirming the omission here is an
asymmetry rather than a deliberate design choice.

## Evidence

```rust
pub(super) fn draw_command_eligible_for_tlas(draw_cmd: &DrawCommand) -> bool {
    draw_cmd.in_tlas
        && !draw_cmd.is_water
        && draw_cmd.material_kind != crate::MATERIAL_KIND_EFFECT_SHADER
}
```
vs. `predicates.rs:610`: `&& material_kind != crate::vulkan::scene_buffer::MATERIAL_KIND_FIRE_REFRACTION`.

## Impact

No live defect — no current producer sets `in_tlas` for fire-refraction draw
commands, so this predicate is never exercised on that material kind today.
Defense-in-depth gap only: if a future producer path did mark a
fire-refraction draw `in_tlas`, this function would let it into the TLAS
against its own constant's documented contract, while the sibling predicate
correctly excludes it.

## Suggested Fix

Add the same `MATERIAL_KIND_FIRE_REFRACTION` exclusion to
`draw_command_eligible_for_tlas`, mirroring the sibling predicate at line 610.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other TLAS-eligibility predicate already gets this right — mirror it here)
- [ ] **TESTS**: A regression test pins this specific fix

## Filed as

GitHub issue #2297, labels: low, renderer, bug.
