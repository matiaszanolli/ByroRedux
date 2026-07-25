# 2142: RL-D6-02: Water-caustic accumulator resize failure leaves WaterPipeline set 2 bound to a destroyed storage image view

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2142
**Labels**: bug, medium, vulkan

---

## Severity
MEDIUM

## Dimension
Resource Lifecycle (GPU teardown ordering) — `/audit-concurrency` 2026-07-25

## Location
`crates/renderer/src/vulkan/context/resize.rs:614-657`, `crates/renderer/src/vulkan/water.rs:455-466`

## Description
On the two failure arms of the water-caustic resize block, the accumulator is destroyed and `self.water_caustic_accum` set to `None`; the rebind is guarded by an `if let (Some(w), Some(accum))` and is therefore skipped, leaving `WaterPipeline::water_caustic_descriptor_sets[frame]` binding 0 holding the destroyed per-FIF storage view. `record_draw` binds set 2 **unconditionally**, and the geometry pass gates the water draw only on `self.water.is_some()` — never on the accumulator. This is strictly worse than RL-D6-01 because the access is a shader **write** (`imageAtomicAdd`).

## Evidence
`resize.rs:633-634` and `:644-645` destroy + null the accumulator on both failure arms; `:652-657` rebind is skipped when `accum` is `None`; `water.rs:459-466` binds set 2 with no `Option` gate; `context/geometry_pass.rs:512-542` gates the water draw only on `self.water`. The init-path twin at `context/mod.rs:2105-2113` carries a stale safety comment claiming the shader-side gate (`sunDirection.w > 0`) protects an unwritten set 2 during a "scaffold-only window" — but Phase D and Phase E (#1255/#1257) have both shipped, so that window is closed.

## Impact
Post-failure, every exterior/water frame binds a descriptor set whose storage image was freed and issues an atomic write against it. Failure-path-only → MEDIUM.

## Related
#1255/#1210 Phase C, sibling of RL-D6-01 (filed separately); also refreshes the stale comment at `context/mod.rs:2105-2113`.

## Suggested Fix
Either gate the set-2 bind + water draw on accumulator presence, or keep a 1×1 R32_UINT dummy storage image owned by `WaterPipeline` and rebind set 2 to it whenever the accumulator drops out (covers both the resize-failure and init-failure arms). Update the stale `mod.rs:2105-2113` comment either way.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
