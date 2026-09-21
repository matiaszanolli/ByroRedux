# 4396: NIFAL-D7-2026-09-14-01: #4166's "components square to inf → zero quaternion" hole is still open on every non-B-spline rotation path (mainline KF keys, static poses, HKX)

State: OPEN  Labels: ['bug', 'animation', 'high', 'safety', 'nifal']

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: HIGH. The dimension agent proposed MEDIUM on the grounds that vanilla never triggers it: 0 of 16.06M vanilla rotation keys across FNV, Oblivion and Shivering Isles. The orchestrator raised it to HIGH for consistency with #4166, the same arithmetic on a sibling path, filed HIGH. The TBC arm produces a NaN bone transform, which reaches BLAS refit and TLAS build. A grep of `crates/renderer/src/vulkan/acceleration/`, `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`, `byroredux/src/render/skinned.rs`, `byroredux/src/render/static_meshes.rs` and `byroredux/src/systems/animation.rs` finds no `is_finite` guard on transforms. Severity is impact, not likelihood.
- **Dimension**: Animation
- **Tier Violated**: no-fabrication
- **Game Affected**: all NIF titles (KF and embedded `NiTransformData`); Skyrim LE/SE via `convert_hkx_clip`
- **Location**:
  - `crates/nif/src/anim/keys.rs:57-78` (`convert_quat_keys`)
  - `crates/core/src/math/coord.rs:200-207` (`normalize_quat`)
  - `crates/nif/src/anim/transform.rs:145-157` (`constant_transform_channel`)
  - `crates/nif/src/anim/bspline.rs:439-448` (B-spline static pose)
  - `crates/hkx/src/animation.rs:1045-1058` (`normalize_quaternion`)
  - Consumers: `crates/core/src/animation/interpolation.rs:314`, `byroredux/src/systems/animation.rs:721-722`
- **Status**: NEW (sibling of closed #4166; #1443 guarded only non-finite components)
- **Description**: #4166's fix guards `len_sq.is_finite()` only inside `normalized_rotation_sample` on the B-spline path. Everywhere else a rotation key is normalized, a component that is individually sane (e.g. `2e19`) squares to `inf`, the inverse becomes `0`, and the result is the zero quaternion. An authored all-zero key also passes, because `normalize_quat` returns zero-length input unchanged. HKX `normalize_quaternion` returns `Ok([0,0,0,0])` for the same input. Neither canonical boundary re-checks unit length.
- **Evidence**: Measured with glam 0.29.3 in scratch programs:
  - **Const, single-key, or `i0 == i1`**: `Mat4::from_quat(zero)` is identity, so the bone gets a silent identity pose.
  - **Linear slerp toward zero**: length 0.707, which decomposes to a sheared scale (0.939, 1, 0.939).
  - **TBC with a zero start key**: `(q0 * identity).normalize()` at `crates/core/src/animation/interpolation.rs:314` is **NaN**, written straight into `Transform.rotation`.
- **Impact**: Malformed or crafted KF/NIF/HKX content only. On such content the result is a silent identity pose or visible shear, and on TBC channels a NaN bone transform that becomes undefined-behaviour AS input — the #3765/#4166 failure chain.
- **Related**: #4166, #3765, #1443, #3316, NIFAL-D7-2026-09-14-02, NIFAL-D7-2026-09-14-03.
- **Suggested Fix**: Promote `normalized_rotation_sample` into `crates/nif/src/anim/keys.rs` as the single rotation-key sanitizer. It should return `None` for non-finite *or* ≤ EPSILON `len_sq`. Use it at `convert_quat_keys`, `constant_transform_channel` and both B-spline static branches. In hkx, add `!length_squared.is_finite()` to `normalize_quaternion`'s error arm. Add `[2e19,0,0,0]` and `[0,0,0,0]` sanitize tests.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# 4397: NIFAL-D7-2026-09-14-02: static-pose fallbacks gate only on `is_flt_max`, which is false for NaN — a NaN pose reaches the canonical clip despite `crates/nif/src/anim/keys.rs` documenting those paths as guarded

State: OPEN  Labels: ['bug', 'animation', 'high', 'safety', 'nifal']

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


---
