# NIFAL Audit — Canonical Translation Layer — 2026-07-03

Deep audit of **NIFAL** (the NIF Abstraction Layer; spec: `docs/engine/nifal.md`).
Scope: the *translate* side — does each parsed NIF data category reach one
canonical, game-agnostic representation through a single explicit `translate()`
boundary, with no leak / fabrication / render-time fallback? (Parse-side
correctness belongs to `/audit-nif`.)

All 9 dimensions were verified against the live tree @ HEAD `8498e559`.
Dedup baseline: `/tmp/audit/issues.json` (71 OPEN) + prior
`docs/audits/AUDIT_NIFAL_2026-07-02.md` (HEAD `1b4e8e84`, 0 findings), plus
`AUDIT_NIFAL_2026-06-28.md`, `AUDIT_NIFAL_2026-06-23.md`,
`AUDIT_NIFAL_2026-06-14.md`, `AUDIT_NIFAL_2026-06-13.md`, `AUDIT_NIFAL_2026-05-30.md`.

Delta since the 2026-07-02 sweep: 31 commits (`1b4e8e84..8498e559`), 30 of them
same-day (2026-07-02/03) fix commits plus the skill-refresh commit at HEAD.
Every commit was triaged for NIFAL relevance; two touch the translate surface
directly (both are fixes landing clean, see table below).

---

## Executive Summary

**Severity tally: 0 CRITICAL · 0 HIGH · 0 MEDIUM · 0 LOW. 0 regressions. 0 NEW findings.**

Every per-category convergence claim in `nifal.md` §2 re-verified against the
current tree still holds. The four tier invariants — **single-boundary**,
**no-fabrication**, **no-leak**, **no-render-time-fallback** — pass on every
category, with re-derived evidence (not just re-stated from the prior report):

- `translate_material` has exactly **2** production callers
  (`scene/nif_loader.rs:828`, `cell_loader/spawn.rs:880`); the other `Material {`
  literal hits in the tree are test helpers (`byroredux/src/helpers.rs::mat()` —
  a `-> Material` return type, not a construction) and `cornell.rs` (the
  self-contained RT harness with no game-data import path, out of NIFAL's
  Imported*→Material scope by design).
- `resolve_pbr` clamps `metalness ∈ [0,1]` / `roughness ∈ [0.04,1]` and only
  fills `NaN` sentinels via the keyword classifier backstop (`material.rs:638-657`).
- `classify_glass_into_material` runs once, inside `translate_material`,
  immediately after `resolve_pbr` (`material_translate.rs:160-161`) — order intact.
