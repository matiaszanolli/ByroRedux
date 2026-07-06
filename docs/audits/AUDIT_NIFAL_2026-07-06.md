# NIFAL Audit — Canonical Translation Layer — 2026-07-06

Deep audit of **NIFAL** (the NIF Abstraction Layer; spec: `docs/engine/nifal.md`).
Scope: the *translate* side — does each parsed NIF data category reach one
canonical, game-agnostic representation through a single explicit `translate()`
boundary, with no leak / fabrication / render-time fallback? (Parse-side
correctness belongs to `/audit-nif`.)

All 9 dimensions were re-verified against the live tree @ HEAD `d59f40ac`.
Dedup baseline: `/tmp/audit/issues.json` (36 OPEN) + the prior
`docs/audits/AUDIT_NIFAL_2026-07-03.md` (HEAD `8498e559`, 0 findings) and
`AUDIT_NIFAL_2026-07-02.md` (0 findings), plus `AUDIT_NIFAL_2026-06-28.md`,
`AUDIT_NIFAL_2026-06-23.md`, `AUDIT_NIFAL_2026-06-14.md`,
`AUDIT_NIFAL_2026-06-13.md`, `AUDIT_NIFAL_2026-05-30.md`.

Delta since the 2026-07-03 sweep: **78 commits** (`8498e559..d59f40ac`). Every
commit was triaged for NIFAL relevance; four touch the translate surface and are
detailed in the triage table below — all four land clean.

---

## Executive Summary

**Severity tally: 0 CRITICAL · 0 HIGH · 0 MEDIUM · 0 LOW. 0 regressions. 0 NEW findings.**

Every per-category convergence claim in `nifal.md` §2 re-verified against the
current tree still holds. The four tier invariants — **single-boundary**,
**no-fabrication**, **no-leak**, **no-render-time-fallback** — pass on every
category, with re-derived evidence (not merely re-stated from prior reports):

- `translate_material` has exactly **2** production callers
  (`byroredux/src/scene/nif_loader.rs:838`, `byroredux/src/cell_loader/spawn.rs:911`).
  The only other `Material {` struct literals in the tree are the boundary itself
  (`byroredux/src/material_translate.rs:78`) and `byroredux/src/cornell.rs` (the
  self-contained RT harness with no game-data import path — out of NIFAL scope by
  design). `ResolvedMaterial`/`BaseMaterial` literals are the BGSM template types,
  not the canonical ECS `Material`.
- `resolve_pbr` clamps `metalness ∈ [0,1]` / `roughness ∈ [0.04,1]`
  (`crates/core/src/ecs/components/material.rs:684-685`) and only fills `NaN`
  sentinels via the keyword-classifier backstop.
- Glass is classified **once**, inside `translate_material`
  (`material_translate.rs:161`), immediately after `resolve_pbr`
  (`material_translate.rs:160`) — ordering intact so forced glass roughness wins.
- `resolve_shape_inner` carries **16** `downcast_ref::<Bhk*Shape>` arms
  (`crates/nif/src/import/collision/shape.rs`) — the #1334-era count exactly,
  preserved through the #1876 file split (collision.rs → `collision/{mod,shape,ragdoll}.rs`).
  `BhkPlaneShape` remains the one documented `None` exception.
- `hkMotionType` byte → canonical `MotionType` collapse intact
  (`crates/nif/src/import/collision/mod.rs:156-163`), full-enum
  (`1..=5|8 → Dynamic`, `6 → Keyframed`, `7 → Static`, `9 → CharacterKinematic`) —
  not the pre-fix `4 => Keyframed / _ => Static` regression. No raw-byte leak downstream.
- `apply_emitter_overlays` has exactly **2** production callers
  (`scene/nif_loader.rs:537`, `cell_loader/spawn.rs:445`); the two other hits
  (`systems/particle.rs:521,548`) are inside the `#[cfg(test)] mod tests` block
  (opens at `particle.rs:435`).
