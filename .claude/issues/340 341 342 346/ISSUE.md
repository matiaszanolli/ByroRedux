# Batch: #340 #341 #342 #346 — audit findings, all CLOSED

All four were CLOSED audit findings. Verification pass (audit-hygiene: confirm
premise against current code before acting) — **all four already fixed in-code**,
each fix citing its issue number. No source change required.

## #340 — SI-05 (LOW, perf): per-frame StringPool.get() lowercase allocation
- **Premise**: `StringPool::get`/`intern` called `s.to_ascii_lowercase()` every
  frame per channel name → 300K–600K heap allocs/sec.
- **State**: FIXED (two ways).
  - `crates/core/src/string/mod.rs:82` `get()` and `:53` `intern()` lowercase
    into a 256-byte stack buffer via `ascii_lowercase_into_buf` (mod.rs:167);
    heap fallback only for names > 256 bytes (never, for bone/channel names).
  - `crates/core/src/animation/types.rs:207` `AnimationClip.channels` is now
    `HashMap<FixedString, TransformChannel>` — keys pre-interned at clip load,
    so the hot path does integer symbol comparison, zero allocation. Comment
    "See #340." This is exactly the suggested fix.

## #341 — S1-01 (HIGH): BSTriShape FO76 Bound Min Max (24 bytes) never consumed
- **Premise**: `#BS_F76#` (bsver==155) `Bound Min Max` (6×f32) between radius and
  skin_ref was skipped, misaligning skin/shader/alpha/vertex_desc.
- **State**: FIXED. `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:263`
  `if stream.bsver() == FO76 { stream.skip(24)?; }` before `skin_ref`.

## #342 — S5-01 (HIGH): BSDynamicTriShape drops every Skyrim NPC head/face mesh
- **Premise**: the SSE `particle_data_size` u32 read sat inside `if data_size > 0`,
  so data_size==0 (every BSDynamicTriShape) skipped it → `parse_dynamic` read
  from a misaligned offset → empty geometry → dropped mesh.
- **State**: FIXED. bs_tri_shape.rs:455 reads `particle_data_size`
  unconditionally for `bsver() < FALLOUT4`, out of the `data_size > 0` gate;
  trailing particle arrays still guarded by `particle_data_size > 0`. Comment
  cites #341/#342 rationale.

## #346 — S4-02 (HIGH): BsTriShape import ignores BSEffectShaderProperty
- **Premise**: `extract_bs_tri_shape` only tried `BSLightingShaderProperty`; effect
  shaders lost emissive/uv/alpha/normal/two_sided/decal.
- **State**: FIXED via the suggested refactor. `extract_bs_tri_shape`
  (crates/nif/src/import/mesh/bs_tri_shape.rs:92) routes through the unified
  `material::extract_material_info_from_refs`, which handles both shader types;
  `two_sided`/`is_decal` pulled from the resulting `MaterialInfo` (lines 213-214).
  Regression coverage in `bs_tri_shape_shader_flag_tests.rs` (cites #346).

## Verification
- `cargo test -q -p byroredux-nif -p byroredux-core` — all green.
- No code change; scoped test evidence only.
