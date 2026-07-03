---
description: "Deep audit of NIFAL — the NIF Abstraction Layer (canonical translation tier): Imported* → translate() → Canonical, single-boundary / no-fabrication / no-render-time-fallback"
argument-hint: "--focus <dimensions> --game <fnv|fo3|skyrim|oblivion|fo4|fo76|starfield>"
---

# NIFAL Audit — Canonical Translation Layer

Deep audit of **NIFAL** (the NIF Abstraction Layer; spec: `docs/engine/nifal.md`).
NIFAL is the engine's canonical translation tier — the cornerstone of cross-game
compatibility. It is not a crate; it is the **discipline** that every per-game NIF
data category is folded into one game-agnostic representation through a single
explicit `translate()` boundary, with no `Option` "resolve-later" leaks and no
render-time heuristics downstream.

The three-tier model this audit enforces (per `docs/engine/nifal.md` §1):

```
  NIF bytes ──parse──▶  Imported*  ──translate()──▶  Canonical  ──consume──▶  ECS / GPU
            (per-game,             (one site per       (the ECS               (no per-game
             raw, messy)           category, folds     component when         branches, no
                                   in every quirk)     one already            Option fallback)
                                                       serves the role)
```

**The canonical-type rule** (spec §1): *where an ECS component already serves the
game-agnostic, engine-facing role, that component IS the canonical type.* Do NOT
flag the absence of a third `Canonical*` struct as a leak — that is deliberate
(ceremony with no new capability). The canonical tier is reached by (a) making the
`translate()` boundary the sole producer and (b) removing residual `Option`/raw
leaks from the component itself.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game data locations,
methodology, deduplication, context rules, and base finding format.
See `.claude/commands/_audit-severity.md` for the severity scale — it carries
**dedicated NIFAL rows** (wrong `translate_material` output = HIGH all-game blast
radius; translatable block silently dropped = MEDIUM, escalate to HIGH if it
removes visible content) and the matching decision-tree branches. Apply those rows;
do not re-derive severities here.

**Scope vs `/audit-nif`**: `/audit-nif` owns the *parse* side (block field
correctness, version handling, stream position, coverage). NIFAL owns the
*translate* side (does each parsed category reach one canonical representation
through one boundary, with no leak/fabrication/fallback?). When a finding is "the
bytes are read wrong," it belongs to `/audit-nif`; when it is "the bytes are read
fine but the data is dropped, duplicated, or resolved per-game downstream," it
belongs here.

## The four tier invariants (every dimension is a lens on these)

- **single-boundary** — exactly one `translate()` site per category that needs one.
  A second construction site that fills a canonical type field-by-field is a
  violation (caller count is the cheap detector).
- **no-fabrication** — no invented value / guessed normalization. A new constant
  must cite a measurement or source (`feedback_no_guessing`). The emissive no-op
  (Dim 1) and particle colour/size-curve deferrals (Dim 5) are the canonical
  "measured, then deliberately NOT normalized" examples.
- **no-leak** — no `Option` "resolve-later" field or raw enum/block-type
  discriminator on a *canonical* type reaches a consumer that has to re-resolve it.
  (Raw-tier `Imported*` carrying `Option`s is fine — the leak is the crossing into
  the canonical/consumer tier.)
- **no-render-time-fallback** — no classification deferred to a per-draw heuristic.
  The deleted `classify_pbr` / render-side glass heuristics are the cautionary tale;
  a re-introduction is a regression, not a new design choice.

