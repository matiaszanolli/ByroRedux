# NIFAL Audit — Canonical Translation Layer — 2026-07-02

Deep audit of **NIFAL** (the NIF Abstraction Layer; spec: `docs/engine/nifal.md`).
Scope: the *translate* side — does each parsed NIF data category reach one
canonical, game-agnostic representation through a single explicit `translate()`
boundary, with no leak / fabrication / render-time fallback? (Parse-side
correctness belongs to `/audit-nif`.)

All 9 dimensions were verified against the live tree @ HEAD `1b4e8e84`.
Dedup baseline: `/tmp/audit/issues.json` (21 OPEN) + prior
`docs/audits/AUDIT_NIFAL_2026-06-28.md` (1 NEW LOW: D6-01), plus
`AUDIT_NIFAL_2026-06-23.md`, `AUDIT_NIFAL_2026-06-14.md`,
`AUDIT_NIFAL_2026-06-13.md`. Game data present for **all 7 titles** (Oblivion /
FO3 / FNV / Skyrim SE / FO4 / FO76 / Starfield).

---

## Executive Summary

**Severity tally: 0 CRITICAL · 0 HIGH · 0 MEDIUM · 0 LOW. 0 regressions. 0 NEW findings.**

Every per-category convergence claim in `nifal.md` §2 was re-verified against the
current code and holds. The four tier invariants — **single-boundary**,
**no-fabrication**, **no-leak**, **no-render-time-fallback** — pass on every
category.

