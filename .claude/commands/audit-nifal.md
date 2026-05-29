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
See `.claude/commands/_audit-severity.md` for the severity scale.

**Scope vs `/audit-nif`**: `/audit-nif` owns the *parse* side (block field
correctness, version handling, stream position, coverage). NIFAL owns the
*translate* side (does each parsed category reach one canonical representation
through one boundary, with no leak/fabrication/fallback?). When a finding is "the
bytes are read wrong," it belongs to `/audit-nif`; when it is "the bytes are read
fine but the data is dropped, duplicated, or resolved per-game downstream," it
belongs here.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,6`). Default: all 8.
- `--game <name>`: Focus on a specific variant: `fnv`, `fo3`, `skyrim`, `oblivion`,
  `fo4`, `fo76`, `starfield`. Default: all detected (from `_audit-common.md` game
  data locations).

## Extra Per-Finding Fields

- **Dimension**: Material | Geometry/Transform | Skinning/Lights | Nodes | Particles | Collision | Completeness
- **Tier Violated**: which tier rule broke — `single-boundary` (duplicate construction
  site) | `no-fabrication` (invented value / guessed normalization) | `no-leak`
  (`Option`/raw discriminator reaches a canonical consumer) | `no-render-time-fallback`
  (classification deferred to a per-draw heuristic) | `parked-not-leak` (verify a
  "deferred" field is genuinely unconsumed, not a silent drop)
- **Game Affected**: which variant(s) the divergence manifests on

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/nifal`.
3. Fetch dedup baseline:
   `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`,
   and scan `docs/audits/` for prior `AUDIT_NIFAL_*` / `AUDIT_NIF_*` reports.
4. Read `docs/engine/nifal.md` (the spec) and `docs/engine/material-abstraction.md`
   (the material-slice predecessor; note its §2 "Leak A"/"Leak B" are recorded
   **closed** in `nifal.md` §3 — do not re-report them as open).
5. Check which game data directories exist.

## Phase 2: Launch Dimension Agents

### Dimension 1: Material — the reference realisation (single boundary, no PBR fallback, glass-once)
**Entry points**:
- `byroredux/src/material_translate.rs::translate_material(mesh: &ImportedMesh, paths: ResolvedPaths, extra_material_flags: u32) -> Material` — the **single** `ImportedMesh → Material` boundary (`ResolvedPaths` struct also defined in this file).
- Callers (MUST be exactly two, both routing through the boundary): `byroredux/src/scene/nif_loader.rs` (loose-NIF path) and `byroredux/src/cell_loader/spawn.rs` (cell path).
- `crates/core/src/ecs/components/material.rs` — the canonical `Material`; `Material::resolve_pbr()`, `classify_pbr_keyword()`, `EmissiveSource` enum.
- `byroredux/src/helpers.rs::classify_glass_into_material` — the single glass classifier.
- `byroredux/src/render/static_meshes.rs` — the renderer consumer (reads `m.metalness` / `m.roughness` directly).

