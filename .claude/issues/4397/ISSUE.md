# #4397 — NIFAL-D7-2026-09-14-02: static-pose fallbacks gate only on `is_flt_max`, which is false for NaN — a NaN pose reaches the canonical clip despite `crates/nif/src/anim/keys.rs` documenting those paths as guarded

**Labels**: high,nifal,animation,safety,bug
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: HIGH. The dimension agent proposed MEDIUM because the vanilla scan found 0 non-finite T/R/S keys. The orchestrator raised it to HIGH on the same basis as D7-01: NaN reaches a `Transform` and the AS build with no downstream finiteness guard.
- **Dimension**: Animation
- **Tier Violated**: no-fabrication
- **Game Affected**: all NIF titles (static poses on `NiTransformInterpolator` / `NiLookAtInterpolator` / `NiBSplineComp*Interpolator`)
- **Location**:
  - `crates/nif/src/anim/transform.rs:134`, `:145-148`, `:158`
  - `crates/nif/src/anim/bspline.rs:181`, `:417`, `:441`, `:464`, `:496`, `:509`, `:518`
  - `crates/nif/src/anim/channel.rs:267`
  - Stale guarantee: `crates/nif/src/anim/keys.rs:14-23`
- **Status**: NEW (#1443 fixed the keyframe converters and explicitly relied on these paths being gated)
- **Description**: `is_flt_max(v)` is `v.abs() >= 3.0e38`, which is false for NaN. The eleven static-pose gates use only it, so a NaN in an authored `NiQuatTransform` or `interp.value` passes every one:
  - Translation goes through the pure swizzle and stays NaN.
  - Rotation normalizes to all-NaN.
  - Scale is copied verbatim.

  `read_ni_quat_transform` does no finiteness check. `constant_transform_channel` is the fallback for 39.7% of FNV transform controlled blocks (#3316), so this is the broadest static-pose path.
- **Evidence**: Measured: `is_flt_max(NaN) == false`, and `zup_to_yup_pos([NaN,…])` gives `[NaN,2,-1]`. `convert_nif_clip` copies keys with no sanitization (`byroredux/src/anim_convert.rs:431-463`).
- **Impact**: On corrupt content, a NaN `Transform` on the bone or node propagates to `GlobalTransform`, skinning and the TLAS.
- **Related**: #1443, #3765, #4166, #3316, NIFAL-D7-2026-09-14-01.
- **Suggested Fix**: Replace `is_flt_max(x)` with `!is_key_value_sane(x)` at the eleven gates; it already includes the FLT_MAX sentinel, so the "axis inactive" semantics are kept. Correct the `crates/nif/src/anim/keys.rs:15-16` doc. Add NaN-pose sanitize tests.

### MEDIUM

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
