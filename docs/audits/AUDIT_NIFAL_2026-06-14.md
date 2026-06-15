# NIFAL Audit — Canonical Translation Layer — 2026-06-14

**Scope**: All 9 dimensions of the NIF Abstraction Layer (canonical translation tier).
Spec: `docs/engine/nifal.md`. This audits the **translate** side (does each parsed
category reach one canonical representation through one boundary, with no leak /
fabrication / render-time fallback?) — distinct from `/audit-nif`, which owns
parse-side byte correctness.

Third NIFAL-dedicated sweep (priors: `AUDIT_NIFAL_2026-05-30.md`,
`AUDIT_NIFAL_2026-06-13.md`). Run as part of a `comprehensive` audit-suite sweep.
Tree @ `main` (435e265d). The completeness harness was inspected statically (it is
`#[ignore]`-gated and needs a Vulkan-free game-data run; the calibration of its
floors is what this dimension audits, not a live re-run).

---

## Executive Summary

**The NIFAL discipline holds across every category. No canonical-tier leak, no
fabrication, no render-time fallback, no scattered boundary, in the code tier.**

Every code-tier finding from the immediately-prior sweep (2026-06-13) is now CLOSED
and verified fixed against the live tree:

- **D5-NEW-01 / #1513** (particle colour/rate/force-field overlays duplicated inline
  at both load paths) — **FIXED**. There is now a single `apply_emitter_overlays`
  boundary (`byroredux/src/systems/particle.rs:60`) that folds all four authored
  overlays (colour curve, base params, birth rate, force fields); both spawn sites
  route through it (`scene/nif_loader.rs:526`, `cell_loader/spawn.rs:413`). The
  particles slice now matches the `translate_material` single-boundary template.
- **D7-NEW-01 / #1512** (the #1320 per-game completeness floors were miscalibrated —
  harness RED for FNV/FO76/Starfield with no real regression) — **FIXED**. The three
  floors are recalibrated to measured ground truth with the measurement documented
  beside each (`translation_completeness.rs`: FNV `material_kind >= 5%` :390, FO76
  `material_path >= 75%` :459, Starfield `material_path >= 75%` :480).
- **D7-B / #1365** (stale render-time-fallback doc on `ImportedMesh.metalness_override`)
  — **FIXED**. The `types.rs:481-510` doc now describes the boundary-only contract
  ("read only at the `translate_material` boundary; the renderer never sees it").

Per-category convergence vs the spec §2 leak inventory:
- **Material** — converged ✓ (reference realisation; all invariants pass; single
  boundary `translate_material`, exactly 2 production callers)
- **Geometry / Transform** — converged ✓ (the clean template; 3 per-game decoders +
  the FO4-CSG precombine path converge to one Y-up `Vec<[f32;3]>`/`Vec<u32>`)
- **Skinning** — converged ✓ (global bone indices at extraction; #613 remap intact on
  all 3 BSTriShape sub-paths; u16-range warning intact)
- **Lights** — converged ✓ (`LightKind` collapses the source-block discriminator;
  radius derived from attenuation; zero `Ni*Light` downcasts downstream)
- **Nodes** — 7 parked passthroughs re-confirmed parked (zero canonical consumers);
  live fields consumed ✓
- **Particles** — base-params **and** the four overlays now both behind one boundary
  (the prior LOW smell closed)
- **Collision** — converged ✓ (coverage diff empty; 15 resolve arms cover the 15
  dispatched solid shapes, mechanically guarded by a test)
- **Animation** — converged ✓ (single `convert_nif_clip` boundary, multiple legit
  callers; zero era branches downstream)
- **Shader-flags / Effects** — converged ✓ (per-game vocabularies dispatched by block
  type; `triangle.frag` `if (game ==` count = 0; all 9 BSLightingShaderProperty
  variants forward trailing data via wildcard-free matches)

**Tier-invariant violation count**: single-boundary 0 · no-fabrication 0 · no-leak 0
· no-render-time-fallback 0.

**Severity tally**: 0 CRITICAL · 0 HIGH · 0 MEDIUM · **1 LOW** (D1-NEW-01 doc-rot,
re-confirmed but now deliberately reframed as a historical note).

