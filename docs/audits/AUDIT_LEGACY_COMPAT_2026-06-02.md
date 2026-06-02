# Legacy Compatibility Audit — 2026-06-02

**Scope:** Full sweep, dimensions 1–11 per `/audit-legacy-compat`.

**Predecessors:**
- [AUDIT_LEGACY_COMPAT_2026-05-23.md](AUDIT_LEGACY_COMPAT_2026-05-23.md) — last full sweep; surfaced 1 LOW (SceneFlags parity), since closed (#1235).
- [AUDIT_LEGACY_COMPAT_2026-05-19.md](AUDIT_LEGACY_COMPAT_2026-05-19.md) — prior full sweep (D1–D4 cluster, all closed).

---

## Executive Summary

**302 commits** landed in the ten-day window since the 2026-05-23 sweep —
the heaviest concentration in M49 (FO4 precombined-geometry CSG reader),
renderer hardening (RT bias / barrier / DoF), and ECS perf
(change-detection propagation). The bulk does not touch the legacy-compat
surface, so dimensions D1, D3, D4, D6, D7, D8, D10, D11 are **steady-state
clean**. The genuinely new surface is concentrated in three places, and
that is where this sweep's findings cluster:

1. **M49 FO4 precombined CSG path** (D2) — verified end-to-end; closes
   open issue **#1351**.
2. **Animation embedded/sequence dispatch** (D5) — surfaced the only
   MEDIUM finding: inline transform controllers are dropped.
3. **Particle / collision new translation arms** (D9 / D10) — collision
   matrix is complete; particle path has one untyped modifier.

The prior sweep's single LOW (`SceneFlags` dropped at spawn) is verified
closed: [`spawn.rs:275`](../../byroredux/src/cell_loader/spawn.rs#L275) and
[`spawn.rs:841`](../../byroredux/src/cell_loader/spawn.rs#L841) now attach
`SceneFlags::from_nif` on both the placement root and per-mesh insert (#1235).

### Severity rollup

| Severity | Count | Where |
|----------|-------|-------|
| CRITICAL | 0 | — |
| HIGH     | 0 | — |
| MEDIUM   | 1 | D5 (LC-D5-01 embedded transform controller dropped) |
| LOW      | 6 | D2 ×1, D5 ×3, D9 ×2 |
| Steady-state | D1, D3, D4, D6, D7, D8, D10, D11 | zero new findings |

### Issue housekeeping (re-verified this sweep)

| Issue | Dim | Status after re-check |
|---|---|---|
| **#1351** (FO4 PreCombined CSG absent) | D2 | **RESOLVED by M49 — recommend CLOSE** (evidence below) |
| #1358 (BGEM effect scalars not forwarded) | D8 | Confirmed OPEN / unfixed |
| #1359 (FO4 CONT never queried by spawn) | D6/loader | Confirmed OPEN |
| #1393 (BSPSysSimpleColorModifier RGBA unguarded) | D5 | Confirmed OPEN / unfixed |
| #1382 (particle sim spawn rate/start_size unguarded) | D9 | Confirmed OPEN |
| #1332 (NiUnknown ceiling / Oblivion catch-all cascade) | D2 | Confirmed OPEN; LC-D9-01 is a fresh instance of this class |

---

## Dimension Status

### D1 — Scene Graph Decomposition · CLEAN
Every NiAVObject field in `docs/legacy/api-deep-dive.md` has its Redux
component present in `crates/core/src/ecs/components/`:
`transform.rs` (LocalTransform), `global_transform.rs` (WorldTransform),
`hierarchy.rs` (Parent + Children), `world_bound.rs`, `collision.rs`
(CollisionObject), `scene_flags.rs`, `name.rs`. The prior-sweep parity gap
(`SceneFlags` at the cell-loader spawn boundary) is closed (#1235).

### D2 — NIF Format Readiness · CLEAN (M49 verified) + 1 LOW
The M49 FO4 precombined CSG path landed and is sound end-to-end (detail
below). 197 dispatch arms; the only structural ceiling is the
**Oblivion (v20.0.0.5, no block_sizes) catch-all hard-error**
([`blocks/mod.rs:1217-1225`](../../crates/nif/src/blocks/mod.rs#L1217-L1225)),
already tracked as **#1332** — not re-filed.

### D3 — Transform Compatibility · CLEAN
Matrix3 → Quat conversion present (`Quat::from_mat3`,
[`collision.rs:621`](../../crates/nif/src/import/collision.rs#L621); main
import path via `transform.rs` with degenerate-rotation SVD repair).
Local→world propagation: `make_transform_propagation_system`
([`systems.rs:41`](../../crates/core/src/ecs/systems.rs#L41)), now
change-detection incremental.

### D4 / D8 / D11 — Material Translation Boundary · CLEAN
`translate_material` ([`material_translate.rs:71`](../../byroredux/src/material_translate.rs#L71))
remains the **sole** producer of a populated `Material`; both spawn paths
(`cell_loader/spawn.rs:873`, `scene/nif_loader.rs:816`) and the new M49
precombine path delegate to it (precombine applies onto `ImportedMesh`,
then routes through the standard spawn). `metalness`/`roughness` are plain
resolved `f32` filled by `resolve_pbr` — no render-time `classify_pbr`
regression. Starfield CDB content still funnels through the single
boundary. **#1358** (BGEM `base_color`/`soft_depth`/`effect_pbr_specular`
parsed but not forwarded) confirmed open; not re-filed.

### D5 — Animation Readiness · 1 MEDIUM + 3 LOW
Core sampling + KF/KFM import are broad and well-built (Linear/Quadratic-
Hermite/TBC, SLERP/SQUAD, float/color/bool/flip channels, B-spline eval
correctly **not** era-gated). Findings below are dispatch-coverage and
finite-guard gaps.

### D6 — String Interning · CLEAN
`StringPool::intern` lowercases via `to_ascii_lowercase`
([`string`](../../crates/core/src/string/)); case-folding divergence vs
Gamebryo's case-preserving `NiFixedString` is documented and intentional
(#895), with `Arc<str>` carried alongside for case-preserving display.

### D7 — NIFAL Canonical-Translation Contract · CLEAN
Each per-category slice retains exactly one `translate()` boundary
(verified for material D8, particle D9, collision D10). No `Option`
resolve-later leaks and no per-game branches re-derive the canonical form
downstream of a boundary.

### D9 — Particle Emitter Translation Parity · 2 LOW
The four `NiPSys*` typed blocks decode; authored params correctly
**override** the name-heuristic presets at both spawn sites
(`cell_loader/spawn.rs:418-425`, `scene/nif_loader.rs:540-547`); the
#1411/#1364 finite + FLT_MAX guards are intact. Two coverage gaps below.

### D10 — Havok Collision-Shape Coverage Matrix · CLEAN
All 15 dispatched `bhk*Shape` variants (incl. the new #1360
`BhkConvexSweepShape` and #1361 `BhkMeshShape`) have a matching
`resolve_shape` arm; the structural guard test
`every_dispatched_bhk_shape_has_resolve_arm`
([`collision.rs:1267`](../../crates/nif/src/import/collision.rs#L1267))
enforces it and passes. The two documented limitations
(`BhkNPCollisionObject` → `synthesize_static_trimesh` fallback;
`BhkPCollisionObject` phantoms → no `TriggerVolume` path yet) are intact,
not regressed. (`bhkPlaneShape` #1334 is *undispatched*, a separate
already-tracked class, not a parsed-then-dropped leak.)

---

## Findings

### LC-D5-01: Inline transform controllers dropped from the embedded-animation path
- **Severity**: MEDIUM
- **Dimension**: D5 (Animation Readiness)
- **Location**: `crates/nif/src/anim/entry.rs:290-498` (drop arm `:493`)
- **Status**: NEW
- **Description**: `import_embedded_animations` dispatches inline (non-KF)
  controllers attached directly to a `NiNode`. It has arms for
  Alpha / Vis / TextureTransform / MaterialColor / shader float+color /
  Flip / Light color+dimmer+intensity+radius / UV — but **no arm for
  `NiTransformController` / `NiKeyframeController`**. Both are parsed as
  typed blocks ([`blocks/mod.rs:699-700`](../../crates/nif/src/blocks/mod.rs#L699-L700))
  and `extract_transform_channel` exists, but neither is wired into the
  embedded walker, so a transform controller on a node falls through to
  the `other =>` debug-log drop ([`entry.rs:493`](../../crates/nif/src/anim/entry.rs#L493)).
- **Evidence**: The match in `entry.rs:290-498` enumerates every handled
  type; no `"NiTransformController"`/`"NiKeyframeController"` case exists.
  Era-independent — the gap bites whichever string the block reports.
- **Impact**: Ambient transform animation baked inline into a loose `.nif`
  (Oblivion/FO3/FNV animated scenery: fans, doors, lifts, swinging signs
  driven by an inline `NiKeyframeController`, no `NiControllerManager`)
  renders **static**. The static mesh still draws, so content is not lost
  — only its motion. Loose `.kf` clips are unaffected (handled in
  `sequence.rs`).
- **Related**: LC-D5-03 (same root issue on the KF-sequence path).
- **Suggested Fix**: Add a `"NiTransformController" | "NiKeyframeController"`
  arm to the embedded dispatch that resolves the `NiSingleInterpController`
  interpolator and feeds `extract_transform_channel`-equivalent TRS keys
  into `clip.channels`.

### LC-D5-02: NIF `KeyType::Constant` collapsed to `Linear` (step keys wrongly LERPed)
- **Severity**: LOW
- **Dimension**: D5 (Animation Readiness)
- **Location**: `byroredux/src/anim_convert.rs:75-76`
- **Status**: NEW
- **Description**: Core `KeyType` has only `Linear / Quadratic / Tbc`
  ([`types.rs`](../../crates/core/src/animation/types.rs)). The NIF→core
  converter maps both `KeyType::XyzRotation => Linear` (`:75`) and
  `KeyType::Constant => Linear` (`:76`). Gamebryo `KEY_CONST` means
  *hold value until next key* (step), not interpolate. The NIF-side scalar
  sampler honors Constant (`keys.rs:130`) but that path only bakes XYZ-Euler
  axes; the runtime transform sampler never sees a step mode.
- **Evidence**: [`anim_convert.rs:76`](../../byroredux/src/anim_convert.rs#L76)
  `KeyType::Constant => KeyType::Linear`.
- **Impact**: Transform channels authored with stepped/constant
  interpolation animate smoothly instead of snapping — wrong motion for
  hard-cut keyframed scenery / IK poses. Low blast radius (rare in vanilla
  TRS streams), silently incorrect.
- **Suggested Fix**: Add a `KeyType::Const` (step) variant to the core enum
  and have the sampler hold `k0.value` across the segment.

### LC-D5-03: KF-sequence dispatch matches only `"NiTransformController"`, not the aliased `"NiKeyframeController"`
- **Severity**: LOW
- **Dimension**: D5 (Animation Readiness)
- **Location**: `crates/nif/src/anim/sequence.rs:40`
- **Status**: NEW
- **Description**: `import_sequence` dispatches controlled blocks on the
  resolved controller-type *string*. The transform arm matches only
  `"NiTransformController"` (`:40`), while the block **parser** deliberately
  aliases both `"NiTransformController" | "NiKeyframeController"`
  ([`blocks/mod.rs:699-700`](../../crates/nif/src/blocks/mod.rs#L699-L700),
  comment: "NiKeyframeController is the pre-Skyrim per-bone animation
  driver"). The import dispatch should carry the same alias for
  consistency; otherwise a controlled block whose type string resolves to
  the classic name falls to the `_ =>` drop (`sequence.rs:123`).
- **Evidence**: asymmetry between `sequence.rs:40` (single name) and the
  parser's two-name alias.
- **Impact**: **Premise caveat** — this only bites if real target-era KF
  controlled blocks carry the `"NiKeyframeController"` type string rather
  than the `"NiTransformController"` the Bethesda exporters typically write.
  That was **not confirmed against sample FNV/Oblivion KF data** in this
  sweep, so the finding is defense-in-depth / parity, not a confirmed
  content regression. Filed LOW pending a sample-data check.
- **Related**: LC-D5-01.
- **Suggested Fix**: One-line alias —
  `"NiTransformController" | "NiKeyframeController" =>` at `sequence.rs:40`.

### LC-D5-04: Mainline keyframe-stream converters lack the finite/FLT_MAX guard applied elsewhere
- **Severity**: LOW
- **Dimension**: D5 (Animation Readiness)
- **Location**: `crates/nif/src/anim/channel.rs:62-70,151-177,336-344`;
  `crates/nif/src/anim/keys.rs:17-49,185-198`
- **Status**: NEW
- **Description**: The B-spline static-fallback and constant-transform paths
  apply `is_flt_max` filtering (`transform.rs`, `bspline.rs`), but the
  mainline keyframe-stream converters (`convert_vec3_keys`,
  `convert_quat_keys`, `convert_float_keys`, and the float/color/bool
  channel extractors) copy raw NIF floats with no `is_finite` / FLT_MAX
  filter — the same class #772 fixed for the B-spline *pose* path.
- **Impact**: A corrupt key value reaches the sampler and the bone/shader
  uniform (NaN skinning matrix). Lower likelihood than the particle paths
  (vanilla key streams are clean) → LOW, but it is an inconsistency worth a
  single shared sanitizer.
- **Related**: #1393, #1382, LC-D9-02 (same finite-guard family).
- **Suggested Fix**: Route all keyframe-stream value reads through one
  `sanitize_key_value` helper that drops/clamps non-finite + FLT_MAX,
  mirroring the emitter-rate `sane()` gate.

### LC-D9-01: `NiPSysPartSpawnModifier` has no dispatch arm → Oblivion catch-all cascade
- **Severity**: LOW
- **Dimension**: D9 (Particle Emitter Translation Parity)
- **Location**: `crates/nif/src/blocks/mod.rs` (no `"NiPSysPartSpawnModifier"`
  case; catch-all `:1190-1225`)
- **Status**: NEW — instance of the **#1332** class
- **Description**: `NiPSysPartSpawnModifier` (a real `NiPSysModifier`
  subclass per `nif.xml`) is not dispatched and falls to the catch-all.
  On FO3+/Skyrim+ the block_size recovery skips it gracefully; on
  **Oblivion (no block_sizes)** the catch-all hard-errors
  ([`blocks/mod.rs:1217-1225`](../../crates/nif/src/blocks/mod.rs#L1217-L1225))
  and truncates the remaining NIF — the #1332 ceiling, triggered here by a
  specific untyped modifier.
- **Evidence**: zero hits for `NiPSysPartSpawnModifier` across `crates/nif/`.
- **Impact**: Bounded — depends on whether the modifier actually appears in
  Oblivion-era content (unverified; its `nif.xml` version range predates
  20.0.0.5). The structural cascade is the real concern and is already
  tracked under #1332.
- **Related**: #1332.
- **Suggested Fix**: Add a base-only `parse_modifier_only` arm (sibling of
  `NiPSysPositionModifier`) so it skips cleanly on every era.

### LC-D9-02: `extract_emitter_params` omits `planar_angle` / `planar_angle_variation` from its finite sweep
- **Severity**: LOW
- **Dimension**: D9 (Particle Emitter Translation Parity)
- **Location**: `crates/nif/src/import/walk/mod.rs:696-703`
- **Status**: NEW
- **Description**: The #1411 finite guard sweeps every lifted scalar except
  the two planar-angle fields, which are read into `EmitterBaseParams` but
  not in the `all_finite` list.
- **Impact**: Harmless today — `apply_emitter_params`
  ([`systems/particle.rs:29`](../../byroredux/src/systems/particle.rs#L29))
  never reads them — but a latent NaN trap if planar angle is ever wired
  into the spawn cone.
- **Related**: #1411, LC-D5-04.
- **Suggested Fix**: Add both fields to the `is_finite` list at `:696-703`,
  or gate them when the spawn path begins consuming them.

### LC-D2-01: Stale doc-comments claim the FO4 CSG path is deferred (post-M49 doc rot)
- **Severity**: LOW
- **Dimension**: D2 (NIF Format Readiness)
- **Location**: `byroredux/src/cell_loader/load.rs:231-239`;
  `byroredux/src/cell_loader/precombined.rs:11-33`
- **Status**: NEW
- **Description**: Block comments still say the CSG companion reader is
  deferred / "this pass spawns zero entities" — contradicting the now-landed
  M49 path (logic is correct; only the comments are stale).
- **Impact**: Documentation rot; misleads the next reader. No runtime effect.
- **Suggested Fix**: Refresh both comments when #1351 is closed.

---

## #1351 closure evidence (recommend CLOSE)

The M49 commit chain (`b93ad7a9` CSG reader → `3d665217` Y-up decode →
`067adc34` render from CSG → `a30c088a` one-LOD-per-object →
`2900de70` texture-from-owning-shape) closes the render loop:

- **CSG reader** `crates/bsa/src/csg.rs` — full `bcsg` decode (chunk table,
  zlib inflate into 64 KiB PSG space), bounds/overflow/EOF-guarded, unit-tested.
- **Decode** `crates/nif/src/import/precombine.rs` — half-precision vertex
  stream → Y-up positions/normals/tangents; **one** LOD selected by finest
  triangulation (`precombined.rs:333-336`), not all three (a30c088a not
  regressed); textured via the owning `BSTriShape`'s material
  (`precombine.rs:269-308`).
- **End-to-end** `byroredux/src/cell_loader/load.rs:240-266` —
  `spawn_precombined_meshes` spawns through the standard upload path; when
  it spawns geometry the `absorbed_refs` (XPRI/XCRI union) suppress the
  duplicate per-REFR architecture, with a clean per-REFR fallback when it
  spawns nothing.

---

## Recommended next steps

1. **LC-D5-01 (MEDIUM)** — add the transform arm to the embedded-animation
   dispatch (`entry.rs`); restores inline animated scenery in loose NIFs.
2. **Close #1351** — FO4 precombined geometry now renders end-to-end; also
   clears LC-D2-01 doc rot.
3. Batch the finite-guard family (LC-D5-04, LC-D9-02, #1393, #1382) behind
   one shared `sanitize` helper.

---

*Generated by `/audit-legacy-compat`. To file findings as issues:*
```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md
```