- `convert_nif_clip` has its documented multiple legitimate callers (6 sites:
  `scene.rs:301`, `scene/nif_loader.rs:1115`, `npc_spawn.rs:312`,
  `systems/animation.rs:1372`, `cell_loader/partial.rs:106`,
  `cell_loader/references/mod.rs:685`); no second `Imported*→AnimationClip`
  construction path.
- `crates/renderer/shaders/` (incl. `include/*.glsl`) has **zero** `if game ==`
  occurrences.
- All 7 raw-tier-parked Dim-4 fields (`bs_value_node`, `bs_ordered_node`,
  `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`)
  have zero non-test/non-parser consumers — every live grep hit is a test file,
  a populate site (`import/mesh/{bs_tri_shape,ni_tri_shape,bs_geometry}.rs`), or
  a `…: None` construction default (`cell_loader.rs:361-362`). None reaches a
  canonical ECS component.
- Skinning u16 partition guard intact (`crates/nif/src/import/mesh/skin.rs:147`);
  no renderer/`render/` match on source light block type (0 hits).
- FO4 `model_space_normals` (F4SF1 bit 12) / `alpha_test` (F4SF2 bit 25) still
  reach `MaterialInfo` (`import/material/walker.rs:332,342`) — #1592 intact.
- `translation_completeness.rs` fill-rate floors unchanged (`>= 60.0` at line 347, …).

### Commits since the 2026-07-03 sweep (`8498e559..d59f40ac`) — NIFAL-relevance triage

