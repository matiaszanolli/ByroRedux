# Legacy Compatibility Audit — 2026-06-14

**Scope:** Full sweep, dimensions 1–7 per `/audit-legacy-compat` (D1 coordinate,
D2 NIFAL contract, D3 material boundary, D4 PHYSAL, D5 EXAL, D6 per-game survey,
D7 subsystem coverage). Part of a `comprehensive` audit-suite run.

**Predecessor:** [AUDIT_LEGACY_COMPAT_2026-06-02.md](AUDIT_LEGACY_COMPAT_2026-06-02.md)
— last full sweep; surfaced 1 MEDIUM (LC-D5-01 embedded transform controller
dropped) + 6 LOW. **LC-D5-01 and LC-D5-03 are verified fixed** (`crates/nif/src/anim/entry.rs:329`
now carries the `"NiSingleInterpController" | "NiTransformController" |
"NiKeyframeController"` arm; `crates/nif/src/anim/sequence.rs:46` carries the
`"NiTransformController" | "NiKeyframeController"` alias).

---

## Executive Summary

The freshest legacy-compat surface since the 2026-06-02 sweep is **PHYSAL**
(the new Physics Abstraction Layer for ragdolls — `0a0bc3ce`, `90caf7bc`,
landed 2026-06-14, plus the M41.x FNV slice that preceded it). The constraint
source-boundary decode (Oblivion / FO3 / FNV / Skyrim, bare + malleable-wrapped)
is **byte-exact and game-agnostic** — verified against `nif.xml` and the 8
passing tests in `crates/nif/src/blocks/collision/bhk_constraint_tests.rs`.
`extract_ragdoll` switches only on `BhkConstraintData`, never on game; the
material/EXAL boundaries remain single-producer and clean.

The one genuine correctness gap this sweep found is in the **PHYSAL sink-side
writeback**: it copies the simulated *body* world pose onto the *bone*
`GlobalTransform` without inverting the body-local offset that the activation
seed applied — so for the normal `bhkRigidBodyT` ragdoll bodies (non-zero
offset from their host bone) the skinned mesh is systematically displaced. The
unit test masks it by using a zero offset.

The other finding is a **coordinate-flip duplication cluster** — ~10 inline
`(x, z, -y)` swaps and a parallel Havok coord path that bypass the single
source of truth in `crates/core/src/math/coord.rs`. All are *correct-valued*
(verified bit-identical), so there is no current mis-placement; the risk is a
future divergent edit on the live REFR placement path. Issue #1318 consolidated
4 such sites; these are the ones it left behind.

### Severity rollup

| Severity | Count | Where |
|----------|-------|-------|
| CRITICAL | 0 | — |
| HIGH     | 0 | — |
| MEDIUM   | 1 | D4 (LC-D4-01 ragdoll writeback omits body-local offset inverse) |
| LOW      | 2 | D1 (LC-D1-01 coord-flip duplication cluster), D7 (LC-D7-01 stale `#869` property docstrings) |
| Steady-state | D2, D3, D5, D6 | zero new findings |

### Issue housekeeping (re-verified this sweep)