Notably, the single OPEN finding from the previous sweep (**D6-01**, LOW —
`bhkPackedNiTriStripsShape` per-axis Scale parsed but dropped) is now **CLOSED**
by commit `2b780a2c` (Fix #1777): the shape's per-axis `Scale` is folded at the
collision-translate boundary (`per_axis_scale(&s.scale)` applied on top of
`havok_scale`), from the parsed block value, not a fabricated constant.

The seven commits that landed on the NIFAL surface since the 2026-06-28 sweep
were each re-verified clean against the current code:

| Commit | Dimension | Verdict |
|---|---|---|
| `2b780a2c` #1777 — fold `bhkPackedNiTriStripsShape` per-axis Scale at collision translate | Collision | **clean** — closes prior D6-01; per-axis scale is the parsed block value, applied on top of `havok_scale` in `resolve_shape_inner`; no fabricated constant |
| `61b8cf7f` #1333 — retain `NiParticleSystem` local transform, compose into both emitter walkers | Particles / Geometry | **clean** — recovers previously-dropped authored local TRS (no fabrication); composes host-world × block-local symmetrically in both walkers (flat/cell bakes into `local_position`; hierarchical/loose defers into a child `Transform`); vanilla authors a zero offset, so behaviourally equivalent for a point spawn origin |
| `6a6b89b9` #1672/#1673 — split `parse_nif_with_options` + lift `import_embedded_animations` nested fns | Geometry / Animation | **clean** — pure structural refactor; no logic change, no new construction site, no divergence between the two paths |
| `fe1d5b1b` #1753/#1760 — route stray axis-swaps through the `coord` helper + drop two dead mesh builders | Geometry | **clean** — closes prior #1753/TD2-005 (inline `(x, z, -y)` literals in `tangent.rs` now route through the single `coord` SoT); dead-code removal only |
| `05f3342b` #1775 — forward `NiPSysEmitter.radius_variation` as size jitter | Particles | **clean** — authored value × `base_scale`, lands in plain-`f32` `start_size_variation`, both spawn paths get it via the shared sub-helper |
| `73592e33` #1771 — authored emitter rate of exactly 0.0 → preset-fallback | Particles | **clean** — in the shared `sane()` filter (`0.0 < r < 3.0e38`); same sentinel discipline as #1363/#1364 |
| `f9cc691b` #1778 — correct WATR.DATA colour offsets on 186-byte FO3/FNV records | (ESM/WATR, not NIFAL) | out of NIFAL scope — WATR is an ESM record, not a NIF category |

### Convergence status vs spec §2 leak inventory

| Category | Spec status | Verified 2026-07-02 |
|---|---|---|
| Materials | converged | ✅ confirmed — single boundary (`translate_material`), exactly 2 production callers (`scene/nif_loader.rs:828`, `cell_loader/spawn.rs:880`), plain-`f32` resolved PBR clamped `[0,1]`/`[0.04,1]` in `resolve_pbr`, NaN-only fills that never overwrite an authored override, glass-once alpha-aware after `resolve_pbr`, three-way flag union, emissive pass-through with no normalization constant |
| Geometry / transform | converged | ✅ confirmed — #1753/TD2-005 closed (`tangent.rs` axis-swaps now through the `coord` SoT); SVD-once at parse; Z-up→Y-up at import; per-game vertex decode converges to one renderer-space `Vec<[f32;3]>`+`Vec<u32>` |
| Skinning | converged | ✅ confirmed — `skin.rs` byte-identical to baseline; #613 global bone indices, u16 guard, `global_skin_transform` carried; no partition re-derivation downstream |
| Lights | converged | ✅ confirmed — `imported_light_from_base` → `LightKind`; no renderer/spawn `match` on source NIF block type |
| Nodes | triaged | ✅ confirmed — all parked fields (`bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`) have **zero** canonical ECS consumers (only tests + `None`/`match` initializers) — parked-not-leak |
| Particles | converged | ✅ confirmed — single overlay boundary `apply_emitter_overlays`, exactly 2 production callers; `initial_color` correctly unapplied; #1333 + #1775 + #1771 clean |
| Collision | audited | ✅ confirmed — 15 shape-resolve arms + documented `BhkPlaneShape` None-drop (#1334) + phantom drops; every dispatched `bhk*Shape` key maps to a resolve arm (CI guard `every_dispatched_bhk_shape_has_resolve_arm`); **D6-01 CLOSED by #1777**; `hkMotionType` collapses to canonical `MotionType` |
| Animation / controllers | converged | ✅ confirmed — one `convert_nif_clip` boundary (multiple legit callers); embedded controllers `text_keys: Vec::new()` by design; #1672/#1673 refactor clean |
| Shader flags / effects | converged | ✅ confirmed — **zero** `if game ==` in `crates/renderer/shaders/` incl. `include/*.glsl`; FO4 `model_space_normals`/`alpha_test` bits reach `MaterialInfo` (#1592 intact) |

The 2026-06-13 D7-NEW-01 (miscalibrated completeness floors) remains addressed:
the harness uses conservative documented thresholds (`>= 60.0` texture-path,
`>= 40.0` tangents — `crates/nif/tests/translation_completeness.rs:347,352`) and
stays `#[ignore]`-gated behind real game data.

---

## Per-Category Tier Matrix

Legend: SB = single-boundary · NF = no-fabrication · NL = no-leak ·
NRTF = no-render-time-fallback. `N-A` = the invariant does not apply to that
category (e.g. Nodes have no single boundary by design; Lights/Skinning have no
render-time classification to leak).

| Category | SB | NF | NL | NRTF | Boundary fn |
|---|---|---|---|---|---|
| Materials | PASS | PASS | PASS | PASS | `byroredux/src/material_translate.rs::translate_material` |
| Geometry / transform | PASS | PASS | PASS | PASS | `crates/nif/src/import/{coord,transform,rotation,mesh/*}.rs` (parse-time) |
| Skinning | PASS | PASS | PASS | N-A | `crates/nif/src/import/mesh/skin.rs` (#613 remap at extraction) |
| Lights | PASS | PASS | PASS | PASS | `crates/nif/src/import/walk/mod.rs::imported_light_from_base` → `LightKind` |
| Nodes | N-A (by design) | PASS | PASS | PASS | *no single boundary* (two structurally-different spawn paths — spec §2 Nodes) |
| Particles | PASS | PASS | PASS | PASS | `byroredux/src/systems/particle.rs::apply_emitter_overlays` |
| Collision | PASS | PASS | PASS | PASS | `crates/nif/src/import/collision.rs::resolve_shape` / `resolve_shape_inner` |
| Animation | PASS | PASS | PASS | PASS | `byroredux/src/anim_convert.rs::convert_nif_clip` |
| Shader flags / effects | PASS | PASS | PASS | PASS | `crates/nif/src/shader_flags.rs` + `import/material/walker.rs` (block-type dispatch) |

---

## Findings

**None.** No CRITICAL / HIGH / MEDIUM / LOW findings this sweep. The single OPEN
finding from the prior sweep (D6-01, LOW) is verified CLOSED by #1777 and is not
re-reported.

---

## Dimension-by-Dimension Verification Notes

### Dimension 1 — Material (PASS)
- `translate_material` is the sole production `ImportedMesh → Material` boundary;
  exactly two callers (`scene/nif_loader.rs:828` loose, `cell_loader/spawn.rs:880`
  cell). Other `Material { … }` literals are `--cornell` synthetic-scene or
  `#[cfg(test)]` fixtures — not import-path construction sites.
- `metalness`/`roughness` are plain `f32`, clamped `[0,1]`/`[0.04,1]` in
  `resolve_pbr` (`material.rs:655-656`); the classifier arm only fires on NaN
  sentinels and never overwrites an authored BGSM/BGEM value (regression tests
  `resolve_pbr_preserves_upstream_translator_values`,
  `resolve_pbr_fills_only_missing_slot` green).
- Glass classified once, alpha-aware, after `resolve_pbr`
  (`classify_glass_into_material`); kinds `>= 100` never demoted, conductors
  (`metalness >= 0.3`)/decals gated out; zero render-time glass heuristic
  (`triangle.frag:1009 isGlass` is a technique gate on an already-resolved
  `materialKind`, not a classification).
- `effect_shader_flags` is the clean three-way OR (BSEffect SLSF ∪ BGSM v>2 ∪
  caller extra). `emissive_mult` passes through verbatim; no
  `EMISSIVE_NORM`/normalization constant exists (spec §4 no-op stands).

### Dimension 2 — Geometry / Transform (PASS)
- Only `fe1d5b1b` touches this territory; it closes #1753/TD2-005 (the inline
  `(x, z, -y)` axis-swap literals in `tangent.rs` now route through the `coord`
  SoT). SVD rotation repair fires once at parse; `compose_transforms` assumes
  sanitized rotations. Per-game vertex decode converges to one renderer-space
  positions/indices array; consumer (`MeshRegistry::upload`) is format-agnostic.

### Dimension 3 — Skinning & Lights (PASS)
- `import/mesh/skin.rs` byte-identical to baseline — #613 global bone-index remap
  at extraction, u16-range guard, `global_skin_transform` carried; palette
  skinning game-agnostic downstream.
- `ImportedLight` → `LightKind` enum + derived radius; no renderer/spawn `match`
  on the source NIF light block type.

### Dimension 4 — Nodes (PASS, parked-not-leak)
- All parked node/mesh fields (`bs_value_node`, `bs_ordered_node`, `tree_bones`,
  `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`) grep to **zero**
  canonical ECS consumers — only `#[cfg(test)]` references and field-initializer
  sites (`: None` / `: match &shape.kind`) in extractors. No parked field feeds a
  canonical component. The absence of a single `translate_node` boundary is by
  design (two structurally-different spawn paths, spec §2 Nodes).

### Dimension 5 — Particles (PASS)
- `apply_emitter_overlays` is the single overlay site; exactly two production
  callers (`scene/nif_loader.rs:527`, `cell_loader/spawn.rs:436`) — the two
  in-file call sites (`particle.rs:521,548`) are `#[test]` cases. `initial_color`
  correctly unapplied (colour stays with the `color_curve` override).
- #1333 (`NiParticleSystem` local transform) recovers previously-dropped authored
  TRS and composes it symmetrically in both walkers — no fabrication, no
  divergence for a point spawn origin. #1775 (radius→size jitter) and #1771
  (0.0-rate preset fallback) hold. Typed emitter parsers read the base
  (BSVER/`V10_4_0_1`-gated), not skip.

### Dimension 6 — Collision (PASS, D6-01 closed)
- 15 `downcast_ref::<Bhk*Shape>` resolve arms (matching the spec's #1360/#1361
  count), plus a documented `BhkPlaneShape` deliberate `None`-drop (#1334 →
  trimesh fallback) and the two phantom drops (`BhkSimpleShapePhantom` /
  `BhkAabbPhantom`, #1363 — trigger volumes, not solid colliders). Every
  dispatched `bhk*Shape` key maps to a resolve arm, enforced by the CI guard
  `every_dispatched_bhk_shape_has_resolve_arm`.
- **D6-01 CLOSED** by `2b780a2c` (#1777): `bhkPackedNiTriStripsShape` per-axis
  `Scale` now folded (`per_axis_scale(&s.scale)`) on top of `havok_scale` in
  `resolve_shape_inner` — the parsed block value, no fabricated constant.
- `havok_scale` applied uniformly inside `collision.rs`; `bhkNiTriStripsShape`
  correctly excluded (render-mesh game units, #1744). `hkMotionType` byte
  collapses to canonical `MotionType` (`havok_motion_type`, #1652) per the nif.xml
  enum — no downstream raw-byte inspection.

### Dimension 7 — Animation / Controllers (PASS)
- `convert_nif_clip` is the single NIF→`AnimationClip` boundary; its multiple
  callers (`npc_spawn.rs`, `cell_loader/references.rs`+`partial.rs`, `scene.rs`+
  `scene/nif_loader.rs`, `systems/animation.rs`) all route through the one fn
  (multiple callers of one boundary is correct). Embedded controllers set
  `text_keys: Vec::new()` by design (`anim/entry.rs:266`). #1672/#1673 is a pure
  structural refactor (lift nested fns; 463→~330 LOC) with no logic change.

### Dimension 8 — Shader flags / texture sets / effect shaders (PASS)
- **Zero** `if game ==` branches in `crates/renderer/shaders/` including the
  `#include`d `include/*.glsl` headers — the cardinal no-render-time-fallback
  invariant for this dimension holds. Per-game flag vocabularies are dispatched by
  block type at parse.
- FO4 render-affecting flags reach the canonical `MaterialInfo` (#1592 intact):
  `Model_Space_Normals` (F4SF1 bit 12) + `Alpha_Test` (F4SF2 bit 25) + the FO76+
  `MODELSPACENORMALS` CRC are ORed into `info.model_space_normals` /
  `info.alpha_test` in `import/material/walker.rs`, not deferred to a render-time
  `if game == fo4`.

### Dimension 9 — Translation-completeness signal + cross-cutting invariants (PASS)
- The `cross_game_translation_completeness` harness
  (`crates/nif/tests/translation_completeness.rs`) remains `#[ignore]`-gated behind
  real game data, with conservative documented fill-rate floors (`>= 60.0`
  texture-path, `>= 40.0` tangents). No unverified-game fill-rate collapse was
  flagged. All four cross-cutting tier invariants pass across every category (see
  the Tier Matrix): each category that needs a boundary declares exactly one; no
  fabricated constant introduced; no `Option`/raw discriminator on a canonical type
  reaches a re-resolving consumer; no per-draw classification heuristic downstream.

---

## Documented-limitation ledger (parked-not-leak — do NOT re-report)

These are known, bounded gaps recorded in `nifal.md` §2 (and verified this sweep to
have zero canonical consumers). Each is blocked on a consumer feature that does not
yet exist; translating now would invent an ECS component nothing reads
(no-fabrication). Restated so the next sweep does not re-file them:

| Item | Source block | State | Blocked on |
|---|---|---|---|
| `bs_value_node` | `BSValueNode` | raw-parked, 0 consumers | M35 LOD selector / billboard hinting |
| `bs_ordered_node` | `BSOrderedNode` | raw-parked, 0 consumers | `RenderOrderHint` + `build_render_data` sort key |
| `tree_bones` | `BSTreeNode` | raw-parked, 0 consumers | SpeedTree wind/bend sim |
| `range_kind` | `BSRange/DamageStage/Blast/DebrisNode` | raw-parked, 0 consumers | destructible / blast / debris systems |
| `lod_group` | `NiLODNode` → `NiRangeLODData` | foundation parsed (child 0 only); **content-absent** in shipped archives | per-frame distance-switch system |
| `bs_lod_cutoffs` | `BSLODTriShape` | raw-parked; content-bearing in-cell LOD (Skyrim ~43 meshes) | in-cell LOD draw-count consumer |
| `bs_sub_index` | `BSSubIndexTriShape` | raw-parked, 0 consumers | dismemberment / locational-damage system |
| `NiTextureEffect` | `NiTextureEffect` | extractor never called; **content-absent** (0 across shipped archives) | (none — do not build speculatively) |
| `NiSwitchNode` identity | `NiSwitchNode` | walked via active-index; type discriminator not surfaced | geometry state-switching driver |
| furniture / inv markers | `BSFurnitureMarker` / `BSInvMarker` | parsed, not walked into `Imported*` | AI sit/lean/sleep packages; inventory-icon system |
| `bs_bound` | `BSBound` extra-data | consumed on loose-NIF path only | a cell-path bound consumer (low value) |
| Particle size-over-life *curve* | `NiPSysGrowFadeModifier` bell shape | only authored *magnitude* translated | richer canonical size model |
| Particle `initial_color` | `NiPSysEmitter` | intentionally unapplied (white nif.xml default) | (design — colour owned by `color_curve`) |
| `BhkNPCollisionObject` | FO4/FO76/Starfield `BhkSystemBinary` blob | decoder is a separate project | cell-loader falls back to synthesized static trimesh |
| `BhkPCollisionObject` / phantoms | Skyrim+ trigger volumes | dropped to `None` | a `TriggerVolume` ECS path |
| `BhkPlaneShape` | AABB-bounded infinite plane (#1334) | `None`-drop → trimesh fallback | a half-space `CollisionShape` variant |
| `BSEffectShaderProperty.base_color_scale` | FO4+ diffuse-tint | tagged via `EmissiveSource::Effect`, not dropped | a proper BSEffect render path (#166) |
| Per-light **ambient** + **morph-weight** anim channels | NIF controllers | captured, no renderer consumer | ambient/morph render consumer |

---

*Report generated 2026-07-02 against HEAD `1b4e8e84`. Next step:
`/audit-publish docs/audits/AUDIT_NIFAL_2026-07-02.md` (no findings to publish this
sweep).*
