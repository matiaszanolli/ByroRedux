# Renderer Audit — Dimension 14: Material Table (R1) — 2026-05-22

## Executive Summary

Zero new correctness findings. One LOW (coverage-gap) and one LOW (technical-debt
followup already documented in source). The R1 refactor is materially closed:
size pin (260 B), per-field offset pin (65 named offsets), GLSL-name pin,
overflow path, dedup contract, mirror-check guards for `ui.vert` + water shaders +
shader-struct-sync are all in place and passing (278/278 renderer-lib tests
green, 18/18 in `vulkan::material::tests`).

The two prior Dim14 audits (2026-05-14, 2026-05-18) are fully resolved.

## Status of 2026-05-18 Findings

- REN-D14-NEW-01 (water.vert stale Rust-struct path): **FIXED** — `water.vert:38-43` now cites
  `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` and references the
  Session-34 split (`#1187`).

## RT Pipeline Assessment — Material Table

The material-table contract holds for ray-query material reads. Three sites in
`triangle.frag` read the SSBO:

| Site (triangle.frag) | Context | Pattern |
|---|---|---|
| `:505`  | Primary ray hit shading | `GpuMaterial hitMat = materials[hitInst.materialId];` |
| `:810`  | Rasterizer fragment main | `GpuMaterial mat = materials[inst.materialId];` |
| `:1882` | Secondary RT hit (reflection/GI) | `GpuMaterial tMat = materials[tInst.materialId];` |

All three index by `instance.materialId` (the contracted indirection). The
"first scene material aliased by `materialId == 0`" footgun is closed by `#807`
(seeded neutral default at slot 0) — RT rays that hit instances with no
explicit material (default-init UI quad, future synthetic geom) get a safe
neutral GpuMaterial rather than aliasing whichever user material happened to
intern first.

The SSBO is bound at `set 1, binding 13` for the triangle pipeline; the water
and caustic pipelines deliberately do NOT bind it (push-constant block / set-0
descriptor respectively). Both exclusions are pinned by tests.

## Rasterization Assessment

- `triangle.vert` — reads only `model`, `vertexOffset`, `boneOffset`, `flags` from
  `GpuInstance`; no material read. Carries the `materialId` slot for layout parity.
- `triangle.frag` — sole declarant of `struct GpuMaterial`; three indexed reads (see above).
- `ui.vert` — reads `inst.textureIndex` (per-instance, NOT material table).
  Pinned by `ui_vert_reads_texture_index_from_instance_not_material_table` at
  `gpu_instance_layout_tests.rs:304-336`. Forbids `buffer MaterialBuffer`,
  `struct GpuMaterial`, and `materials[inst` indexing.
- `water.vert` / `water.frag` — material params live in the `WaterPush` push
  constant block. Pinned by `water_shaders_must_not_acquire_material_buffer_binding`
  at `gpu_instance_layout_tests.rs:346-370`.
- `caustic_splat.comp` — reads `avgAlbedoR/G/B` from `GpuInstance` (NOT material
  table). Comment at `caustic_splat.comp:75-81` documents the deferred migration
  (closed `#1098 / #1111`, parked as low-priority technical debt — see findings).

The 5-shader lockstep contract for `struct GpuInstance` is enforced by
`every_shader_struct_gpu_instance_names_material_kind_slot` at
`gpu_instance_layout_tests.rs:198-284`.

## Findings

### CRITICAL

None.

### HIGH

None.

### MEDIUM

None.

### LOW

#### REN-D14-NEW-02 — Identity-invariant regression test absent

- **File:** `crates/renderer/src/vulkan/material.rs` (test module)
- **Severity:** LOW (coverage gap, not a correctness defect)
- **Observation:** Item 11 of the spec asks whether render output for N copies
  of the same material is byte-identical pre/post R1 dedup. No test pins this
  end-to-end. The closest existing coverage is
  `intern_by_hash_dedups_identical_materials` at `context/mod.rs:2596` which
  pins "two DrawCommands with identical material fields hash to the same id";
  the missing link is "rendering the resulting frame produces byte-identical
  pixels to the pre-R1 path". A RenderDoc capture diff would close it but
  isn't an automated regression guard.