| Commit | Touches NIFAL surface? | Verdict |
|---|---|---|
| `634873db` #1873 — gate PBR env-map metalness lift on authored specular, not the struct default | **Yes** — Material dimension (`crates/core/src/ecs/components/material.rs`, `crates/nif/src/import/material/mod.rs`) | **clean fix, strengthens no-render-time-fallback.** The env-map arm in `classify_pbr_keyword` read `specular_color` luminance to infer metalness but couldn't distinguish an authored white specular from `MaterialInfo`'s unauthored `[1,1,1]` default, chroming `BSShaderPPLightingProperty`-only decorative FO3/FNV meshes. Now threads `specular_authored` (backed by the real `MaterialInfo::has_material_data` signal, populated only by the NiMaterialProperty/BSLightingShaderProperty walker arms); unauthored surfaces stay dielectric. The `resolve_pbr` backstop path passes `specular_authored: false` conservatively — consistent with `classify_legacy_pbr`. No fabrication (flag is a real authored signal), no new construction site, classifies at import. Verified fixed. |
| `41152f13` #1876 — split `import/collision.rs` (2587 LOC) into `collision/{mod,shape,ragdoll}.rs` | **Yes** — Collision dimension (structural) | **clean refactor.** All 16 `Bhk*Shape` resolve arms preserved (moved into `collision/shape.rs`); `havok_motion_type` + `havok_scale` moved to `collision/mod.rs` intact. Skill/`_audit-common` paths already point at the split layout (`crates/nif/src/import/collision/mod.rs`). No arm dropped, no leak introduced. |
| `88d41600` #1850/#1851 — surface dropped breakable ragdoll edges + pin measured joint counts | Partial — `crates/nif/src/import/collision/ragdoll.rs` | out of NIFAL Dim-6 shape-resolve scope (ragdoll template = PHYSAL consumer, `docs/engine/physal.md`). Diagnostic + parked-edge surfacing; does not alter the `resolve_shape_inner` bhk*Shape→`CollisionShape` boundary. |
| `155852e3` #1885 — route NiBlendInterpolator blend-array counts through `allocate_vec` | No — `crates/nif/src/blocks/interpolator.rs` (parse-side allocation guard) | out of NIFAL scope (parse-side, owned by `/audit-nif`); does not change the `convert_nif_clip` translate boundary. |
| `550ff215` Starfield shader/weak-ref stream drift; `450691e0` #1838/#1839 restore raw-BSVER gates; `8b50e238` #1840/#1841 dead NifVariant helpers; `45a0239d` Skyrim SLSF2 bit-21 vocab; `61a95570` #1830 BSGeometry hint cross-check | No | parse-side correctness / block coverage / shader-flag *vocabulary* corrections in `crates/nif/src/blocks/` + `shader_flags.rs` — read-side, owned by `/audit-nif` + the per-game skills. None adds a second translate construction site or leaks a per-game branch downstream. |
| `a8d65d6c`/`d4b981fa` #1889/#1890/#1891 — materialise the VWD flag as a `VisibleWhenDistant` marker | No — ESM cell-placement flag (`crates/plugin/src/esm/cell/`, `byroredux/src/cell_loader/references/mod.rs`, `docs/engine/exal.md`) | out of NIFAL scope (EXAL exterior/cell-placement, not a NIFAL NIF-node field). Confirmed it does **not** consume the parked `bs_value_node` billboard-mode hint — it is sourced from the ESM VWD record flag. |
| remaining ~66 commits | No | CHARAL ruleset docs, renderer/RT denoiser doc + batching fixes (#1874 ghosting viz, #1800/#1801/#1804-#1817), save-registry (#1862), ECS lock-poison naming (#1836/#1837), resources.rs/cell_loader/references module splits, audit reports — none touch `crates/nif/src/import/`, `material_translate.rs`, `anim_convert.rs`, `systems/particle.rs::apply_emitter_overlays`, or the collision/material/shader-flag translate surface. |

### Convergence status vs spec §2 leak inventory

| Category | Spec §2 status | Verified 2026-07-06 |
|---|---|---|
| Materials | converged | ✅ single boundary (2 callers), plain-`f32` resolved PBR, glass-once ordering, three-way flag union, emissive pass-through unchanged. #1873 tightens the backstop classifier (authored-specular gate) — a consumer-side hardening, not a boundary change. |
| Geometry / transform | converged | ✅ per-game vertex decode converges to one renderer-space `Vec<[f32;3]>`+`Vec<u32>`; Z-up→Y-up once; SVD repair once at parse. |
| Skinning | converged | ✅ global bone indices (#613), u16 partition guard intact, `global_skin_transform` carried. |
| Lights | converged | ✅ `LightKind` enum + derived radius; renderer never inspects source block type (0 hits). |
| Nodes | triaged (parked-not-leak) | ✅ live fields consumed at spawn sites; 7 parked fields have zero canonical consumers. |
| Particles | converged (emitter overlay) | ✅ `apply_emitter_overlays` single boundary (2 callers); `initial_color` intentionally unapplied; force fields Z-up→Y-up at overlay time. |
| Collision | audited/converged | ✅ 16 shape arms (post-#1876 split), `BhkPlaneShape` the one documented `None`; `hkMotionType` full-enum collapse; `havok_scale` applied uniformly. |
| Animation / controllers | converged | ✅ single `convert_nif_clip` boundary (6 legit callers); no `Option`/era branch downstream. |
| Shader flags / texture sets / effect shaders | converged | ✅ dispatched by block type; 0 shader `if game ==`; FO4 F4SF flags reach `MaterialInfo` (#1592). |

---

## Per-Category Tier Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Material | `material_translate::translate_material` | PASS (2 callers) | PASS (emissive no-op §4) | PASS (plain `f32`) | PASS (0 shader branches; #1873 classifies at import) |
| Geometry/Transform | `import/mesh/*` + `import/coord.rs` | PASS | PASS | PASS | PASS |
| Skinning | `import/mesh/skin.rs` (extraction) | PASS | PASS | PASS (global indices) | N-A |
| Lights | `import/walk/mod.rs` → `LightKind` | PASS | PASS | PASS | PASS (no block-type match) |
| Nodes | (by design, no single boundary) | N-A (spec §2) | PASS | PASS (parked, 0 consumers) | N-A |
| Particles | `systems::apply_emitter_overlays` | PASS (2 callers) | PASS (initial_color unapplied) | PASS | N-A |
| Collision | `import/collision/shape.rs::resolve_shape_inner` | PASS | PASS (motion full-enum) | PASS (16 arms) | N-A |
| Animation | `anim_convert::convert_nif_clip` | PASS (6 callers, one fn) | PASS | PASS (quaternion keys) | PASS |
| Shader flags | `shader_flags.rs` + `import/material/walker.rs` | PASS (block-type dispatch) | PASS | PASS (FO4 flags reach MaterialInfo) | PASS (0 shader `if game ==`) |

---

## Findings

**None.** No CRITICAL / HIGH / MEDIUM / LOW findings. No regressions.

### Premises checked and disproved (stale-premise guard)

- *"The #1876 collision.rs split dropped a shape arm."* — **Disproved.** Arm count
  is still 16 in `collision/shape.rs`; the same 16 `Bhk*Shape` types are present.
- *"The #1873 material change reintroduced a render-time PBR classifier."* —
  **Disproved.** #1873 does the opposite: it makes the *import-time* classifier
  more conservative (gates the env-map metalness lift on authored specular). No
  render-side `classify_pbr` was added; `render/static_meshes.rs` still reads
  `m.metalness`/`m.roughness` directly.
- *"The VWD #1889 change consumes the parked `bs_value_node` billboard hint."* —
  **Disproved.** `VisibleWhenDistant` is materialised from the ESM cell-placement
  VWD record flag (`esm/cell/support.rs` + `cell_loader/references/mod.rs`), an
  EXAL/cell-path concern, not the NIFAL NIF-node passthrough field.

---

## Documented-limitation ledger (parked-not-leak — do NOT re-report)

These are captured-but-unconsumed by design; each is blocked on a consumer feature
that does not exist yet, so translating now would invent an ECS component nothing
reads (the no-fabrication rule). Re-verified zero canonical consumers at HEAD `d59f40ac`:

- **Node passthroughs:** `bs_value_node` (BSValueNode LOD/billboard hint),
  `bs_ordered_node` (alpha-sort/draw-order), `tree_bones` (SpeedTree bone names),
  `range_kind` (destructible/blast/debris discriminator), `lod_group`
  (NiLODNode — content-absent foundation), `bs_lod_cutoffs` (BSLODTriShape
  mesh-level LOD, Skyrim ~43 meshes), `bs_sub_index` (BSSubIndexTriShape
  dismemberment segments).
- **Mesh/scene passthroughs:** `ImportedTextureEffect` (NiTextureEffect —
  content-absent, dead extractor), `NiSwitchNode` identity (active-index walked,
  discriminator unsurfaced), `BSFurnitureMarker`/`BSInvMarker`, `BSBound` (loose
  path only).
- **Collision:** `BhkNPCollisionObject` (FO4+ Havok-serialised `BhkSystemBinary`
  blob — separate decoder project; cell loader falls back to synthesized static
  trimesh), `BhkPCollisionObject` phantoms (need a `TriggerVolume` ECS path),
  `BhkPlaneShape` (returns `None` — no half-space `CollisionShape` variant;
  trimesh fallback renders the ground surface).
- **Particles:** size-over-life *curve* (grow→steady→fade bell shape; only the
  authored magnitude is translated), per-emitter multi-emitter attribution.
- **Animation:** per-light **ambient** colour channels and **morph-weight**
  channels (captured, no renderer consumer yet).
- **Emissive scale:** resolved **no-op** (spec §4) — all three `EmissiveSource`
  variants measured across Oblivion/FNV/Skyrim/FO4 share a ~1.0 scale; a future
  normalization constant would be a `no-fabrication` violation in reverse.

---

## Method notes

- 78-commit delta (`8498e559..d59f40ac`) triaged commit-by-commit for NIFAL
  relevance; four translate-surface commits inspected in full (table above).
- Core invariants re-derived by grep/read against the live tree, not carried over
  from the prior report: boundary caller counts, shape-arm count, motion-type
  collapse, shader `if game ==` count, parked-field consumer scan, skinning guard,
  FO4 flag routing, completeness thresholds.
- Game data present for Oblivion / FNV / Skyrim SE / FO4 / Starfield (per
  `_audit-common.md`); the `#[ignore]`-gated `translation_completeness.rs` harness
  was not driven this sweep (its thresholds were confirmed unchanged; no extractor
  under it was modified in the delta).
