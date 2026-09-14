# #4396 — NIFAL-D7-2026-09-14-01: #4166's "components square to inf → zero quaternion" hole is still open on every non-B-spline rotation path (mainline KF keys, static poses, HKX)

**Labels**: high,nifal,animation,safety,bug
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

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
