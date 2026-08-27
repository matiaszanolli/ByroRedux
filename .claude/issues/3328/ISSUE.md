# FNV-2026-08-26-D6-02

**Issue**: #3328
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 6 — Animation, Skinning & Particles
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/nif/src/anim/channel.rs:56` (float), `channel.rs:170` (Point3-as-colour)

**Premise verified**: `extract_float_channel_at` does
`let data_idx = interp.data_ref.index()?;` and never reads `interp.value`.
`resolve_color_keys_at`'s `NiColorInterpolator` / `NiPoint3Interpolator` arms
`return Vec::new()` when `data_ref` is null. This is internally inconsistent
two ways: `extract_bool_channel_at` (`channel.rs:368`) *does* fall back to
`interp.value`, and `extract_emitter_rate` (`import/walk/mod.rs:897`) reads
`sane(interp.value)` off the very same `NiFloatInterpolator` type as its
documented constant-value branch. nif.xml gives `NiFloatInterpolator.Value`
the same `#INV_FLOAT#` "pose value if lacking data" semantics as the Point3
variant it documents explicitly (line 3256).

**Evidence**:
```
$ ... --example _tmp_fnv_d6_nullpose_all -- "Fallout - Meshes.bsa"
NiFloatInterpolator  null_data=2479 with_real_pose=2479
NiColorInterpolator  null_data=0    with_real_pose=0
NiPoint3Interpolator null_data=32   with_real_pose=32
```
Every single null-data float/point3 interpolator in vanilla FNV carries a
real (non-sentinel) authored value; none are the FLT_MAX "nothing here" form.

**Impact**: alpha (`NiAlphaController`), UV (`NiTextureTransformController` /
`NiUVController`), shader-float and morph-weight channels whose interpolator is
posed rather than keyed contribute nothing, so the target property holds its
spawn-time value instead of the authored constant. Lower severity than D6-01
because a *constant* channel and "no channel" often coincide visually, and
because a subset of the 2 479 are particle-rate interpolators already served by
`extract_emitter_rate`'s own fallback.

**Fix sketch**: mirror `extract_bool_channel_at` — on a null `data_ref` (or a
`data_ref` that yields zero sane keys), emit a single key
`{time: 0.0, value: interp.value}` gated on `is_key_value_sane` /
`FLT_MAX_SENTINEL`, for the float, colour and Point3 arms.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
