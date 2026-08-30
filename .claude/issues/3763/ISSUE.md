# #3763 — SAFE-2026-08-30-D6-02: `GpuLight` has no Rust↔GLSL lockstep leg and no size pin — and a sibling test's doc claims it does

**Labels**: bug, renderer, medium, shaders, test-gap

---

- **Severity**: MEDIUM
- **Dimension**: 6 — R1 material table layout (`GpuLight` sub-check)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` (`gpu_light_glsl_copies_stay_in_lockstep`, and the false claim in the `gpu_instance_glsl_copies_stay_in_lockstep` doc); `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`pub struct GpuLight`)
- **Source**: `docs/audits/AUDIT_SAFETY_2026-08-30.md` (`SAFE-D6-02`), HEAD `64f64480`

## Description

`GpuMaterial` and `GpuInstance` each have **two** legs of protection: mirror-vs-mirror
across the GLSL copies, **and** mirror-vs-Rust field order/type against the `#[repr(C)]`
struct (`gpu_material_glsl_field_order_matches_rust_struct`,
`gpu_instance_glsl_copies_stay_in_lockstep`).

**`GpuLight` has only the first leg.** `gpu_light_glsl_copies_stay_in_lockstep` walks the
four GLSL declarations (`include/bindings.glsl`, `cluster_cull.comp`, `caustic_splat.comp`,
`volumetrics_inject.comp`) and asserts they equal **each other** — it never looks at
`gpu_types.rs`.

There is additionally **no `size_of::<GpuLight>()` assertion anywhere in the crate**
(re-verified at HEAD: 0 hits), unlike `GpuInstance` (160 B) and `GpuCamera` (368 B).

Consequently: **appending or reordering a field on the Rust `GpuLight` while leaving all
four GLSL copies untouched passes every test in the workspace**, and the CPU would then
upload a stride the shaders do not decode.

## Evidence

- The `gpu_light_glsl_copies_stay_in_lockstep` loop body only ever compares `fields` to
  `reference`, both of which come from `SOURCES` (four GLSL paths). No
  `include_str!("../gpu_types.rs")`, no `parse_rust_struct_fields` — in contrast to the
  `GpuMaterial` and `GpuInstance` guards.
- **`shader_contract_tests.rs:1748` states the `GpuInstance` guard delivers "the same
  two-leg coverage `GpuMaterial` and `GpuLight` already have" — factually wrong for
  `GpuLight`** (verbatim at HEAD: `/// \`GpuMaterial\` and \`GpuLight\` already have.`).
  That is exactly the kind of claim that stops a future author from adding the missing leg.
- `grep -rn 'GpuLight' crates/renderer/src | grep -iE 'assert|size_of'` returns only
  *consumers* that derive sizing from `size_of::<GpuLight>()` (`upload.rs` ×2,
  `buffers.rs`, `context/mod.rs`) — self-consistent on the CPU side, so a Rust-only change
  silently propagates to the buffer stride with **no test failure**.
- Current state is correct: Rust `position_radius / color_type / direction_angle / params`
  (four `[f32; 4]`, 64 B) matches all four GLSL copies verbatim. **This is a missing guard,
  not a live drift.**

## Impact

A `GpuLight` field addition or reorder that touches only the Rust side (or only some subset
of a future fifth GLSL copy) **ships green**. The failure mode is silent per-light data
corruption in the clustered-lighting, caustic and volumetric passes — the class the
severity table rates HIGH when it actually happens (`#[repr(C)]` GPU struct size/layout
drifts from shader struct). Rated MEDIUM here because nothing is currently drifted; this is
the defence-in-depth gap that would let it happen unnoticed. `GpuLight` is also the *most*
mirrored struct after `GpuInstance` (four copies), and **#1916 already fired once on this
exact struct**.

## Suggested Fix

Extend `gpu_light_glsl_copies_stay_in_lockstep` with the second leg, **reusing machinery
already in the module**:
`parse_rust_struct_fields(include_str!("../gpu_types.rs"), "pub struct GpuLight")` →
`normalize_ident` → assert count + order against the shared GLSL field list, mirroring
`gpu_instance_glsl_copies_stay_in_lockstep`. Add `assert_eq!(size_of::<GpuLight>(), 64)`
alongside `gpu_instance_is_160_bytes_std430_compatible` in `gpu_instance_layout_tests.rs`.
**Correct the false claim at `shader_contract_tests.rs:1748` in the same change** (or delete
the `GpuLight` mention from it).

Per `feedback_shader_struct_sync.md`, this struct sits on the codebase's highest-stated
lockstep-drift risk surface.

## Related

#1916 (CLOSED — added the four-way GLSL leg), #2748 (CLOSED — added the Rust leg for
`GpuInstance` after finding its guard was presence-only), #1657 (CLOSED — added the Rust
leg for `GpuMaterial`), #1810 (CLOSED — stale 48 B `GpuLight` byte-math).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — audit every mirrored GPU struct for which legs it actually has, not just `GpuLight`
- [ ] **TESTS**: A regression test pins this specific fix (the fix *is* the test)