This is the cleanest tree the NIFAL sweep has measured: the prior round still carried
1 MEDIUM + 2 LOW, all now closed, and only a single LOW doc-rot remains.

---

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | ✅ | ✅ | ✅ | ✅ | `material_translate::translate_material` |
| Geometry / Transform | ✅ | ✅ | ✅ | ✅ | import extractors → `MeshRegistry::upload`; `import/coord.rs` |
| Skinning | N/A | ✅ | ✅ | N/A | `import/mesh/skin.rs` (global remap at extraction) |
| Lights | N/A | ✅ | ✅ | N/A | `import/walk/mod.rs::walk_node_lights` → `LightKind` |
| Nodes | N/A (by design, spec §2) | ✅ | ✅ (7 parked, 0 consumers) | N/A | dual spawn paths (documented) |
| Particles | ✅ | ✅ | ✅ | N/A | `systems/particle.rs::apply_emitter_overlays` |
| Collision | ✅ | ✅ | ✅ | N/A | `import/collision.rs::resolve_shape` |
| Animation | ✅ | ✅ | ✅ | ✅ | `anim_convert::convert_nif_clip` |
| Shader-flags / Effects | ✅ | ✅ | ✅ | ✅ | `import/material/walker.rs` (block-type dispatch) → `MaterialInfo` |

Cross-cutting verdict (Dim 9): single-boundary / no-fabrication / no-leak /
no-render-time-fallback all **PASS** across every category in the code tier.

---

## Findings

### D1-NEW-01: Dead symbol `resolve_classifier_overrides` still named in `material-abstraction.md`
- **Severity**: LOW (doc-rot)
- **Dimension**: Material
- **Tier Violated**: none (documentation only)
- **Game Affected**: none
- **Location**: `docs/engine/material-abstraction.md:143`
- **Status**: Existing (re-confirmed) — prior IDs D1-01 (`AUDIT_NIFAL_2026-05-30.md`),
  D1-NEW-01 (`AUDIT_NIFAL_2026-06-13.md`); originally #1309 (CLOSED — code-side rename
  only, the doc line was never removed). No open issue tracks the doc line.
- **Description**: The function was renamed `resolve_classifier_overrides` →
  `resolve_pbr`. All `.rs` references are correct (grep for the old symbol returns only
  `material-abstraction.md` + prior audit reports — zero source hits). Since the last
  sweep the line has been **reframed** from a flat stale citation into a historical
  note: it now reads "`Material::resolve_pbr` (planned here as
  `resolve_classifier_overrides`, renamed before landing)". The companion `:147`
  reference the prior audit flagged is gone, and the `roughness_override = 0.10`
  framing the prior audit flagged at `:133/:150` is also gone (the remaining
  `roughness_override` mentions at `:10/:50/:86` correctly refer to the **raw-tier**
  `ImportedMesh.metalness_override`/`roughness_override` field, which still exists under
  that name — those are not doc-rot). What remains is one dead symbol name still spelled
  out in prose.
- **Evidence**: `grep -rn resolve_classifier_overrides docs/ crates/ byroredux/` →
  one hit at `material-abstraction.md:143`; the live symbol is `Material::resolve_pbr`
  (`crates/core/src/ecs/components/material.rs`).
- **Impact**: A reader greps for `resolve_classifier_overrides`, finds it only in this
  doc, and may believe the symbol still exists. Pure doc-rot, zero runtime effect. Note
  the design-doc is already banner-marked superseded by `nifal.md` (`:1-14`), which
  lowers the harm — this finding is LOW-confidence as a *bug* because the rename note is
  arguably intentional history. Recorded for completeness because it was flagged in
  three prior sweeps and never cleaned.