- **Why-bug:** No machine-checked invariant guarantees that future edits to
  `to_gpu_material` won't silently change rendered output for a material that
  still dedups to the same id. Risk surface is narrow (the hash equality at
  `material_hash_matches_gpu_material_field_hash` covers byte-equality of the
  GpuMaterial itself), but the shader-side read path is uncovered by automated
  tests.
- **Fix:** **Deferred — needs runtime verification** with RenderDoc, or a
  golden-frame test that loads a fixed scene and asserts a screenshot hash.
  Both require a Vulkan device and are out of scope for `cargo test`. Document
  as a coverage gap rather than fixing speculatively.

#### REN-D14-NEW-03 — `caustic_splat.comp` still reads `avgAlbedo*` per-instance (Phase 6 leftover)

- **File:** `crates/renderer/shaders/caustic_splat.comp:75-93`, `gpu_types.rs:96-108`
- **Severity:** LOW (documented technical debt, not a defect)
- **Observation:** `GpuInstance.avg_albedo_r/g/b` + `_pad_albedo` occupy 16 B
  (offsets 96..112) of every instance. The caustic compute pipeline reads them
  from its own descriptor set (set 0 binding 5) instead of via
  `materials[inst.materialId]`. The `triangle.frag` GI-miss path also samples
  them from `GpuInstance` (off the rasterizer's set 1 binding 4). Both reads
  are documented in source (`gpu_types.rs:97-108`, `caustic_splat.comp:75-81`)
  and the underlying issue is referenced as closed (`#1098`, `#1111`).
- **Why-bug:** Item 12 (Phase 6 closeout) — `avg_albedo_*` semantically belongs
  in `GpuMaterial` (it's a per-material constant), not per-instance. Keeping
  it per-instance wastes 16 B × `MAX_INSTANCES` (262144) = 4 MB per frame of
  SSBO bandwidth on data that dedups in practice. The retained-on-instance
  rationale is "migrating caustic_splat.comp needs a new MaterialBuffer
  binding on the caustic descriptor set" — true, but it remains debt.
- **Fix:** Wire `MaterialBuffer` into the caustic descriptor set (set 0, new
  binding); rewrite `caustic_splat.comp` to read
  `materials[inst.materialId].avgAlbedo*`; drop the four fields from
  `GpuInstance` (112 → 96 B); update all 5 shader mirrors in lockstep. Track
  as the natural sibling of `#973` (MSWP swap migration) rather than filing as
  a new issue — the source comments at `gpu_types.rs:99-104` already capture
  the plan.

## Verified-Clean List

- **Item 1** (260 B size pin): `gpu_material_size_is_260_bytes` at
  `material.rs:727-730`. Passes. Comment block at `material.rs:38-45` documents
  the 272 → 260 B change from `#804`.
- **Item 2** (per-field offset pin): `gpu_material_field_offsets_match_shader_contract`
  at `material.rs:810-922`. **65 `offset_of!` assertions** counted, spanning all
  16 vec4 groups (offsets 0..256 + the trailing `greyscale_lut_index` at 256).
  The `#890 Stage 2c` `greyscale_lut_index` assertion is present (`:921`).
- **Item 3** (all scalar, no `[f32; 3]`): grep of `material.rs:65-196` confirms
  every field is `pub <name>: f32` or `pub <name>: u32`. No `[f32; N]` arrays
  in the struct. The doc comment at `:48-51` calls this out explicitly.
- **Item 4** (Hash/Eq on raw bytes; no uninit): `as_bytes` at `:372-380`,
  `PartialEq` at `:383-387`. `to_gpu_material` at `context/mod.rs:320-407` is
  a struct-literal initialiser — every field assigned by name, no
  `MaybeUninit`, no `unsafe { mem::zeroed() }`. SAFETY comment at `:497` in
  `upload.rs` reaffirms it.
- **Item 5** (stable intern, over-cap → 0 with one-shot warn):
  `intern_overflow_returns_material_zero` + `intern_overflow_persists_across_clear`
  at `material.rs:1252-1331`. Warn callsite confirmed at `material.rs:661-666`
  (`log::warn!` inside `INTERN_OVERFLOW_WARNED.call_once`).
- **Item 6** (per-frame upload, sized to `min(intern_count, MAX_MATERIALS)`):
  `upload_materials` at `upload.rs:458-512`. `debug_assert!` at `:469-474`
  pins "intern() should have capped"; `let count = materials.len().min(MAX_MATERIALS)`
  at `:475` is the truncation belt-and-braces. Dirty-gate via
  `hash_material_slice` at `:488-491` skips no-op re-uploads (DIM8-01 / #878).
- **Item 7** (dedup-ratio telemetry): `ctx.scratch` command at
  `commands.rs:310-362` surfaces `materials_unique / materials_interned` ratio
  via `ScratchTelemetry.materials_*` (lines `:353-358`). NOTE: the spec
  describes this as `tlm` — actual command name is `ctx.scratch`. Functional
  equivalent.
- **Item 8** (`material_id: u32` on Phase-3+ struct, legacy fields gone):
  `GpuInstance` at `gpu_types.rs:57-110` is 112 B with `material_id` at
  offset 88. Pinned by `gpu_instance_is_112_bytes_std430_compatible` and
  `gpu_instance_field_offsets_match_shader_contract` at
  `gpu_instance_layout_tests.rs:24,58`. Forbidden-reappearance grep for
  25 legacy field names lives at `:239-283` of the same file.
- **Item 9** (shader-side same offsets, `ui.vert` regression closed): GLSL
  field names pinned by `gpu_material_glsl_field_names_pinned` at
  `material.rs:758-794` (`triangle.frag:89-126` is the GLSL source-of-truth).
  `#776 / #785` regression closed by
  `ui_vert_reads_texture_index_from_instance_not_material_table` at
  `gpu_instance_layout_tests.rs:304-336`.
- **Item 10** (exactly 5 shaders declare `struct GpuInstance`):
  `grep -l "struct GpuInstance" crates/renderer/shaders/` returns
  `triangle.vert`, `triangle.frag`, `ui.vert`, `water.vert`,
  `caustic_splat.comp`. Confirmed.
- **Item 12 (partial)** (Phase-6 leftovers): see REN-D14-NEW-03. The other
  per-instance slots that survive (`model`, `texture_index`, `bone_offset`,
  `vertex_offset/count`, `flags`, `material_id`) are all genuinely
  per-DRAW data — texture_index is justified at `gpu_types.rs:61-66` (UI
  quad path).

## Prioritized Fix Order

Both findings are LOW. No urgent action.

1. **REN-D14-NEW-03** — only if the 16 B/instance × 262144-instance worst case
   ever shows up on the perf budget (~4 MB/frame at saturation, currently
   non-blocking against the 6 GB VRAM target). Pair with `#973` once that
   lands so both `GpuInstance` shrink steps ride a single shader-struct-sync
   refresh.
2. **REN-D14-NEW-02** — fold into the broader RenderDoc / golden-frame test
   harness, not as a one-off. Today's hash-equality contract is sufficient
   for the material-table layer; the missing coverage is downstream of it.

## Methodology Notes

- All claims verified by reading source (no inference from comments alone).
- `cargo test -p byroredux-renderer --lib` run: 278/278 pass (includes the 18
  `vulkan::material::tests` + the cross-shader `gpu_instance_layout_tests`).
- Offset assertions counted by inspecting the test body — 65 named `offset_of!`
  assertions cover every named field in `GpuMaterial`.
- Shader file enumeration confirmed by
  `grep -l "struct GpuInstance" crates/renderer/shaders/` — exactly 5.
- The two findings are explicitly LOW because both have source documentation
  acknowledging the gap and a clear migration path; neither blocks
  correctness or RT-spec compliance.
