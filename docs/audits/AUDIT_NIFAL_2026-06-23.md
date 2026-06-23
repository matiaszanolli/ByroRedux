# NIFAL Audit — Canonical Translation Layer — 2026-06-23

Deep audit of **NIFAL** (the NIF Abstraction Layer; spec: `docs/engine/nifal.md`).
Scope: the *translate* side — does each parsed NIF data category reach one
canonical, game-agnostic representation through a single explicit `translate()`
boundary, with no leak / fabrication / render-time fallback? (Parse-side
correctness belongs to `/audit-nif`.)

All 9 dimensions were run inline against the live tree. Dedup baseline:
`/tmp/audit/issues.json` (28 OPEN) + prior `docs/audits/AUDIT_NIFAL_2026-06-13.md`
and `AUDIT_NIFAL_2026-06-14.md` (each closed with 1 LOW doc-rot finding).

---

## Executive Summary

**Severity tally: 0 CRITICAL · 0 HIGH · 0 MEDIUM · 0 LOW (NEW). 0 regressions.**

Every per-category convergence claim in `nifal.md` §2 was re-verified against the
current code and holds. The four tier invariants — **single-boundary**,
**no-fabrication**, **no-leak**, **no-render-time-fallback** — pass on every
category. No new findings.

Convergence status vs spec §2 leak inventory:

| Category | Spec status | Verified 2026-06-23 |
|---|---|---|
| Materials | converged | ✅ confirmed (single boundary, plain-`f32` resolved PBR, glass-once) |
| Geometry / transform | converged | ✅ confirmed (SVD-once at parse, Z-up→Y-up at import) |
| Skinning | converged | ✅ confirmed (global bone indices, u16 guard intact) |
| Lights | converged | ✅ confirmed (no renderer match on source block type) |
| Nodes | triaged | ✅ confirmed (parked fields have zero canonical consumers) |
| Particles | converged | ✅ confirmed (single overlay boundary, rate sentinel rejected) |
| Collision | audited | ✅ confirmed (16/16 parsed shapes resolve, motion-type collapse correct) |
| Animation / controllers | converged | ✅ confirmed (one `convert_nif_clip` boundary, multi-caller) |
| Shader flags / effects | converged | ✅ confirmed (zero `if game ==` in shaders incl. `#include`s) |

The two prior LOW doc-rot findings (`resolve_classifier_overrides` /
`material-abstraction.md`) are no longer reachable as NIFAL leaks and were not
re-surfaced. The 2026-06-13 D7-NEW-01 (miscalibrated completeness floors) is
addressed: the harness now uses conservative, documented thresholds
(`>= 60.0` texture-path, `>= 40.0` tangents in
`crates/nif/tests/translation_completeness.rs`).

---

## Per-Category Tier Matrix

Boundary fn cited per category; `single-boundary` = N-A where the spec documents
the absence of one boundary as deliberate (Nodes).

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | PASS | PASS | PASS | PASS | `material_translate::translate_material` |
| Geometry / Transform | PASS | PASS | PASS | PASS | `import/coord.rs` + `rotation::sanitize_rotation` (parse-time) |
| Skinning | PASS | PASS | PASS | N-A | `import/mesh/skin.rs` (extraction-time remap) |
| Lights | PASS | PASS | PASS | PASS | `import/walk/mod.rs` → `LightKind` |
| Nodes | N-A (by design) | PASS | PASS | PASS | none — two structural load paths (spec §2 Nodes) |
| Particles | PASS | PASS | PASS | N-A | `systems::particle::apply_emitter_overlays` |
| Collision | PASS | PASS | PASS | N-A | `import/collision.rs::resolve_shape` |
| Animation | PASS | PASS | PASS | PASS | `anim_convert::convert_nif_clip` |
| Shader flags / Effects | PASS | PASS | PASS | PASS | `shader_flags.rs` (block-type dispatch) + `MaterialInfo` |

---

## Findings

**None.** No single-boundary, no-fabrication, no-leak, or no-render-time-fallback
violation was found that could not be disproven against the current code.

