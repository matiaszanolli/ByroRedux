# #4443: REN-2026-09-16-D7-04: `GpuMaterial`'s per-group offset comments (Rust and GLSL) and three doc sentences still describe the pre-#3909 layout and the pre-growth role count

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4443
- **Labels**: low,renderer,shaders,documentation,doc-rot
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW (documentation; the offset pins are the real guard and they pass)
- **Dimension**: GPU-Struct Layout / Material Table
- **Location**:
  - `crates/renderer/src/vulkan/material.rs`: the group banners inside
    `struct GpuMaterial`, the supplemental-role banner, and `mod material_flag`.
  - `crates/renderer/shaders/include/bindings.glsl`: `struct GpuMaterial`.
  - `docs/engine/shader-pipeline.md`: the `GpuMaterial` section.
- **Status**: NEW
- **Description**: #3909 removed `texture_index` at offset 48 and shifted every
  later field down by 4. The per-field `// offset N` comments were updated; the
  group banners were not.
  - **Rust banners.** They still read "skin_tint … (offsets 144-156)" for fields
    at 140–152, "(offsets 160-172)", "(176-188)", "(192-204)", "(208-220)",
    "(224-236)", "(240-256)", "vec4 #18; offsets 260-280", "vec4 #19; offsets
    276-280", "vec4 #20; offsets 280-292" for `ior` at 276, "vec4 #21; offsets
    284-296", "(offsets 300-344)" for roles at 296–340, "(offsets 348-360)",
    "(offsets 364-392)" and "(offsets 396-428)".
  - **GLSL mirror.** It says "Offset 280" for `ior` (276) and still carries
    "(208-220)", "(224-236)" and "(240-252)".
  - **Role counts.** `crates/renderer/src/vulkan/material.rs` says "Nine of the twelve are sampled". The
    group now holds 16 roles, 13 of them sampled. `docs/engine/shader-pipeline.md` says "The
    twelve entries at 300–344" (296–340), and "included in both draw-command and
    `GpuMaterial` hashing" (there is one byte-hash since #4201).
  - **Flag bits.** `material_flag::EFFECT_LI_SHIFT` says "Bits 11–15 are reserved
    for future single-bit flags", but bits 11, 12, 13, 14 and 15 are all assigned
    (`THIN_GLASS`, `MSN_HAS_AUTHORED_Z`, `SOFT_LIGHTING`, `RIM_LIGHTING`,
    `BACK_LIGHTING`).
  - **GLSL literals.** The `EFFECT_*` block says "the GLSL refers to the same
    `0x...u` literals". The GLSL includes the generated
    `crates/renderer/shaders/include/shader_constants.glsl` instead, and the
    module's own header forbids hand-written literals.
- **Evidence**: `// offset 140` on `skin_tint_a` directly under the "offsets
  144-156" banner; `float ior;` under "Offset 280."; `pub const THIN_GLASS: u32 = 1 << 11;`.
- **Impact**: A contributor adding a field would read offsets that are 4 bytes
  wrong, in the one struct whose layout the skill calls the load-bearing lockstep
  contract.
- **Related**: #3909, #806, #4201, REN-2026-09-05-D7-02
- **Suggested Fix**:
  1. Drop the numeric ranges from the group banners (the per-field comments and
     the pin test already carry them), or regenerate them.
  2. Update the three prose sentences and the `EFFECT_LI_SHIFT` / `EFFECT_*` notes.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