**Highest blast radius first.** Order findings by NIFAL risk: (1) a wrong/divergent
canonical `Material` out of `translate_material` (HIGH, silently wrong on *every*
game, no per-draw fallback to mask it); (2) a *translatable* parsed block silently
dropped at an unsupported-shape/skip fallback (removes authored content); (3) a
no-render-time-fallback violation (classification leaked into shader/render); (4) a
single-boundary violation (a second construction site that can diverge the paths).

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,6`). Default: all 9.
- `--game <name>`: Focus on a specific variant: `fnv`, `fo3`, `skyrim`, `oblivion`,
  `fo4`, `fo76`, `starfield`. Default: all detected (from `_audit-common.md` game
  data locations).

## Extra Per-Finding Fields

- **Dimension**: Material | Geometry/Transform | Skinning/Lights | Nodes | Particles
  | Collision | Animation | Shader-flags/Effects | Completeness
- **Tier Violated**: which tier invariant broke — `single-boundary` | `no-fabrication`
  | `no-leak` | `no-render-time-fallback` | `parked-not-leak` (verify a "deferred"
  field is genuinely unconsumed, not a silent drop)
- **Game Affected**: which variant(s) the divergence manifests on

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/nifal`.
3. Fetch dedup baseline:
   `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`,
   and scan `docs/audits/` for prior `AUDIT_NIFAL_*` / `AUDIT_NIF_*` reports.
4. Read `docs/engine/nifal.md` (the spec — per-category leak inventory in §2,
   surveyed/converged status per category) and `docs/engine/material-abstraction.md`
   (the material-slice predecessor; its §2 "Leak A"/"Leak B" are recorded **closed**
   in `nifal.md` §3 — do not re-report them as open).
5. Check which game data directories exist.

## Phase 2: Launch Dimension Agents

### Dimension 1: Material — the reference realisation (single boundary, no PBR fallback, glass-once)
**Entry points**:
- `byroredux/src/material_translate.rs::translate_material(mesh: &ImportedMesh, paths: ResolvedPaths, extra_material_flags: u32) -> Material` — the **single** `ImportedMesh → Material` boundary (`ResolvedPaths` struct also defined here). It calls `Material::resolve_pbr()` then `helpers::classify_glass_into_material` internally.
- Callers (MUST be exactly two, both routing through the boundary): `byroredux/src/scene/nif_loader.rs` (loose-NIF path) and `byroredux/src/cell_loader/spawn.rs` (cell path).
- `crates/core/src/ecs/components/material.rs` — the canonical `Material`; `Material::resolve_pbr()`, `classify_pbr_keyword()` (the `NaN`-sentinel backstop classifier), `EmissiveSource` enum (`None`/`Material`/`Lighting`/`Effect`).
- `byroredux/src/helpers.rs::classify_glass_into_material` — the single glass classifier (defined once; the many call sites in that file are its unit tests).
- `byroredux/src/render/static_meshes.rs` — the renderer consumer (reads `m.metalness` / `m.roughness` directly).

**Checklist**:
- `translate_material` is the ONLY site that fills a `Material` from an `ImportedMesh`. Any second site building a `Material` field-by-field from import data is a `single-boundary` violation (the pre-converged state had two ~110-line literals in `spawn.rs` + `nif_loader.rs` — regression pattern).
- `Material.metalness` / `Material.roughness` are plain `f32` (resolved, clamped `metalness ∈ [0,1]`, `roughness ∈ [0.04,1]` — verify the clamp in `resolve_pbr`). No `metalness_override: Option<f32>` / `roughness_override: Option<f32>` field on the canonical `Material`, and no per-draw `classify_pbr` fallback in the renderer (`no-leak` + `no-render-time-fallback`).
- `resolve_pbr()` only fills `NaN` sentinels (via `classify_pbr_keyword`) then clamps; for NIF/BGSM content the override is already `Some(…)` at import so the classifier arm is a backstop. It must not overwrite an authored BGSM/BGEM override.
- Glass is classified **once**, alpha-aware, inside `translate_material` via `classify_glass_into_material`, AFTER `resolve_pbr` so the forced glass roughness wins. Engine-synthesized kinds (`material_kind >= 100`) are never demoted; conductors (`metalness >= 0.3`), non-alpha, and decals are gated out. No glass heuristic at render time (`material-abstraction.md` §2 "Leak A" is closed — confirm it stays deleted).
- `material_kind: u32` is intentionally kept as-is (it is the GPU shader-dispatch contract — the `material_kind == N` ladder in `triangle.frag`; 0–20 vanilla `shader_type`, 100 GLASS, 101 EFFECT_SHADER). Do NOT flag its `u32`-ness as a leak. **Future-slice invariant**: any `SurfaceClass` enum MUST lower to the exact `triangle.frag` ladder (drift risk vs the shader — a shader-adjacent change).
- `effect_shader_flags` packs the union of BSEffect SLSF bits (`cell_loader::pack_effect_shader_flags`) + BGSM v>2 bits (`cell_loader::pack_bgsm_material_flags`) + the caller's `extra_material_flags` (REFR-overlay model-space-normals on the cell path; `0` on the loose path).