### Verification notes (what was checked and why it passed)

**Dim 1 — Material (HIGH blast radius).**
- `translate_material` (`byroredux/src/material_translate.rs:73`) is the only site
  building a `Material` from an `ImportedMesh`. Confirmed callers: exactly two
  production sites — `scene/nif_loader.rs:796` and `cell_loader/spawn.rs:880` —
  both passing identical `ResolvedPaths`; the cell path's only extra is
  `extra_material_flags` (REFR-overlay model-space-normals), the documented
  divergence. The other `Material {` literals are the `--cornell` self-contained
  test scene (`cornell.rs`) and unit-test helpers (`helpers.rs:102`), not import
  paths.
- `Material.metalness` / `roughness` are plain `f32`; `resolve_pbr`
  (`crates/core/src/ecs/components/material.rs:638`) clamps `metalness ∈ [0,1]`,
  `roughness ∈ [0.04,1]` and only fills `NaN` sentinels via `classify_pbr_keyword`
  (backstop). No `*_override: Option<f32>` on the canonical type.
- Glass classified once after `resolve_pbr` via
  `helpers::classify_glass_into_material` (gated `material_kind >= 100` skip,
  `metalness >= 0.3` skip, `!has_alpha || is_decal` skip). Renderer
  (`render/static_meshes.rs:299`) reads `m.metalness`/`m.roughness`/
  `m.material_kind` directly; the render-side glass heuristic is confirmed deleted
  (#1280 sub-step 3c note at `static_meshes.rs:361`).

**Dim 2 — Geometry / Transform.** `sanitize_rotation` fires only at parse time
(`stream.rs:674`, `:697`); no consumer re-validates rotations. Per-game vertex
decoders (`ni_tri_shape.rs`, `bs_tri_shape.rs`, `bs_geometry.rs`) all call
`zup_matrix_to_yup_quat` at the import boundary.

**Dim 3 — Skinning / Lights.** `skin.rs` carries the partition-local→global remap
(#613) and the `global_skin_transform`; the u16-range guard is intact. No renderer
`match` on `NiAmbientLight`/`NiDirectionalLight`/`NiPointLight`/`NiSpotLight`
(grep clean). `LightKind` is produced at `import/walk/mod.rs` and never inspected
downstream.

**Dim 4 — Nodes.** All seven raw-tier-parked fields (`bs_value_node`,
`bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`,
`bs_sub_index`) have **zero canonical ECS consumers** — the only non-test hits are
producer-side field initializers in the mesh extractors / `cell_loader.rs` /
`asset_provider.rs`, never reads. `parked-not-leak` confirmed.

**Dim 5 — Particles.** `apply_emitter_overlays` (`systems/particle.rs:60`) is the
single overlay site; both spawn sites (`nif_loader.rs:526`, `spawn.rs:436`) pass
`color_curve` / `force_fields` as arguments — no inline overlay (the #1513 dedup
holds). `initial_color` is not applied. `extract_emitter_rate`'s `sane()` rejects
non-finite and `>= 3.0e38` FLT_MAX sentinels (#1363/#1364). Force fields are
Z-up→Y-up converted at overlay time. `read_emitter_base` reads the base (not
skip), gating `radius_variation` on `V10_4_0_1` per nif.xml.

**Dim 6 — Collision (HIGH blast radius).** Diffed the parsed-shape set against the
resolve arms: **16 parsed `Bhk*Shape` structs, 16 resolve arms.** Fifteen produce
a `CollisionShape`; `BhkPlaneShape` returns an explicit, documented `None`
(#1334 — no half-space variant; falls to the synthesized-trimesh fallback) — a
documented limitation, not a silent drop. `havok_motion_type`
(`import/collision.rs:145`) maps the full canonical `hkMotionType` enum
(`1..=5|8 → Dynamic`, `6 → Keyframed`, `7 → Static`, `9 → CharacterKinematic`,
else `Static`), verified against `reference/nifxml/nif.xml`'s `hkMotionType` enum
(#1652 regression pin holds; the old `4 => Keyframed / _ => Static` collapse is
gone). The canonical `RigidBodyData.motion_type` is what reaches consumers; the
raw byte does not leak (`nif_loader.rs:427` logs the canonical enum, inserts the
canonical body).

**Dim 7 — Animation.** `convert_nif_clip` is the single NIF→`AnimationClip`
boundary; its 7 callers are all consumers of the one boundary (correct, not a
violation). Per-game variation (B-spline, XYZ-Euler, TBC/Hermite, Z-up→Y-up) is
resolved at import.

**Dim 8 — Shader flags / Effects.** Zero `if game ==` runtime branches in
`triangle.frag` or any `#include`d `include/*.glsl` (all game-name grep hits are
explanatory comments / data-driven `dalcFlags` checks). FO4 `Model_Space_Normals`
(F4SF1 bit 12) + `Alpha_Test` (F4SF2 bit 25) + the FO76 `MODELSPACENORMALS` CRC
reach `MaterialInfo` (`import/material/walker.rs:310-338`, #1592). Compile-time
bit-26/27 equivalence asserts present in `shader_flags.rs`. All 9
`BSLightingShaderProperty` shader-type variants forward their trailing data.

**Dim 9 — Completeness + cross-cutting.** Each category that needs a boundary
declares one (Material / Particles / Animation / Collision / EXAL); Nodes is the
documented N-A. The completeness harness
(`crates/nif/tests/translation_completeness.rs`) is `#[ignore]`-gated with
structural-consistency hard asserts + conservative per-game fill-rate floors.

---

## Documented-limitation ledger (do NOT re-report next sweep)

These are parked-not-leak by design — each blocked on a consumer feature that does
not exist, so translating now would invent an ECS component nothing reads
(`no-fabrication`). Spec: `nifal.md` §2.

- **Node/mesh passthroughs**: `bs_value_node`, `bs_ordered_node`, `tree_bones`,
  `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index` — zero canonical
  consumers (re-verified).
- **`NiTextureEffect`** — extractor dead because content-absent (0 occurrences,
  measured 2026-06-02). Do not build a projector pass speculatively.
- **`BhkNPCollisionObject`** (FO4/FO76/Starfield `BhkSystemBinary` blob) — decoder
  is a separate project; consumer falls back to `spawn.rs::synthesize_static_trimesh`.
- **`BhkPCollisionObject`** phantoms (Skyrim+ triggers) — need a `TriggerVolume`
  ECS path, not a rigid body; the `is::<…>` discriminators distinguish the two.
- **`BhkPlaneShape`** (#1334) — no half-space `CollisionShape` variant; explicit
  `None` → trimesh fallback.
- **Particle size-over-life curve** — only authored *magnitude* translated
  (`start_size = end_size = initial_radius × base_scale`); the grow→fade bell shape
  needs a richer canonical size model.
- **Emissive normalization** — resolved no-op (`nifal.md` §4); all three
  `EmissiveSource` variants measured at ~1.0 scale. Re-adding a normalization
  constant would be a `no-fabrication` violation in reverse.
- **Per-light ambient colour channels + morph-weight animation channels** —
  captured, no renderer consumer yet.
- **`NiLight.kind`** (LightKind discriminator) — NIF per-mesh lights spawn as a
  point-ish `LightSource` (`spawn.rs:354`); directionality is not consumed.
  Consistent with the converged "renderer never inspects source block type"
  contract — parked, not a leak.

## Pre-existing OPEN issues touching this layer (Existing — not re-reported)

- **#1333** — modern `NiParticleSystem` local transform discarded → emitter
  ignores host-relative offset. Import-side; tracked.
- **#1659** (SKY-D3-03) — `BSDismemberSkinInstance` per-partition body-part flags
  parsed but discarded at import. Maps to the parked `bs_sub_index` /
  dismemberment-system gap.
- **#1627** (TD5-002) — `GpuMaterial::glass()` transmission TODO names a closed
  issue; preset unused. Renderer-side tech-debt, not a NIFAL translate leak.
