# REN-D3-2026-09-21-02: `MeterParams` ↔ `exposure_meter.comp` `Params` is missing from the SPIR-V UBO block-size table

**Labels**: low, renderer, shaders, test-gap, bug

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: LOW (test gap; the two layouts match today) · **Dimension**: GPU-Struct
**Location**: `crates/renderer/src/vulkan/reflect.rs` `every_remaining_uniform_block_size_matches_its_host_struct` (~:746); `crates/renderer/src/vulkan/exposure_meter.rs` `MeterParams` (~:36) and `meter_params_are_two_vec4s` (~:367); `crates/renderer/shaders/exposure_meter.comp` `uniform Params` (binding 2)
**Status**: NEW (introduced by `c5663fe39`)
**Verified against**: HEAD `f97775ca8`

## Description

`every_remaining_uniform_block_size_matches_its_host_struct` pins each host UBO mirror's size against the block size in the committed `.spv`. It covers `CompositeParams`, `TaaParams`, `CausticParams`, `SvgfTemporalParams`, `SSAOParams` and the two bloom `Params` blocks.

The new exposure-meter UBO is not in the table. That is `MeterParams` on the host and `layout(set = 0, binding = 2) uniform Params` in `exposure_meter.comp`. `meter_params_are_two_vec4s` asserts only the Rust size (32 B). So a GLSL-side change to `Params`, or a stale `exposure_meter.comp.spv`, would not fail any test.

The audit checked the two blocks field by field, and they match at HEAD.

## Evidence

- `grep -n "exposure_meter\|MeterParams" crates/renderer/src/vulkan/reflect.rs` finds no match.
- `exposure_meter.rs` has `fn meter_params_are_two_vec4s() { assert_eq!(std::mem::size_of::<MeterParams>(), 32); }`.

## Impact

`cargo test` would not detect future drift between the host struct and the shader block. It would surface only as wrong exposure, or through CI's parity job if the `.spv` is stale. No runtime effect today.

## Related

- #1493 and #2464 (closed): earlier UBOs added to the reflection table for the same reason.
- REN-D3-2026-09-21-01 (#4584): the other Stage-1 GPU-contract pin gap.

## Suggested Fix

Add `("exposure_meter.comp", include_bytes!("../../shaders/exposure_meter.comp.spv"), "Params", size_of::<MeterParams>())` to the table. Consider a guard that fails when a new host UBO mirror is added without a table row.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D3-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: other UBO mirrors outside the table (e.g. `SkyCubeParams`, which has its own field-lockstep test in `sky_cube.rs`) checked for block-size coverage
- [ ] **TESTS**: the new table row fails when `exposure_meter.comp`'s `Params` block changes size