**Regression pins**:
- **material_translate dedup**: the two duplicate `Material` construction literals were collapsed into `translate_material`. A field added in one load path that doesn't go through the boundary can silently diverge the two paths — the regression this dedup prevents.
- **Emissive scale = no-op** (spec §4): `Material.emissive_mult` is fed by three `EmissiveSource` variants (`Material` legacy / `Lighting` Skyrim+ / `Effect` FO4+). All three were **measured** across Oblivion/FNV/Skyrim/FO4 and already share a ~1.0 scale — **no normalization is applied or wanted**. A future "emissive normalization constant" is a `no-fabrication` violation (inventing a correction for a divergence the ground truth shows does not exist). The one genuine distinction (`BSEffectShaderProperty.base_color_scale` is a diffuse-tint, not emissive) is captured by the `EmissiveSource::Effect` discriminator and left for a future BSEffect render path (#166 rename note in `import/material/walker.rs`). Open question Q2 in `material-abstraction.md` is resolved no-op — do not re-open it. Tooling: `crates/nif/examples/material_dump.rs` (the `emisM` + `emSrc` columns).
**Output**: `/tmp/audit/nifal/dim_1.md`

### Dimension 2: Geometry / Transform — the cleanest category (the template the others match)
**Entry points**:
- `crates/nif/src/import/coord.rs` — Z-up (Gamebryo) → Y-up (renderer): `zup_point_to_yup`, `zup_matrix_to_yup_quat` (thin wrappers over `byroredux_core::math::coord`).
- `crates/nif/src/import/mesh/tangent.rs` — `synthesize_tangents` / `synthesize_tangents_yup` (Mikkelsen synthesis fallback).
- `crates/nif/src/import/mesh/` per-game extractors — `ni_tri_shape.rs`, `bs_tri_shape.rs`, `bs_geometry.rs`; each feeds `ImportedMesh.local_bound_radius` (field on `crates/nif/src/import/types.rs`, derived via `mesh::extract_local_bound`).
- `crates/nif/src/rotation.rs` — degenerate-rotation SVD repair: `is_degenerate_rotation`, `repair_rotation_svd_or_identity`, `sanitize_rotation` (done ONCE at parse time — see #277; NOTE the repair lives in `rotation.rs`, not `transform.rs`).
- `crates/nif/src/import/transform.rs::compose_transforms` — parent×child composition (assumes rotations already sanitized).
- Consumer: `crates/renderer/src/mesh.rs::MeshRegistry::upload` (format-agnostic).

**Checklist**:
- Every per-game vertex decode (classic `NiTriShape`, Skyrim packed-half `BSTriShape`, Starfield `BSGeometry` UDEC3) converges to a single `Vec<[f32;3]>` positions + `Vec<u32>` indices in renderer space. No `Option`-gated "decode-later" geometry reaches the consumer.
- Z-up→Y-up is applied consistently at the import boundary, not duplicated per-consumer; the renderer never re-handles coordinate frames.
- Tangents either come from authored extra-data OR Mikkelsen synthesis — one resolved tangent array reaches the vertex buffer (no per-game tangent branch in the shader).
- SVD rotation repair fires once at parse; `compose_transforms` / `zup_matrix_to_yup_quat` assume valid rotations (don't re-check per composition). A consumer re-validating rotations is a leak of raw-tier messiness.
- `local_bound_radius` is derived in renderer (Y-up) space at extraction. No render-time bound recomputation.
**Output**: `/tmp/audit/nifal/dim_2.md`

### Dimension 3: Skinning & Lights
**Entry points**:
- Skinning: `crates/nif/src/import/mesh/skin.rs` — `ImportedSkin` (struct on `crates/nif/src/import/types.rs`, with `global_skin_transform`), the #613 partition-local→global bone-index remap (done at extraction).
- Lights: `crates/nif/src/import/types.rs::LightKind` (`Ambient`/`Directional`/`Point`/`Spot`) + `ImportedLight.radius`; populated in `crates/nif/src/import/walk/mod.rs`.

**Checklist**:
- **Skinning** (`no-leak` / converged): `ImportedSkin` emits **global** bone indices — partition-local remap done at extraction (#613 / SK-D1-01: pre-#613 silently aliased every vertex past partition 0). The defensive u16-range warning in `skin.rs` (the `bone_refs_slice.len() > u16::MAX` guard) must stay. `global_skin_transform` carried through. Palette skinning is game-agnostic downstream — no consumer should re-derive partition layout.
- **Lights** (`no-leak` / converged): `ImportedLight` resolves to the `LightKind` enum with a derived effective `radius` (Bethesda units, from attenuation). The renderer must NEVER inspect the source NIF block type (NiAmbientLight / NiDirectionalLight / NiPointLight / NiSpotLight) — that is the raw-tier discriminator collapsed at translate. A downstream `match` on source block type is a leak.
**Output**: `/tmp/audit/nifal/dim_3.md`

### Dimension 4: Nodes — raw-tier-parked passthroughs (verify parked, not silently dropped)
**Entry points**:
- `crates/nif/src/import/types.rs` — parked `ImportedNode` fields: `bs_value_node: Option<BsValueNodeData>`, `bs_ordered_node: Option<BsOrderedNodeData>`, `tree_bones: Option<TreeBones>`, `range_kind: Option<BsRangeKind>`, `lod_group: Option<LodGroupData>`; parked `ImportedMesh` fields: `bs_lod_cutoffs: Option<[u32;3]>`, `bs_sub_index: Option<BsSubIndexTriShapeData>`.
- Live (canonical) node data consumers: spawn sites in `byroredux/src/scene/nif_loader.rs` + `byroredux/src/cell_loader/spawn.rs` (`name`, `flags`→`SceneFlags`, `collision`→`CollisionShape`/`RigidBodyData`, `billboard_mode`→`Billboard`).

**Checklist**:
- The live node data (name, flags, collision, billboard_mode) IS consumed at the spawn sites — confirm no canonical node field is dropped.
- The `ImportedNode → ECS` step is deliberately NOT a single `translate_node` boundary: the two load paths handle nodes structurally differently (loose-NIF spawns the full NiNode hierarchy as entities; cell loader uses a flattened placement-root). Do NOT flag the absence of one boundary as a `single-boundary` violation for nodes — it is documented (spec §2 Nodes).
- The fields below are **raw-tier-parked with deferred translation** — verify (per-game) they have **zero canonical ECS consumers** (`parked-not-leak`). They sit on the raw `ImportedMesh`/`ImportedNode`, which the tier model permits to carry per-game data, and reach no canonical component. If you find ANY of them feeding a canonical ECS component without a translate step, THAT is a leak finding. (Grep `\.field` / `field:` outside `types.rs`, the parser, and `_tests` — the expected hit count is zero.)

  | Field | Source block | Authored data | Blocked on |
  |---|---|---|---|
  | `bs_value_node` | `BSValueNode` | LOD-distance / billboard-mode hint (FO3/FNV) | M35 LOD selector |
  | `bs_ordered_node` | `BSOrderedNode` | alpha-sort bound + draw-order hint | `RenderOrderHint` + `build_render_data` sort key |
  | `tree_bones` | `BSTreeNode` | SpeedTree branch/trunk bone names | SpeedTree wind/bend sim |
  | `range_kind` | `BSRangeNode`/`BSDamageStage`/`BSBlastNode`/`BSDebrisNode` | destructible/blast/debris discriminator | destructible-switching / blast / debris systems |
  | `lod_group` | `NiLODNode` → `NiRangeLODData` | center + per-level near/far (Y-up); foundation parsed, import walks child 0 only; **content-absent** in shipped archives | per-frame distance-switch system |
  | `bs_lod_cutoffs` | `BSLODTriShape` | mesh-level LOD0/1/2 triangle-count cutoffs (Skyrim ~43 meshes — the content-bearing in-cell LOD) | in-cell LOD draw-count consumer |
  | `bs_sub_index` | `BSSubIndexTriShape` | dismemberment / locational-damage segment ids | dismemberment system |

  When a consumer feature lands, its slice must translate the parked field (data already captured — no parser change). Until then, this table is the bounded-gap record. The deeper passthrough inventory (NiTextureEffect, NiSwitchNode identity, BSFurnitureMarker/BSInvMarker, BSBound cell-path) lives in `nifal.md` §2 "Passthroughs" — cross-check against it; do not re-report a documented passthrough as a leak.
**Output**: `/tmp/audit/nifal/dim_4.md`

### Dimension 5: Particles — one shared overlay boundary folds every authored emitter override
**Entry points**:
- Parser (typed blocks): `crates/nif/src/blocks/particle.rs` — `NiPSysEmitter { params: EmitterBaseParams }` (box/sphere/cylinder/array/mesh variants via `read_emitter_base`/`read_volume_emitter_base`), `NiPSysEmitterCtlr { interpolator_ref }`, `NiPSysEmitterCtlrData` (legacy birth-rate), `NiPSysGrowFadeModifier { base_scale }`.
- Import: `crates/nif/src/import/walk/mod.rs::extract_emitter_params` → `ImportedEmitterParams` (surfaced on `ImportedParticleEmitter(+Flat)`); `extract_emitter_rate` (controller → `NiFloatInterpolator` constant / `NiFloatData` first key; legacy fallback `NiPSysEmitterCtlrData`).
- **The boundary**: `byroredux/src/systems/particle.rs::apply_emitter_overlays` — the **single overlay site** (#1513) that folds colour curve + base params + birth rate + force fields onto a name-heuristic preset in place. Called from BOTH `byroredux/src/scene/nif_loader.rs` (~line 526) and `byroredux/src/cell_loader/spawn.rs` (~line 413) via `crate::systems::apply_emitter_overlays` (re-exported by `pub(crate) use particle::*` in `systems.rs`). `apply_emitter_params` is the sub-helper it delegates to for the kinematic/lifetime/size subset — not itself the boundary.

**Checklist** (`no-fabrication` / `single-boundary`):
- `apply_emitter_overlays` is the single site overlaying authored data onto the preset. Both load paths route through it — a second inline overlay (colour, base params, rate, or force fields written field-by-field at a spawn site) is a `single-boundary` violation. This is the #1513 dedup: before it, the four overlays were copy-pasted inline at both sites.
- Authored **kinematic + lifetime** fields (speed, speed_variation, declination, declination_variation, life, life_variation) override the name-heuristic preset guesses (via `apply_emitter_params`).
- `initial_color` is **intentionally NOT applied** — colour stays owned by the `color_curve` override (white nif.xml default would wash out tuned presets). Flag a future change that starts applying it as a `no-fabrication` regression (in reverse).
- Spawn **rate** is authored: `extract_emitter_rate` follows `NiPSysEmitterCtlr.interpolator_ref`; the overlay sets `preset.rate` when present (FLT_MAX sentinel rejected — #1363/#1364). Legacy `NiParticleSystemController` content has no controller → keeps preset rate.
- Particle **size**: `apply_emitter_params` sets constant `start_size = end_size = initial_radius × base_scale` (`base_scale None → 1.0`). `base_scale` is essential (FNV oasis smoke `radius 50 × 0.15 = 7.5`; raw radius alone would be ~7× oversized). The grow→steady→fade bell shape canNOT map to the linear `start_size→end_size` — only the authored *magnitude* is translated (size-over-life curve is documented future work, not a leak).
- **Force fields** are Z-up→Y-up converted at overlay time (`convert_force_fields_zup_to_yup`, #984), not per-particle per-frame.

**Regression pins**:
- **Typed particle blocks**: the box/sphere/cylinder/array/mesh parsers read the base via `read_emitter_base` instead of skipping it (byte advancement unchanged, `Radius Variation` interleaved before `Life Span` per nif.xml). A parser reverting to skipping the base, or a per-game hardcoded layout without the BSVER gate, is the regression.
- Tooling: `crates/nif/examples/emitter_dump.rs` (`rate / radius / bscale / speed / spdVar / decl / declVar / life / lifeVar / initColor`).
**Output**: `/tmp/audit/nifal/dim_5.md`

### Dimension 6: Collision — every parsed bhk*Shape resolves to a CollisionShape (no silent drop)
**Entry points**:
- `crates/nif/src/import/collision.rs` — `resolve_shape` / `resolve_shape_inner` (recursive bhk-shape → `CollisionShape`); `CollisionShape` / `RigidBodyData` / `MotionType` are `byroredux_core::ecs::components::collision` types (the canonical tier). Havok→engine transform + per-game `havok_scale` (`scene.havok_scale`, ×7.0 TES4/FO3/FNV, ×69.99 Skyrim+/FO4) applied uniformly. Recursion depth is bounded (#1385) and non-finite floats guarded (#1409).

**Checklist** (`no-leak` — "parsed for byte-correctness then dropped at the unsupported-shape fallback" is the prime leak class):
- Every parsed `bhk*Shape` variant is handled (resolved to a `CollisionShape`, delegated, folded into a `Compound`, or explicitly parked with a documented reason) in `resolve_shape_inner`. **As of #1334 there are 16 shape arms** (count `downcast_ref::<Bhk*Shape>` arms, excluding the `BhkCollisionObject`/`BhkNPCollisionObject`/`BhkPCollisionObject`/`BhkRigidBody`/`BhkConstraint` arms which are objects, not shapes): `BhkSphereShape`, `BhkPlaneShape`, `BhkMultiSphereShape`, `BhkBoxShape`, `BhkCapsuleShape`, `BhkCylinderShape`, `BhkConvexVerticesShape`, `BhkMoppBvTreeShape`, `BhkConvexSweepShape`, `BhkListShape`, `BhkConvexListShape`, `BhkTransformShape`, `BhkNiTriStripsShape`, `BhkMeshShape`, `BhkPackedNiTriStripsShape`, `BhkCompressedMeshShape`. `BhkPlaneShape` (`#1334`) is the one deliberate exception — it returns `None` (no half-space `CollisionShape` variant yet; the trimesh fallback renders the correct ground surface anyway), documented at its arm, not a leak. A parsed `*Shape` block type with NO resolve arm at all (falls through to the unsupported-shape fallback) silently vanishes the authored collision — that is a leak finding. (Cross-check the live arm count against the parsed-shape set in `crates/nif/src/blocks/collision/` — the audit is the diff.)
- Havok→engine transform + `havok_scale` are applied uniformly inside `collision.rs` (Z-up→Y-up `(x, z, -y)`, quaternion swap). No consumer re-applies the scale.
- **`hkMotionType` byte collapses to the canonical `MotionType` at translate** (`#1652`, `extract_from_classic`): the raw Havok byte resolves to `Dynamic` / `Keyframed` / `Static` / `CharacterKinematic` per the canonical `hkMotionType` enum — the per-game raw byte is the raw-tier discriminator and must NOT leak past this decode. A downstream consumer inspecting the raw motion byte (instead of `RigidBodyData.motion_type`) is a `no-leak` violation; the old `4 => Keyframed / _ => Static` collapse is a `no-fabrication` regression (invents the wrong canonical value).

**Regression pins** (do NOT re-report these as open leaks — verify they stay resolved):
- **`BhkMultiSphereShape`** → `Compound` of `Ball` children at each sphere's scaled center (single centred sphere unwraps to a plain `Ball`). Pre-fix fell through the fallback (#9c6096aa).
- **`BhkConvexListShape`** → `Compound` of resolved convex sub-shapes (mirrors `BhkListShape`; FO3/FNV/Skyrim destructibles + debris). Pre-fix dropped silently (#9c6096aa).
- **`BhkConvexSweepShape`** (delegates to its inner `shape_ref`) and **`BhkMeshShape`** (resolves tri-strip data with per-axis scale) → added #1360/#1361. A revert is a `no-leak` regression.
- **Documented limitations (NOT leaks)** — confirm they stay documented in the table at the top of `import/collision.rs`, and do NOT report them as leaks:
  - `BhkNPCollisionObject` (FO4/FO76/Starfield Havok-serialised `BhkSystemBinary` blob) — decoder is a separate project; consumer falls back to `cell_loader/spawn.rs::synthesize_static_trimesh` for Architecture meshes.
  - `BhkPCollisionObject` phantoms (Skyrim+ trigger volumes) — need a `TriggerVolume` ECS path, not a rigid body. The `is::<BhkNPCollisionObject>` / `is::<BhkPCollisionObject>` discriminators let the trimesh fallback distinguish the two — verify they're intact.
**Output**: `/tmp/audit/nifal/dim_6.md`

### Dimension 7: Animation / controllers — single NIF→AnimationClip boundary (surveyed converged 2026-06-02)
**Entry points**:
- Parser/import: `crates/nif/src/anim/entry.rs::import_kf` (KF sequences) + `import_embedded_animations` (mesh-embedded controllers); both funnel through one set of `extract_*_channel_at` cores in `crates/nif/src/anim/`.
- **The boundary**: `byroredux/src/anim_convert.rs::convert_nif_clip` — the single NIF→core `AnimationClip` translation (multiple callers — `npc_spawn.rs`, `cell_loader/references.rs` + `partial.rs`, `scene.rs` + `scene/nif_loader.rs`, `systems/animation.rs` — all route through this one fn; multiple callers of one boundary is correct, not a single-boundary violation).
- Canonical type: ECS `AnimationClip` (`crates/core/src/animation/`).

**Checklist** (`no-leak` / `no-fabrication`):
- Every per-game variation is resolved at import: B-spline compressed interpolators (FO3/FNV + Skyrim+ — *not* Skyrim-only, per `feedback_bspline_not_skyrim_only`) sampled to linear keys; XYZ-Euler rotation keys composed to quaternions; TBC/Hermite tangents decoded; Z-up→Y-up once. The player/stack consumers must see only game-agnostic quaternion keys — no `Option`/era branch downstream.
- Text-key events wired: `NiControllerSequence.text_keys_ref` → `AnimationClip.text_keys` → `AnimationTextKeyEvents` ECS → scripting. Embedded controllers set `text_keys: Vec::new()` by design (mesh-local controllers carry no event keys) — verify that's a deliberate empty, not a drop.
- Intentionally **parked** (captured, no renderer consumer yet, NOT leaks): per-light **ambient** colour channels and **morph-weight** channels. Confirm they reach no canonical consumer.
**Output**: `/tmp/audit/nifal/dim_7.md`

### Dimension 8: Shader flags / texture sets / effect shaders — per-game vocabularies collapse at parse (surveyed converged 2026-06-02)
**Entry points**:
- `crates/nif/src/shader_flags.rs` — namespaced per-game flag vocabularies (`fo3nv_f1`, `skyrim_slsf1`, `fo4_slsf1`, + FO76/Starfield CRC32 arrays), the `ShaderFlags<'a>` typed view, `is_decal()` / `is_two_sided()`, and compile-time equivalence asserts (bits 26/27) guarding bit-meaning collisions.
- `MaterialInfo` (`crates/nif/src/import/material/`) — decal / two-sided read once per property type; `BSShaderTextureSet` slot→role mapping keyed on `shader_type`.
- `EmissiveSource::Effect` + `material_kind == 101` — `BSEffectShaderProperty` capture/route.

**Checklist** (`no-render-time-fallback` / `no-leak`):
- Per-game flag vocabularies are dispatched by **block type** (the wire format already discriminates the game), NOT by a runtime `if game ==`. Verify `triangle.frag` **and its `#include`d `include/*.glsl` headers** have **zero** `if game ==` branches — the renderer reads `material.is_decal` / `two_sided` with no per-game branch. A per-game branch leaking into the shader is the cardinal `no-render-time-fallback`/leak violation for this dimension.
- All 9 `BSLightingShaderProperty` shader-type variants forward their trailing data (SkinTint/HairTint/Parallax/MultiLayer/Eye/Sparkle — the pre-#343 8-of-9 drop is closed). A variant dropping its trailing data is a regression.
- `BSEffectShaderProperty` captured + routed (EFFECT_* flags, `material_kind == 101`). The one *deferred* item is the `base_color_scale` diffuse-tint-vs-emissive render path — tagged via `EmissiveSource::Effect`, not dropped (don't re-report as a leak).
- **FO4 render-affecting flags reach `MaterialInfo`** (`#1592`, `import/material/walker.rs`): `Model_Space_Normals` (F4SF1 bit 12) + `Alpha_Test` (F4SF2 bit 25) are ORed into `MaterialInfo` (plus the FO76+ `MODELSPACENORMALS` CRC for `bsver >= 132`) so the per-game flag vocabulary collapses into the canonical fields (`model_space_normals` / `alpha_test`), not a render-time `if game == fo4`. The NIF flag is a strictly lower-priority source than the later BGSM merge (which OR-upgrades). Regression = the walker parsing the F4SF pair but dropping these bits (a `no-leak` violation — the data is read fine but never reaches the canonical material).
**Output**: `/tmp/audit/nifal/dim_8.md`

### Dimension 9: Translation-completeness signal + cross-cutting tier invariants
**Entry points**:
- `crates/nif/tests/translation_completeness.rs` — `cross_game_translation_completeness` (`#[ignore]`-gated; run with `cargo test -p byroredux-nif --test translation_completeness -- --ignored`), `collect_stats`, `MaterialStats::record/print_row`. Per-game (`Oblivion`/`FNV`/`Skyrim`/…) aggregate fill-rate over the canonical `Material` slots.

**Checklist** (the four tier invariants stated up top, applied across every dimension):
- **single-boundary**: each category that needs one declares its boundary, not scattered construction — Material ✓ `translate_material`; Particles ✓ `apply_emitter_overlays`; Animation ✓ `convert_nif_clip`; EXAL exterior ✓ `env_translate.rs::translate_*`; Nodes ✗ by design (Dim 4). New categories must declare a boundary.
- **no-fabrication**: the emissive no-op (Dim 1) and particle colour/size-curve deferrals (Dim 5) are the canonical "measured, then deliberately NOT normalized" examples. Any new constant must cite a measurement or source.
- **no-leak**: no `Option`/raw discriminator on a canonical type reaches a consumer that re-resolves it.
- **no-render-time-fallback**: no classification deferred to a per-draw heuristic (the deleted `classify_pbr` / render-side glass heuristics are the cautionary tale; `triangle.frag` `if game ==` count must be zero — Dim 8).
- The completeness harness is the **per-game coverage signal**: a category that converges on FNV but drops to ~0 fill on Starfield is an unverified-game leak even if no single-game audit flagged it. Treat large per-game fill-rate divergence (in `print_row` output) as a lead, not gospel — verify the underlying extractor.
**Output**: `/tmp/audit/nifal/dim_9.md`

## Phase 3: Merge

1. Read all `/tmp/audit/nifal/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_NIFAL_<TODAY>.md` (YYYY-MM-DD) with structure:
   - **Executive Summary** — per-category convergence status (converged / triaged /
     pending) vs the spec §2 leak inventory; count of single-boundary / no-fabrication
     / no-leak / no-render-time-fallback violations found.
   - **Per-Category Tier Matrix** — table of category × tier-invariant (single-boundary,
     no-fabrication, no-leak, no-render-time-fallback) marked pass / fail / N-A, with the
     boundary fn cited for each.
   - **Findings** — grouped by severity (apply the NIFAL severity rows in
     `_audit-severity.md`), using the base finding format plus the Extra Per-Finding
     Fields above.
   - **Documented-limitation ledger** — restate the parked-not-leak items (node/mesh
     passthroughs, FO4+ NP blob, phantoms, size-over-life curve, ambient/morph anim
     channels) so they are not re-reported next sweep.
3. Remove cross-dimension duplicates.

Run `.claude/commands/_audit-validate.sh` before finalizing (backticked paths must
resolve against the live tree — Path-Reference Convention in `_audit-common.md`).

Suggest: `/audit-publish docs/audits/AUDIT_NIFAL_<TODAY>.md`