**Checklist**:
- `translate_material` is the ONLY construction site that fills a `Material` from an `ImportedMesh`. Any second site building a `Material` field-by-field from import data is a `single-boundary` violation (the pre-converged state had two ~110-line literals in `spawn.rs` + `nif_loader.rs` — regression pattern).
- `Material.metalness` / `Material.roughness` are plain `f32` (resolved, clamped `metalness ∈ [0,1]`, `roughness ∈ [0.04,1]`). No `metalness_override: Option<f32>` / `roughness_override: Option<f32>` field on the canonical `Material`, and no per-draw `classify_pbr` fallback in the renderer (`no-leak` + `no-render-time-fallback`).
- `resolve_pbr()` fills `NaN` sentinels from `classify_pbr_keyword` and clamps; it is idempotent and only fills missing slots (don't let it overwrite an authored BGSM/BGEM override).
- Glass is classified **once**, alpha-aware, via `classify_glass_into_material`, AFTER the PBR resolve so the forced glass roughness wins. No glass heuristic at render time (`docs/engine/material-abstraction.md` §2 "Leak A" is closed — confirm it stays deleted).
- `material_kind: u32` is intentionally kept as-is (it is the GPU shader-dispatch contract — the `material_kind == N` ladder in `triangle.frag`). Do NOT flag its `u32`-ness as a leak. **Future-slice invariant**: any `SurfaceClass` enum MUST lower to the exact `triangle.frag` ladder (drift risk vs the shader — a shader-adjacent change).
- `effect_shader_flags` packs the union of BSEffect SLSF bits + BGSM v>2 bits + the caller's `extra_material_flags` (REFR-overlay model-space-normals on the cell path; `0` on the loose path).

**Regression pins (this session, 2026-05-28)**:
- **material_translate dedup**: the two duplicate `Material` construction literals were collapsed into `translate_material`. A field added in one load path that doesn't go through the boundary can silently diverge the two paths — the regression this dedup prevents.
- **Emissive scale = no-op** (spec §4): `Material.emissive_mult` is fed by three `EmissiveSource` variants (`Material` legacy / `Lighting` Skyrim+ / `Effect` FO4+). All three were **measured** across Oblivion/FNV/Skyrim/FO4 and already share a ~1.0 scale — **no normalization is applied or wanted**. A future "emissive normalization constant" is a `no-fabrication` violation (inventing a correction for a divergence the ground truth shows does not exist). The one genuine distinction (`BSEffectShaderProperty.base_color_scale` is a diffuse-tint, not emissive) is captured by the `EmissiveSource::Effect` discriminator and left for a future BSEffect render path. Open question Q2 in `material-abstraction.md` is resolved no-op — do not re-open it. Tooling: `crates/nif/examples/material_dump.rs` (the `emisM` + `emSrc` columns).
**Output**: `/tmp/audit/nifal/dim_1.md`

### Dimension 2: Geometry / Transform — the cleanest category (the template the others match)
**Entry points**:
- `crates/nif/src/import/coord.rs` — Z-up (Gamebryo) → Y-up (renderer): `zup_point_to_yup`, `zup_matrix_to_yup_quat` (thin wrappers over `byroredux_core::math::coord`).
- `crates/nif/src/import/mesh/tangent.rs` — `synthesize_tangents` / `synthesize_tangents_yup` (Mikkelsen synthesis fallback).
- `crates/nif/src/import/mesh/` per-game extractors — `ni_tri_shape.rs`, `bs_tri_shape.rs`, `bs_geometry.rs`; each sets `ImportedMesh.local_bound_radius` (field on `crates/nif/src/import/types.rs`).
- `crates/nif/src/rotation.rs` — degenerate-rotation SVD repair: `is_degenerate_rotation`, `repair_rotation_svd_or_identity` (done ONCE at parse time — see #277; NOTE the spec's `transform.rs` attribution drifted, the repair lives in `rotation.rs`).
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
- Skinning: `crates/nif/src/import/mesh/skin.rs` — `ImportedSkin` (`crates/nif/src/import/types.rs`), `global_skin_transform`, the #613 partition-local→global bone-index remap (done at extraction).
- Lights: `crates/nif/src/import/types.rs::LightKind` (`Ambient`/`Directional`/`Point`/`Spot`) + `ImportedLight.radius`; populated in `crates/nif/src/import/walk/mod.rs`.

**Checklist**:
- **Skinning** (`no-leak` / converged): `ImportedSkin` emits **global** bone indices — partition-local remap done at extraction (#613 / SK-D1-01: pre-#613 silently aliased every vertex past partition 0). The defensive u16-range warning (`skin.rs` ~line 148) must stay. `global_skin_transform` carried through. Palette skinning is game-agnostic downstream — no consumer should re-derive partition layout.
- **Lights** (`no-leak` / converged): `ImportedLight` resolves to the `LightKind` enum with a derived effective `radius` (Bethesda units, from attenuation). The renderer must NEVER inspect the source NIF block type (NiAmbientLight / NiDirectionalLight / NiPointLight / NiSpotLight) — that is the raw-tier discriminator collapsed at translate. A downstream `match` on source block type is a leak.
**Output**: `/tmp/audit/nifal/dim_3.md`

### Dimension 4: Nodes — the four raw-tier-parked passthroughs (verify parked, not silently dropped)
**Entry points**:
- `crates/nif/src/import/types.rs` — `ImportedNode` fields: `bs_value_node: Option<BsValueNodeData>`, `bs_ordered_node: Option<BsOrderedNodeData>`, `tree_bones: Option<TreeBones>`, `range_kind: Option<BsRangeKind>`.
- Live (canonical) node data consumers: spawn sites in `byroredux/src/scene/nif_loader.rs` + `byroredux/src/cell_loader/spawn.rs` (`name`, `flags`→`SceneFlags`, `collision`→`CollisionShape`/`RigidBodyData`, `billboard_mode`→`Billboard`).

**Checklist**:
- The live node data (name, flags, collision, billboard_mode) IS consumed at the spawn sites — confirm no canonical node field is dropped.
- The `ImportedNode → ECS` step is deliberately NOT a single `translate_node` boundary: the two load paths handle nodes structurally differently (loose-NIF spawns the full NiNode hierarchy as entities; cell loader uses a flattened placement-root). Do NOT flag the absence of one boundary as a `single-boundary` violation for nodes — it is documented (spec §2 Nodes).
- The four fields below are **raw-tier-parked with deferred translation** — verify (per-game) they have **zero canonical ECS consumers** (`parked-not-leak`). They are NOT leaks (they sit on the raw `ImportedNode`, which the tier model permits to carry per-game data, and reach no canonical component). Each is blocked on a not-yet-existing consumer feature. If you find ANY of them now feeding a canonical ECS component without a translate step, THAT is a leak finding:

  | Field | Source block | Authored data | Blocked on |
  |---|---|---|---|
  | `bs_value_node` | `BSValueNode` | LOD-distance / billboard-mode hint (FO3/FNV) | M35 LOD selector |
  | `bs_ordered_node` | `BSOrderedNode` | alpha-sort bound + draw-order hint | `RenderOrderHint` + `build_render_data` sort key |
  | `tree_bones` | `BSTreeNode` | SpeedTree branch/trunk bone names | SpeedTree wind/bend sim |
  | `range_kind` | `BSRangeNode`/`BSDamageStage`/`BSBlastNode`/`BSDebrisNode` | destructible/blast/debris discriminator | destructible-switching / blast / debris systems |

  When a consumer feature lands, its slice must translate the parked field (the data is already captured — no parser change needed). Until then, this table is the bounded-gap record.
**Output**: `/tmp/audit/nifal/dim_4.md`

### Dimension 5: Particles — authored base params override the name-heuristic preset (one shared apply helper)
**Entry points**:
- Parser (typed blocks): `crates/nif/src/blocks/particle.rs` — `NiPSysEmitter { params: EmitterBaseParams }` (+ `parse_box_emitter`/volume variants via `read_emitter_base`/`read_volume_emitter_base`), `NiPSysEmitterCtlr { interpolator_ref }`, `NiPSysEmitterCtlrData` (legacy birth-rate), `NiPSysGrowFadeModifier { base_scale }` (`parse_grow_fade_modifier`).
- Import: `crates/nif/src/import/walk/mod.rs::extract_emitter_params` → `ImportedEmitterParams` (on `crates/nif/src/import/types.rs`, surfaced on `ImportedParticleEmitter(+Flat)`); `extract_emitter_rate` (controller → `NiFloatInterpolator` constant / `NiFloatData` first key; legacy fallback `NiPSysEmitterCtlrData`).
- Translate / apply: `byroredux/src/systems/particle.rs::apply_emitter_params` — the **one shared helper**, called from BOTH `byroredux/src/scene/nif_loader.rs` (~line 541) and `byroredux/src/cell_loader/spawn.rs` (~line 419) via `crate::systems::apply_emitter_params`.

**Checklist** (`no-fabrication` / `single-boundary`):
- `apply_emitter_params` is the single site that overlays authored params onto the preset. Both load paths route through it — a second inline overlay is a `single-boundary` violation.
- Authored **kinematic + lifetime** fields (speed, speed_variation, declination, declination_variation, life, life_variation) override the name-heuristic preset guesses where genuinely authored.
- `initial_color` (white nif.xml default) is **intentionally NOT applied** — colour stays owned by the `color_curve` override. Applying the default white would wash out tuned presets — flag it as a regression (a `no-fabrication` violation in reverse) if a future change starts applying it.
- Spawn **rate** is authored: `extract_emitter_rate` follows `NiPSysEmitterCtlr.interpolator_ref`; the translate sets `preset.rate` when present (FLT_MAX sentinel rejected). Legacy `NiParticleSystemController` content has no controller → keeps preset rate.
- Particle **size**: the translate sets constant `start_size = end_size = initial_radius × base_scale` (`base_scale None → 1.0`). `base_scale` is essential (FNV oasis smoke `radius 50 × 0.15 = 7.5`; raw radius alone would be ~7× oversized). The grow→steady→fade bell shape canNOT map to the linear `start_size→end_size` — only the authored *magnitude* is translated (size-over-life curve is documented future work, not a leak).

**Regression pins (2026-05-28)**:
- **Typed particle blocks**: `NiPSysEmitter` / `NiPSysEmitterCtlr` / `NiPSysEmitterCtlrData` / `NiPSysGrowFadeModifier` are typed blocks carrying decoded params (the box/sphere/cylinder/array/mesh parsers read the base via `read_emitter_base` instead of skipping it; byte advancement unchanged, `Radius Variation` interleaved before `Life Span` per nif.xml). A future parser that reverts to skipping the base, or a per-game hardcoded layout without the BSVER gate, is the regression.
- Tooling: `crates/nif/examples/emitter_dump.rs` (`rate / radius / bscale / speed / declination / life / initColor`).
**Output**: `/tmp/audit/nifal/dim_5.md`

### Dimension 6: Collision — every parsed bhk*Shape resolves to a CollisionShape (no silent drop)
**Entry points**:
- `crates/nif/src/import/collision.rs` — `resolve_shape` / `resolve_shape_inner` (recursive bhk-shape → `CollisionShape`); `CollisionShape` / `RigidBodyData` / `MotionType` are `byroredux_core::ecs::components::collision` types (the canonical tier). Havok→engine transform + per-game `havok_scale` (`scene.havok_scale`, ×7.0 TES4/FO3/FNV, ×69.99 Skyrim+) applied uniformly.

**Checklist** (`no-leak` — the "parsed for byte-correctness then dropped at the unsupported-shape fallback" pattern is the prime leak class here):
- Every parsed `bhk*Shape` variant resolves to a `CollisionShape`. As of 2026-05-28 there are **13** `downcast_ref::<Bhk*Shape>` arms in `resolve_shape_inner`: `BhkSphereShape`, `BhkMultiSphereShape`, `BhkBoxShape`, `BhkCapsuleShape`, `BhkCylinderShape`, `BhkConvexVerticesShape`, `BhkMoppBvTreeShape`, `BhkListShape`, `BhkConvexListShape`, `BhkTransformShape`, `BhkNiTriStripsShape`, `BhkPackedNiTriStripsShape`, `BhkCompressedMeshShape`. A parsed `*Shape` block type with NO resolve arm (falls through to the unsupported-shape fallback) silently vanishes the authored collision — that is a leak finding.
- Havok→engine transform + `havok_scale` are applied uniformly inside `collision.rs` (Z-up→Y-up `(x, z, -y)`, quaternion swap). No consumer re-applies the scale.

**Regression pins (this session, 2026-05-28)**:
- **`BhkMultiSphereShape`** → now a `Compound` of `Ball` children at each sphere's scaled center (single centred sphere unwraps to a plain `Ball`). Pre-fix it fell through the unsupported-shape fallback. A revert is a `no-leak` regression.
- **`BhkConvexListShape`** → now a `Compound` of resolved convex sub-shapes (mirrors `BhkListShape`; FO3/FNV/Skyrim destructibles + debris). Pre-fix dropped silently.
- **Documented limitations (NOT leaks)** — confirm they stay documented in the table at the top of `import/collision.rs`, and do NOT report them as leaks:
  - `BhkNPCollisionObject` (FO4/FO76/Starfield Havok-serialised blob) — decoder is a separate project; consumer falls back to `cell_loader/spawn.rs::synthesize_static_trimesh` for Architecture meshes.
  - `BhkPCollisionObject` phantoms (Skyrim+ trigger volumes) — need a `TriggerVolume` ECS path, not a rigid body. A small bookkeeping discriminator (`is::<BhkNPCollisionObject>` / `is::<BhkPCollisionObject>` in `collision.rs`) lets the trimesh fallback distinguish the two — verify it's intact.
**Output**: `/tmp/audit/nifal/dim_6.md`

### Dimension 7: Translation-completeness signal + the cross-cutting tier invariants
**Entry points**:
- `crates/nif/tests/translation_completeness.rs` — `cross_game_translation_completeness` (`#[ignore]`-gated; run with `cargo test -p byroredux-nif --test translation_completeness -- --ignored`), `collect_stats`, `MaterialStats::record/print_row`. Per-game (`Oblivion`/`FNV`/`Skyrim`/…) aggregate fill-rate over the canonical `Material` slots.

**Checklist** (cross-cutting — these are the NIFAL invariants stated in `docs/engine/nifal.md` §1):
- **single-boundary**: exactly one `translate()` site per category that needs one (Material ✓ `translate_material`; Particles ✓ `apply_emitter_params`; Nodes ✗ by design, see Dim 4). New categories must declare their boundary, not scatter construction.
- **no-fabrication**: no invented values / guessed normalization. The emissive no-op (Dim 1) and the particle color/size-curve deferrals (Dim 5) are the canonical examples of "measured, then deliberately NOT normalized." Any new constant must cite a measurement or source (`feedback_no_guessing` policy).
- **no-leak**: no `Option` "resolve-later" field or raw enum discriminator on a canonical type reaches a consumer that has to re-resolve it. (Raw-tier `Imported*` carrying `Option`s is fine — the leak is when it crosses into the canonical/consumer tier.)
- **no-render-time-fallback**: no classification deferred to a per-draw heuristic (the deleted `classify_pbr` / render-side glass heuristics are the cautionary tale).
- The completeness harness is the **per-game coverage signal**: a category that converges on FNV but drops to ~0 fill on Starfield is an unverified-game leak even if no single-game audit flagged it. Treat large per-game fill-rate divergence (in the `print_row` output) as a lead, not gospel — verify the underlying extractor.
**Output**: `/tmp/audit/nifal/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/nifal/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_NIFAL_<TODAY>.md` (YYYY-MM-DD) with structure:
   - **Executive Summary** — per-category convergence status (converged / triaged /
     pending) vs the spec §2 leak inventory; count of single-boundary / no-fabrication
     / no-leak / no-render-time-fallback violations found.
   - **Per-Category Tier Matrix** — table of category × tier-invariant (single-boundary,
     no-fabrication, no-leak, no-render-time-fallback) marked pass / fail / N-A, with the
     boundary fn cited for each.
   - **Findings** — grouped by severity, using the base finding format plus the Extra
     Per-Finding Fields above.
   - **Documented-limitation ledger** — restate the parked-not-leak items (node
     passthroughs, FO4+ NP blob, phantoms, size-over-life curve) so they are not
     re-reported next sweep.
3. Remove cross-dimension duplicates.

Run `.claude/commands/_audit-validate.sh` before finalizing (backticked paths must
resolve against the live tree — Path-Reference Convention in `_audit-common.md`).

Suggest: `/audit-publish docs/audits/AUDIT_NIFAL_<TODAY>.md`