- `resolve_shape_inner` carries **16** `downcast_ref::<Bhk*Shape>` arms
  (matches the #1334-era count exactly), `BhkPlaneShape` still the one
  documented `None` exception.
- `hkMotionType` byte→`MotionType` collapse intact at `import/collision.rs:147-152`
  (`havok_motion_type`), with the canonical `Dynamic/Keyframed/Static/CharacterKinematic`
  variants — no raw-byte leak downstream.
- `apply_emitter_overlays` has exactly **2** production callers
  (`scene/nif_loader.rs:527`, `cell_loader/spawn.rs:436`); the third hit in
  `systems/particle.rs:548` is inside a `#[test]` fn.
- `convert_nif_clip` has its documented multiple legitimate callers (6 sites);
  no second `Imported*→AnimationClip` construction path.
- `crates/renderer/shaders/` (incl. `include/*.glsl`) has **zero** `if game ==`
  occurrences.
- All 7 raw-tier-parked Dim-4 fields (`bs_value_node`, `bs_ordered_node`,
  `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`)
  have zero non-test consumers (grep hits are exclusively `assert!(...is_none())`
  in `crates/spt/src/import/mod.rs` or absent entirely for the last three).
- `translation_completeness.rs` thresholds unchanged (`>= 60.0` / `>= 40.0` /
  `>= 60.0` / `>= 40.0` fill-rate floors at lines 347/352/422/438).
- FO4 `Model_Space_Normals` (F4SF1 bit 12) / `Alpha_Test` (F4SF2 bit 25) still
  reach `MaterialInfo` in `import/material/walker.rs:311-331` (#1592 intact).

### Commits since the 2026-07-02 sweep (`1b4e8e84..8498e559`), NIFAL-relevance triage

| Commit | Touches NIFAL surface? | Verdict |
|---|---|---|
| `ba728882` #1828/#1829 — keep iterating past sentinel `BSGeometry` mesh slots | **Yes** — Geometry dimension, `crates/nif/src/import/mesh/bs_geometry.rs` | **clean fix, closes a real no-leak violation** — both Stage A (`Internal`) and Stage B (`External`) previously accepted the *first* slot that merely parsed, even when its body was the `scale<=0` sentinel (empty vertices/triangles); a sentinel-first slot order silently dropped a populated mesh at a later LOD slot. Both branches now require non-empty vertices+triangles before accepting, with a 299-line regression-test sibling (`bs_geometry_sentinel_slot_tests.rs`). Verified: this closes what would have been a fresh Dim-2 no-leak finding had it landed unfixed — reported here as **verified-fixed**, not re-flagged. |
| `1748e148` #1819 — classify SpeedTree placeholder billboard PBR at import time | **Yes** — Material dimension (backstop-classifier collision), `crates/spt/src/import/mod.rs` | **clean fix** — `placeholder_billboard_mesh` previously left `metalness_override`/`roughness_override` both `None`, so SpeedTree foliage was the *only* production content reaching `resolve_pbr`'s `classify_pbr_keyword` texture-substring backstop (every NIF mesh path classifies at import and sets `Some(...)`). Leaf texture names collided with unrelated keyword buckets with no word-boundary check (`"...boxwood..."` → WOOD; `"...genericelderberry..."` → GLASS across a word seam, crossing the RT-reflection roughness gate). Now sets explicit foliage-default overrides (`Some(0.0)`/`Some(0.85)`) at import, matching the classify-at-import convention every other importer follows. This is exactly the pattern the NIFAL no-render-time-fallback / material dimension exists to enforce — verified fixed. |
| `9f48a16e` #1804 — gate the two-sided blend split on `z_write` | No — renderer batching correctness (`crates/renderer/src/vulkan/context/draw.rs`), consumes existing canonical `two_sided`/`z_write`/`alpha_blend` fields with no new construction site | out of NIFAL scope (owned by `/audit-renderer` / `/audit-performance`) |
| `27334481` #1823 — remove wrong 0/1 blend-factor swap corrupting FO4 Additive/Multiplicative | Partial — `byroredux/src/asset_provider/material.rs` sets `ImportedMesh.src_blend_mode`/`dst_blend_mode` (raw-tier fields), consumed identically at both `nif_loader.rs:783-784` and `spawn.rs:976-977` as a straight passthrough into `DrawCommand` | out of `Material` boundary scope (blend mode is not a `Material` field; it rides a separate raw-tier byte pair with one decode site — `bgsm_blend_to_gamebryo` — and one passthrough at both spawn sites, no fork risk). Renderer-observable correctness fix, owned by `/audit-renderer`; re-checked here only to confirm it does not introduce a second `Material`-adjacent construction site — it does not. |
| `e3e9df0d` #1795 — quantize particle color fade for `MaterialTable` dedup | No — `byroredux/src/render/particles.rs::emit_particles`, per-frame procedural `DrawCommand` construction from live `ParticleEmitter` ECS state, not an `Imported*→Material` translation | out of NIFAL scope (particles are procedural at render time, not NIF-imported meshes with a `Material` component; owned by `/audit-performance`) |
| `ffe9a816` #1718 — log dropped ragdoll bodies/constraints on bone-name miss | No — `byroredux/src/ragdoll.rs::template_from_imported`, a PHYSAL consumer (post-collision-resolve ragdoll template application), not the `resolve_shape_inner` bhk*Shape→CollisionShape boundary this audit's Dim 6 owns | out of NIFAL scope (owned by `/audit-scripting`-adjacent PHYSAL / `docs/engine/physal.md`); diagnostic-only change (added `log::warn!`), no translation logic changed |
| remaining 25 commits | No | ESM/WATR record fix, `.pex` decompiler hardening, save/load validation, BLAS/TLAS scratch-buffer lifecycle, feature-matrix doc corrections, skill-file refresh — none touch `crates/nif/src/import/`, `material_translate.rs`, `anim_convert.rs`, `systems/particle.rs::apply_emitter_overlays`, or `shader_flags.rs` |

### Convergence status vs spec §2 leak inventory

| Category | Spec status | Verified 2026-07-03 |
|---|---|---|
| Materials | converged | ✅ confirmed — single boundary, 2 production callers, plain-`f32` resolved PBR, glass-once ordering, three-way flag union, emissive pass-through unchanged. The #1819 fix is a *consumer-side* hygiene win (SpeedTree now classifies at import like every other importer) rather than a change to the boundary itself. |
| Geometry / transform | converged | ✅ confirmed, **strengthened** by #1828/#1829 — the Starfield `BSGeometry` sentinel-slot no-leak gap is closed; per-game vertex decode still converges to one renderer-space `Vec<[f32;3]>`+`Vec<u32>` |
| Skinning | converged | ✅ confirmed unchanged — `skin.rs` untouched this delta; #613 global bone indices, u16 guard, `global_skin_transform` intact |
| Lights | converged | ✅ confirmed unchanged — `imported_light_from_base` → `LightKind`; no block-type `match` outside comments in spawn/renderer code |
| Nodes | triaged | ✅ confirmed — all 7 parked fields (4 Nodes-table + 3 Passthroughs-table: `bs_lod_cutoffs`, `bs_sub_index` counted once each) still zero non-test consumers |
| Particles | converged | ✅ confirmed unchanged — single overlay boundary, 2 production callers; `initial_color` still correctly unapplied |
| Collision | audited | ✅ confirmed unchanged — 16 shape-resolve arms, documented `BhkPlaneShape` None-drop, `hkMotionType` collapse intact |
| Animation / controllers | converged | ✅ confirmed unchanged — `convert_nif_clip` single boundary, 6 legitimate callers |
| Shader flags / effects | converged | ✅ confirmed unchanged — zero `if game ==` in shaders; FO4 `model_space_normals`/`alpha_test` bits intact |

---

## Per-Category Tier Matrix

Legend: SB = single-boundary · NF = no-fabrication · NL = no-leak ·
NRTF = no-render-time-fallback. `N-A` = the invariant does not apply to that
category.

| Category | SB | NF | NL | NRTF | Boundary fn |
|---|---|---|---|---|---|
| Materials | PASS | PASS | PASS | PASS | `byroredux/src/material_translate.rs::translate_material` |
| Geometry / transform | PASS | PASS | PASS | PASS | `crates/nif/src/import/{coord,transform,rotation,mesh/*}.rs` (parse-time) |
| Skinning | PASS | PASS | PASS | N-A | `crates/nif/src/import/mesh/skin.rs` (#613 remap at extraction) |
| Lights | PASS | PASS | PASS | PASS | `crates/nif/src/import/walk/mod.rs::imported_light_from_base` → `LightKind` |
| Nodes | N-A (by design) | PASS | PASS | PASS | *no single boundary* (two structurally-different spawn paths — spec §2 Nodes) |
| Particles | PASS | PASS | PASS | PASS | `byroredux/src/systems/particle.rs::apply_emitter_overlays` |
| Collision | PASS | PASS | PASS | N-A | `crates/nif/src/import/collision.rs::resolve_shape`/`resolve_shape_inner` |
| Animation / controllers | PASS (multi-caller, single logic) | PASS | PASS | N-A | `byroredux/src/anim_convert.rs::convert_nif_clip` |
| Shader flags / effects | N-A (per-category, dispatched by block type) | PASS | PASS | PASS | `crates/nif/src/shader_flags.rs` + `import/material/walker.rs` |

---

## Findings

**None.** No new findings, no regressions of prior findings, no re-opened
"documented limitation" items. This is the fourth consecutive clean sweep
(2026-06-28, 2026-07-02, 2026-07-03) — the NIFAL surface has been stable
since the collision/particle/geometry convergence work landed in late June.

---

## Documented-limitation ledger (restated so they are not re-reported)

These are **known, bounded, deliberate** gaps — not leaks. Re-verified present
and still accurately described as of this sweep:

- **Nodes (raw-tier-parked, zero consumers)**: `bs_value_node` (`BSValueNode`,
  LOD-distance/billboard hint, FO3/FNV), `bs_ordered_node` (`BSOrderedNode`,
  alpha-sort/draw-order hint), `tree_bones` (`BSTreeNode`, SpeedTree bone
  names), `range_kind` (`BSRangeNode`/`BSDamageStage`/`BSBlastNode`/`BSDebrisNode`,
  destructible discriminator) — each blocked on a not-yet-built consumer
  system (M35 LOD selector, `RenderOrderHint`, SpeedTree wind sim,
  destructible-switching respectively).
- **Passthroughs**: `lod_group` (`NiLODNode`→`NiRangeLODData`, foundation done,
  content-absent in shipped archives — forward-compat only), `bs_lod_cutoffs`
  (`BSLODTriShape`, content-bearing Skyrim in-cell LOD, foundation parked,
  runtime draw-count switch deferred — perf-only gain, poor risk/reward per
  the measure-first verdict), `bs_sub_index` (`BSSubIndexTriShape`,
  dismemberment segment ids), `ImportedTextureEffect` (`NiTextureEffect`,
  extracted but never called — content-absent, 0 occurrences measured), furniture/inv
  markers (`BSFurnitureMarker`/`BSInvMarker`, parsed not walked), `NiSwitchNode`
  identity (walked via active-index, type discriminator not surfaced —
  content-present but gameplay-gated), `bs_bound` (`BSBound`, loose-NIF path
  only, cell path derives `WorldBound` from geometry instead).
- **Collision documented limitations**: `BhkPlaneShape` → `None` (#1334, no
  half-space `CollisionShape` variant yet; trimesh fallback renders the
  correct ground surface). `BhkNPCollisionObject` (FO4+ Havok-serialised
  `BhkSystemBinary` blob — decoder is a separate project; `spawn.rs::synthesize_static_trimesh`
  fallback for Architecture). `BhkPCollisionObject` phantoms (Skyrim+ trigger
  volumes — need a `TriggerVolume` ECS path, not a rigid body).
- **Particles**: size-over-life *curve* (grow→steady→fade bell shape; only
  the authored magnitude is translated to the linear `start_size→end_size`
  model — a richer canonical size model is future work). Per-emitter (vs
  scene-first) attribution for multi-emitter NIFs. `initial_color`
  intentionally unapplied (colour stays owned by `color_curve`).
- **Animation**: per-light ambient colour channels and morph-weight channels
  — captured, no renderer consumer yet, deliberately parked.
- **Shader flags**: `BSEffectShaderProperty.base_color_scale`
  diffuse-tint-vs-emissive render path — tagged via `EmissiveSource::Effect`,
  not dropped; deferred to a future BSEffect-proper render path.
- **Emissive scale**: no normalization applied across `Material`/`Lighting`/
  `Effect` `EmissiveSource` variants — all three measured (Oblivion/FNV/Skyrim
  SE/FO4) to already share a ~1.0 scale; a future "normalization constant"
  would be a `no-fabrication` regression (Q2 in `material-abstraction.md`
  resolved no-op).

---

## Notes on audit scope discipline

Two commits in this delta (`27334481` blend-factor fix, `e3e9df0d` particle
color quantization, `9f48a16e` two-sided blend split, `ffe9a816` ragdoll
logging) were deliberately triaged **out of scope** rather than folded in as
NIFAL findings, per the audit's own scope boundary (`/audit-nif` owns parse
correctness; `/audit-renderer`/`/audit-performance` own render-time GPU
behavior; PHYSAL/`docs/engine/physal.md` owns post-collision-resolve ragdoll
template logic). Each was still checked for one thing relevant to NIFAL's
mandate — whether it introduced a second `Material`/`CollisionShape`/
`AnimationClip` construction site outside the declared boundary — and none did.

---

Suggest: `/audit-publish docs/audits/AUDIT_NIFAL_2026-07-03.md` (no-op — zero
findings to publish as issues this sweep).