| Issue | Dim | Status after re-check |
|---|---|---|
| #1330 (BSShaderNoLightingProperty over-reads 16B on FO3/FNV bsver≤26) | D6 | **Code-fixed** (`crates/nif/src/blocks/shader.rs:172` now gates on `bsver() > FLAGS_U32_THRESHOLD`; regression tests present at `shader_tests.rs`). Issue still OPEN — **recommend CLOSE**. Not re-filed. |
| #1359 (FO4 CONT never queried by cell loader) | D6 | Confirmed OPEN / unfixed. Not re-filed. |
| #1334 (Skyrim SE bhkPlaneShape undispatched) | D7 | Confirmed OPEN; single-file, block_size-recovered. Not re-filed. |
| #1317 / #1324 (dead code in debug-ui/sfmaterial/scripting) | D7 | Confirmed OPEN. Out of dimension scope. |
| Oblivion catch-all hard-error ceiling (`blocks/mod.rs:1223`) | D2 | Intact; documented structural limitation. Per-block instances filed+closed individually (#1444 = prior LC-D9-01, now CLOSED). Not a fresh finding. |

---

## Dimension Status

### D1 — Coordinate-System Correctness · 1 LOW
The single source of truth (`crates/core/src/math/coord.rs`,
`crates/nif/src/import/coord.rs`, `byroredux/src/cell_loader/euler.rs`) is
correct. Strip de-stitch winding (`crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:578`
swaps the **last two** verts on odd tris — CCW) and the camera Y-flip
(`crates/core/src/ecs/components/camera.rs:48` `proj.col_mut(1).y *= -1.0`) are
both verified correct. No new `4096.0` exterior-cell literal regression (the
`crates/plugin/src/esm/cell/mod.rs` parse-layer const is decoupled and
test-tied; `RENDER_ORIGIN_SNAP` is a single-source const with an equality
test). The one finding (LC-D1-01) is a duplication-of-the-SoT cluster — all
correct-valued.

### D2 — NIFAL Canonical-Translation Contract · CLEAN
`extract_ragdoll` (`crates/nif/src/import/collision.rs:268`) and the constraint
decode carry **no** `game ==` branch; the only per-game seam is the constraint
CInfo byte-layout (`parse_oblivion` / `parse_fo3`), exactly as `physal.md` §3
prescribes. The Oblivion no-block_sizes catch-all hard-error
(`blocks/mod.rs:1223`) is the known structural ceiling, intact.

### D3 — Material Translation Boundary · CLEAN
`byroredux/src/material_translate.rs:73` (`translate_material`) remains the
**sole** populated-`Material` producer; both spawn paths delegate
(`cell_loader/spawn.rs:857`, `scene/nif_loader.rs:796`). `metalness`/`roughness`
are plain resolved `f32` filled by `resolve_pbr`; the deleted per-draw
`classify_pbr` survives only in comments/tests. Glass classified once at the
boundary. The only other `Material { … }` literals are `cornell.rs` (the
`--cornell` reference harness) and a `#[cfg(test)]` helper. The
`material-abstraction.md` slice is CONVERGED (2026-05-28 banner) and superseded
by `nifal.md`.

### D4 — PHYSAL Per-Game Havok Articulation · 1 MEDIUM
Source boundary (constraint decode) is byte-exact and game-agnostic across
Oblivion / FO3 / FNV / Skyrim (8 passing byte-advance tests). `extract_ragdoll`
+ `template_from_imported` + `activate_ragdoll` are single-producer; storages
registered (`main.rs:577-579`); writeback wired Stage::Late (`main.rs:864`).
The one finding (LC-D4-01) is in the sink-side writeback: the body-local offset
is applied forward at seed but not inverted at writeback.

### D5 — EXAL Per-Game Exterior Environment · CLEAN
`byroredux/src/env_translate.rs` is the sole construction site for
`SkyParamsRes` / `WeatherDataRes` / `WaterMaterial` (every other hit is
`#[cfg(test)]`). Bulk `--grid` loader and streaming bootstrap both delegate.
No downstream per-game exterior branch except the sanctioned
`default_water_for_worldspace` GameKind decision and the per-game `.bto` LOD
provider selection (`object_lod.rs:103`, mandated by `exal.md` §5.2). No
reintroduced inline Mojave block; no latitude-parse premise error (sun uses
`tod_hours` + `SUN_SOUTH_TILT`).

### D6 — Per-Game Translation-Survey Gaps · CLEAN
Every production `bsver()` comparison in the parser uses a **named** constant
from `crate::version::bsver::*` — zero bare magic-int branches survive (the
`survey §4` site tables are superseded by the §9 migration). ESM per-game
record dispatch is gated at the single dispatcher via `GameKind`, not in record
bodies. #1330 is code-fixed (see housekeeping). No silent wrong-default
fallbacks on the Fallout-stress paths (half-float verts, CRC32 flags, inline
tangents, BGSM dispatch all discriminate on a named const or the wire format).

### D7 — Subsystem Coverage vs Legacy · 1 LOW
Non-uniform scale: **premise falsified** — Gamebryo `NiAVObject` stores a single
scalar `m_fScale`; Redux is format-faithful, and matrix-baked non-uniform scale
was *measured* to be empty across 10,837 FNV architecture blocks (documented in
`docs/engine/nif-engine-translation-layer.md`). Property→pipeline mapping: all
12 `NiProperty` types map to a sink — NiAlphaProperty (blend + alpha-test
threshold + func, all 8 funcs in `triangle.frag`), NiStencilProperty draw-mode
(two-sided → dynamic `cmd_set_cull_mode`), NiZBuffer, NiWireframe (LINE pipeline
variant), NiShade (flat-shading instance flag) all honored; NiFogProperty is the
documented skip; the stencil-buffer *test* proper is captured-but-dormant
(documented, #337). The one finding (LC-D7-01) is stale doc-rot.

---

## Findings

### LC-D4-01: Ragdoll writeback omits the inverse body-local offset → skinned mesh displaced
- **Severity**: MEDIUM
- **Dimension**: D4 (PHYSAL — sink boundary)
- **Location**: `byroredux/src/ragdoll.rs:189-191` (seed) vs `byroredux/src/ragdoll.rs:255-264` (writeback)
- **Status**: NEW
- **Description**: `activate_ragdoll` seeds each Rapier body at the bone world
  pose **composed with the body-local offset**:
  `translation = gt.translation + gt.rotation * (b.local_translation * gt.scale)`,
  `rotation = gt.rotation * b.local_rotation` (`:190-191`). The per-frame
  `ragdoll_writeback_system` then copies the body's simulated **world** pose
  *directly* back onto the bone's `GlobalTransform` —
  `gt.translation = t; gt.rotation = r` (`:261-262`) — with **no inverse** of
  `local_translation` / `local_rotation`. The seed and the writeback are
  asymmetric: the offset is added going in but never removed coming out.
- **Evidence**: Seed at `ragdoll.rs:190-191` applies `body_local`; writeback at
  `ragdoll.rs:261-262` writes `body_pose` verbatim. `local_translation` /
  `local_rotation` come from `ImportedRagdollBody.translation` /
  `.rotation` (`crates/nif/src/import/collision.rs:299-304`), which is the
  `BhkRigidBody.translation` / `.rotation`. Ragdoll bodies are commonly authored
  as **`bhkRigidBodyT`** — the active-transform variant whose translation/rotation
  are non-zero (`crates/nif/src/blocks/collision/rigid_body.rs:14` "bhkRigidBodyT
  has active translation/rotation"). The unit test
  `activate_then_writeback_moves_bones` uses `local_translation: Vec3::ZERO` /
  `local_rotation: Quat::IDENTITY` (`ragdoll.rs:320-321`), so it never exercises a
  non-zero offset — the bug is invisible to `cargo test`.
- **Impact**: Every real ragdoll whose bodies carry a non-zero bone offset
  (the normal `bhkRigidBodyT` case) writes the **body** origin onto the **bone**
  transform. The skinned mesh — which reads bone `GlobalTransform` — is
  systematically displaced by the body offset for the lifetime of the ragdoll:
  limbs render offset from where the simulated bodies actually are, producing a
  visibly wrong crumple. Bounded blast radius: the path is gated behind the
  `ragdoll <id>` debug-server command (slice 1), not a default content path, and
  no crash. This is **not** the documented "Havok cone+2-plane → Rapier per-axis
  limit" approximation (`physal.md` §3) — that concerns limit fidelity, not body
  placement; this offset bug is undocumented.
- **Related**: PHYSAL §3 "Build + step + writeback"; `physal.md` does not note
  this asymmetry.
- **Suggested Fix**: In `ragdoll_writeback_system`, recover the bone pose from the
  body pose by inverting the seed composition — store the body-local offset on the
  `Ragdoll`/`RagdollBodySpec` (it is currently dropped after seeding) and apply
  `bone_rotation = body_rotation * local_rotation⁻¹`,
  `bone_translation = body_translation - bone_rotation * (local_translation * scale)`.
  Then extend `activate_then_writeback_moves_bones` with a **non-zero**
  `local_translation`/`local_rotation` so the regression is pinned.

### LC-D1-01: Z-up→Y-up coordinate-flip duplicated at ~10 sites that bypass the single source of truth
- **Severity**: LOW
- **Dimension**: D1 (Coordinate-System Correctness)
- **Location**: `byroredux/src/cell_loader/references.rs:235-238`;
  `byroredux/src/cell_loader/refr.rs:496`;
  `byroredux/src/cell_loader/transition.rs:135-136`;
  `byroredux/src/systems/particle.rs:90-91`;
  `crates/nif/src/import/mod.rs:208`;
  `crates/nif/src/import/mesh/tangent.rs` + `mesh/skin.rs:475-477`;
  `crates/nif/src/import/collision.rs:796-833` (`havok_to_engine` /
  `havok_quat_to_engine` / `decompose_havok_matrix`)
- **Status**: NEW (incomplete consolidation of CLOSED #1318)
- **Description**: The skill names `crates/core/src/math/coord.rs`
  (`zup_to_yup_pos`, `zup_to_yup_quat_wxyz`) + its NIF-typed wrappers in
  `crates/nif/src/import/coord.rs` as the **single source of truth** for the
  `(x, z, -y)` axis swap, and flags any duplicated swap that bypasses them as a
  regression. #1318 ("Z-up coord-flip leaked 4 sites", CLOSED) consolidated four;
  the sites above were left behind. The two highest-blast-radius are on the
  **live REFR placement path**: `references.rs:235` (every spawned object's
  position — built inline as `Vec3::new(pos[0], pos[2], -pos[1])` while the
  adjacent rotation *does* route through `euler_zup_to_quat_yup_refr`) and
  `refr.rs:496` (SCOL child placement, same asymmetry). The Havok helpers in
  `collision.rs` are a self-consistent **parallel** SoT; `decompose_havok_matrix`
  uses `Quat::from_mat3` directly, skipping the `#333` explicit-normalize guard
  that `zup_matrix_to_yup_quat` carries.
- **Evidence**: `references.rs:235-238` `Vec3::new(placed_ref.position[0],
  placed_ref.position[2], -placed_ref.position[1])`; `refr.rs:496`
  `Vec3::new(p.pos[0], p.pos[2], -p.pos[1])`; `collision.rs:796` `fn
  havok_to_engine(x,y,z) -> Vec3 { Vec3::new(x, z, -y) }`. All verified
  bit-identical in value to the SoT helpers (grep agent disproved every
  candidate for a wrong sign/order — there is **no current mis-placement**).
- **Impact**: No runtime defect today — values are correct. The risk is
  maintainability/regression: a future fix to the canonical swap (e.g. another
  `#333`-class normalize guard, or a precision change) will not propagate to
  these copies, and a hand edit to the REFR-placement copy could silently skew
  all object placement. Filed LOW because there is no current incorrect behavior;
  blast radius is "future divergent edit on a hot path."
- **Related**: CLOSED #1318 (the partial consolidation); the pre-#1044 five-site
  duplication the skill cites as the canonical example.
- **Suggested Fix**: Route `references.rs` / `refr.rs` / `transition.rs` /
  `particle.rs` / `import/mod.rs` position swaps through `coord::zup_to_yup_pos`,
  and fold the three Havok helpers into the `coord.rs`/`import/coord.rs` family
  (routing `decompose_havok_matrix`'s rotation through `zup_matrix_to_yup_quat`
  so it picks up the normalize guard). Keep the magnitude-only `half_extents`
  variant (`import/mod.rs:209`) as-is — it is a deliberate non-swap.

### LC-D7-01: Stale `#869` "deferred" docstrings on NiWireframeProperty / NiShadeProperty (both fully consumed)
- **Severity**: LOW
- **Dimension**: D7 (Subsystem Coverage / doc rot)
- **Location**: `crates/nif/src/import/material/walker.rs:976-991`
- **Status**: NEW
- **Description**: The walker docstrings for `NiWireframeProperty` and
  `NiShadeProperty` say renderer consumption is "deferred — tracked at #869." Both
  are now fully wired: NiWireframeProperty → `PipelineKey::Opaque{wireframe}` LINE
  pipeline variant (`crates/renderer/.../pipeline.rs`, selected at draw time);
  NiShadeProperty → `INSTANCE_FLAG_FLAT_SHADING` (consumed in the draw path and
  shader). The comments contradict the live code.
- **Evidence**: docstrings at `walker.rs:976-991` vs the live LINE-variant +
  flat-shading-flag consumption traced through the renderer.
- **Impact**: Documentation rot only — misleads the next reader into thinking
  these properties are dropped. No runtime effect.
- **Related**: #869 (referenced by the stale comment).
- **Suggested Fix**: Refresh both docstrings to state both properties are
  consumed (LINE pipeline variant + flat-shading instance flag), and drop the
  "#869 deferred" reference.

---

## What was verified clean (do-not-re-file)

- **PHYSAL source boundary** — constraint CInfo decode byte-exact vs `nif.xml`
  for Oblivion (6/7 Vec4 pivots-first, no motors/no Perp B1) and FO3+ (8 Vec4 +
  motors), bare + malleable-wrapped; 8/8 tests pass. `extract_ragdoll` and
  `joint_from_imported` are game-agnostic single-producers.
- **Material boundary** — `translate_material` sole producer; `resolve_pbr`
  clamp-only; glass once at boundary; no render-time `classify_pbr`.
- **EXAL** — `env_translate.rs` sole producer of all three exterior canonical
  resources; bootstrap + bulk loader delegate; no downstream per-game branch.
- **Coordinate values** — every inline swap is the correct `(x, z, -y)` (or the
  matching matrix/quat); winding + camera Y-flip correct; no CRITICAL.
- **Per-game survey** — named BSVER constants everywhere; #1330 code-fixed.
- **Property→pipeline + transform fidelity** — all 12 properties map to a sink;
  non-uniform scale premise is false (measured-empty + documented); NiFogProperty
  documented skip; stencil-test dormancy documented (#337).
- **Emissive scale** (`EmissiveSource` 3 variants ~1.0) — measured, no
  normalization is correct (`nifal.md` §4). NIFAL Option-sentinels (DALC,
  default_water_height) — real game distinctions, not leaks.

---

## Recommended next steps

1. **LC-D4-01 (MEDIUM)** — fix the ragdoll writeback offset asymmetry; pin it
   with a non-zero-offset regression test. Highest-value fix (it's the headline
   feature of the new PHYSAL slice and currently renders real ragdolls displaced).
2. **Close #1330** — code is fixed; the issue is stale OPEN.
3. **LC-D1-01 (LOW)** — finish the #1318 coord-flip consolidation, prioritising
   the two REFR-placement sites; fold the Havok coord helpers into the SoT so they
   inherit the `#333` normalize guard.
4. **LC-D7-01 (LOW)** — refresh the stale `#869` property docstrings.

---

*Generated by `/audit-legacy-compat`. To file findings as issues:*
```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-06-14.md
```