- **Related**: #1309 (CLOSED), D1-01 / D1-NEW-01 (prior reports)
- **Suggested Fix**: Either drop the trailing parenthetical entirely
  ("`Material::resolve_pbr` collapses the `Option`s…") or rephrase so the dead symbol
  is not spelled as if it were a live name (e.g. "*was* drafted as
  `resolve_classifier_overrides`"). One-line edit; or close as wontfix given the
  superseded banner.

---

## Documented-limitation ledger (parked-not-leak — do NOT re-report next sweep)

- **Node passthroughs** (Dim 4 — all 7 re-confirmed PARKED, zero canonical consumers;
  whole-tree grep of the field names *and* their payload types returned empty outside
  parser/import/test tiers): `bs_value_node` (→ M35 LOD selector), `bs_ordered_node`
  (→ `RenderOrderHint` + sort key), `tree_bones` (→ SpeedTree wind/bend), `range_kind`
  (→ destructible/blast/debris systems), `lod_group` (→ per-frame distance switch),
  `bs_lod_cutoffs` (→ in-cell LOD draw-count), `bs_sub_index` (→ dismemberment). The
  two `byroredux/src` hits each for `bs_lod_cutoffs`/`bs_sub_index` are **producers**
  (test-helper + CSG/CDB-test `ImportedMesh { … }` literals assigning `None`), not
  consumers. The deeper §2 "Passthroughs" inventory (`NiTextureEffect` content-absent,
  `NiSwitchNode` identity, `BSFurnitureMarker`/`BSInvMarker`, `BSBound` cell-path) is
  unchanged.
- **Collision FO4+ NP blob** (`BhkNPCollisionObject`): Havok-serialised
  `BhkSystemBinary` blob; decoder is a separate project. Consumer falls back to
  `cell_loader/spawn.rs::synthesize_static_trimesh` (gated on `collisions.is_empty()`,
  so no double scale-application). `is::<BhkNPCollisionObject>` discriminator intact.
  NOT a leak.
- **Collision phantoms** (`BhkPCollisionObject` / `BhkSimpleShapePhantom` /
  `BhkAabbPhantom`): Skyrim+ trigger volumes; both phantom subclasses drop uniformly
  via explicit `is::<>` discriminators (D6-03 / #1363). Need a `TriggerVolume` ECS
  path, not a rigid body. NOT a leak.
- **Particle size-over-life curve** (Dim 5): the grow→steady→fade bell shape can't map
  to the linear `start_size → end_size`; only the authored *magnitude*
  (`initial_radius × base_scale`) is translated. Documented future work, NOT a leak.
- **Particle `initial_color`** (Dim 5): intentionally NOT applied — colour is owned by
  the `color_curve` override. Applying the white nif.xml default would be a
  no-fabrication-in-reverse regression.
- **Emissive normalization** (Dim 1, spec §4): the three `EmissiveSource` variants
  share a measured ~1.0 scale; NO normalization is applied or wanted. A future
  "emissive normalization constant" is a no-fabrication violation. Open question Q2 in
  `material-abstraction.md` is resolved no-op.
- **Lights** (Dim 3): point/spot cone + NIF-direct ambient/directional flattened to
  point `LightSource` at spawn — a renderer feature-completeness gap, NOT a tier
  violation (the discriminator collapses at the single boundary with no downstream
  re-resolution; spawn reads only `radius`/`color`, never `light.kind`).
- **Animation parked channels** (Dim 7): per-light ambient colour channels
  (`ColorTarget::LightAmbient`, explicit no-op park `systems/animation.rs`),
  morph-weight channels (`AnimatedMorphWeights` written but no renderer/GPU consumer),
  and texture-flip channels (carried into the clip, no apply path). All captured-but-
  parked at the GPU boundary, not re-resolved. NOT leaks.

---

## Existing OPEN issues touching this layer (tracked, not re-filed)

- **#1445 / LC-D9-02** — `extract_emitter_params`' `all_finite` sweep
  (`walk/mod.rs:714-720`) omits `planar_angle` / `planar_angle_variation` from the
  NaN/Inf guard. Confirmed STILL OPEN. The data has no canonical `ParticleEmitter`
  consumer (correct no-fabrication park), so #1445 is purely the robustness-sweep gap.
- **#1333 / NIF-2026-05-29-05** — modern `NiParticleSystem` local transform discarded
  → emitter ignores host-relative offset. Confirmed STILL OPEN. Arguably a NIFAL drop
  on the modern path, but already tracked.
- **#1334 / NIF-2026-05-29-06** — Skyrim SE `bhkPlaneShape` undispatched. Parser-tier
  gap (no `BhkPlaneShape` struct exists at all → cannot be a parsed-then-dropped
  no-leak finding); correctly owned by `/audit-nif`.

---

## Regression Guards (verified HOLD this sweep)

- **Material**: single boundary (`translate_material`, exactly 2 production callers —
  `nif_loader.rs:796`, `spawn.rs:857`; `cornell.rs` is a non-NIF test scene); plain-f32
  PBR seeded from override-or-NaN then `resolve_pbr` clamp; glass classified once,
  alpha-aware, AFTER `resolve_pbr` (`classify_glass_into_material`); emissive no-op; no
  render-side `classify_pbr`/glass heuristic. #1346/#1365 hold. The `normal_alpha_spec`
  roughness is resolved once at spawn into `Material.roughness` (#1480), idempotent,
  not per-draw.
- **Geometry**: three per-game decoders + the FO4-CSG precombine path converge to one
  `Vec<[f32;3]>` + `Vec<u32>` (Y-up); coord conversion single-source
  (`import/coord.rs`, zero swap sites in `crates/renderer`); SVD repair once at parse
  (`rotation.rs`, two `stream.rs` read sites); `MeshRegistry::upload` format-agnostic;
  `local_bound_radius` derived in Y-up at extraction, runtime only transforms it.
- **Skinning**: `ImportedSkin` global bone indices (#613) remapped at extraction on all
  three BSTriShape sub-paths (`skin.rs:111/116/122`); u16-range warning intact
  (`skin.rs:147-153`); `global_skin_transform` carried; no downstream partition
  re-derivation (zero `NiSkinPartition`/`vertex_map` in renderer).
- **Lights**: source-block discriminator collapsed to `LightKind` at one site
  (`walk/mod.rs:1121-1178` / `imported_light_from_base`); radius derived from
  attenuation (`attenuation_radius`, 1/256 cull, single `2048.0` degenerate-clamp);
  zero `Ni*Light` downcasts in `crates/renderer` + `byroredux/src/render`.
- **Collision**: 15 resolve arms cover the 15 dispatched solid shapes (coverage diff
  empty, guarded by `dispatch_coverage_tests::every_dispatched_bhk_shape_has_resolve_arm`,
  non-vacuous, passes); MultiSphere→Compound-of-Balls + ConvexList→Compound +
  ConvexSweep-delegate + Mesh-tristrip pins hold; `havok_scale` single-application
  (3 reads, all in `collision.rs`); Z-up→Y-up `(x,z,-y)` + quaternion swap; non-finite
  guarded.
- **Particles**: typed `NiPSysEmitter`/`Ctlr`/`CtlrData`/`GrowFadeModifier` blocks;
  `read_emitter_base` reads (not skips); the four authored overlays now route through
  the single `apply_emitter_overlays` (#1513); `sane()` rejects FLT_MAX
  (`(0.0..3.0e38)`, D5-03/#1364); `base_scale` finite-positive; spawn-step NaN/Inf
  guards on `rate`/`start_size` (#1382).
- **Animation**: single `convert_nif_clip` boundary (multiple legit callers; no second
  field-by-field construction of the canonical clip); B-splines sampled to linear keys
  on FO3/FNV + Skyrim+; XYZ-Euler → quaternions; Z-up→Y-up once; zero era/game branches
  in `player.rs`/`stack.rs`/`interpolation.rs` apply paths; text keys wired, embedded
  `text_keys: Vec::new()` a deliberate empty.
- **Cross-cutting**: `triangle.frag` `if (game ==` count = 0; all 9
  BSLightingShaderProperty shader-type variants forwarded via wildcard-free matches
  (compile-error on a new variant, not a silent drop); compile-time SLSF bit-26/27
  equivalence asserts intact; canonical `Material` `Option`s are absence-encodings or
  `material_kind`-gated payload slots, never re-resolved per-game.
