# NIFAL Audit — Canonical Translation Layer — 2026-06-13

**Scope**: All 7 dimensions of the NIF Abstraction Layer (canonical translation tier).
Spec: `docs/engine/nifal.md`. This audits the **translate** side (does each parsed
category reach one canonical representation through one boundary, with no leak /
fabrication / render-time fallback?) — distinct from `/audit-nif`, which owns
parse-side byte correctness.

Second NIFAL-dedicated sweep (prior: `AUDIT_NIFAL_2026-05-30.md`). The completeness
harness ran live against all 7 installed game data dirs (Oblivion / FO3 / FNV /
Skyrim SE / FO4 / FO76 / Starfield). Tree @ `main` (c2778fc5).

---

## Executive Summary

**The NIFAL discipline holds across every category. No canonical-tier leak, no
fabrication, no render-time fallback, no scattered boundary.** The two collision
leaks that were the headline of the 2026-05-30 sweep (D6-01 / D6-02) are confirmed
fixed (#1360 / #1361) and the collision coverage diff is now empty (15 resolve arms
cover the 15 dispatched solid shapes, mechanically guarded by a test). The
completeness blind spot (D7-A / #1362) is fixed and *effective* — the Starfield
external-`.mesh` resolver actually imports geometry now.

Per-category convergence vs the spec §2 leak inventory:
- **Material** — converged ✓ (reference realisation; all 7 invariants pass)
- **Geometry / Transform** — converged ✓ (the clean template; 3 per-game decoders converge)
- **Skinning** — converged ✓ (global bone indices at extraction; #613 remap intact)
- **Lights** — converged ✓ (`LightKind` collapses the source-block discriminator; radius derived from attenuation)
- **Nodes** — 4 parked passthroughs re-confirmed parked (zero canonical consumers); live fields consumed ✓
- **Particles** — base-params boundary converged ✓; one LOW single-boundary smell on the *adjacent* overlays (D5-NEW-01)
- **Collision** — converged ✓ (coverage diff empty; both prior HIGH leaks fixed)

**Tier-invariant violation count**: single-boundary **1 LOW** (D5-NEW-01) ·
no-fabrication 0 · no-leak 0 · no-render-time-fallback 0. Plus one
completeness-**signal** miscalibration (D7-NEW-01, test infrastructure, not a layer
violation).

**Severity tally**: 0 CRITICAL · 0 HIGH · **1 MEDIUM** (D7-NEW-01) · **2 LOW**
(D1-NEW-01 doc-rot, D5-NEW-01 single-boundary smell).

Versus the prior sweep (2 HIGH + 1 MEDIUM + 6 LOW), this is a markedly cleaner tree:
every prior HIGH/MEDIUM is closed and verified, and the only genuine code-tier
finding this round is a LOW duplication smell.

---

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | ✅ | ✅ | ✅ | ✅ | `material_translate::translate_material` |
| Geometry / Transform | ✅ | ✅ | ✅ | ✅ | import extractors → `MeshRegistry::upload`; `import/coord.rs` |
| Skinning | N/A | ✅ | ✅ | N/A | `import/mesh/skin.rs` (global remap at extraction) |
| Lights | N/A | ✅ | ✅ | N/A | `import/walk/mod.rs::walk_node_lights` → `LightKind` |
| Nodes | N/A (by design, spec §2) | ✅ | ✅ (4 parked, 0 consumers) | N/A | dual spawn paths (documented) |
| Particles | ⚠️ base-params ✅ / adjacent overlays inline (D5-NEW-01) | ✅ | ✅ | N/A | `systems/particle.rs::apply_emitter_params` |
| Collision | ✅ | ✅ | ✅ | N/A | `import/collision.rs::resolve_shape` |

Cross-cutting verdict (Dim 7): single-boundary / no-fabrication / no-leak /
no-render-time-fallback all **PASS** across every category in the code tier. The
single ⚠️ is a LOW duplication smell, not a divergence.

---

## Findings

### D7-NEW-01: #1320 per-game fill-rate floors are miscalibrated against ground truth — completeness harness RED for three games with no real regression
- **Severity**: MEDIUM
- **Dimension**: Completeness
- **Tier Violated**: n/a (test-infrastructure — the *signal* is broken, not the layer)
- **Game Affected**: FNV (reached), FO76 + Starfield (unreached, latent)
- **Location**: `crates/nif/tests/translation_completeness.rs:384` (FNV `m_kind>=35`), `:442` (FO76 `tex>=75`), `:463` (Starfield `tex>=75`)
- **Status**: NEW
- **Description**: The per-game fill-rate floors added by Fix #1320 (commit `4376f7a6`) were authored on the assumption "newer engine ⇒ higher fill-rate for every slot." Three floors contradict the actual measured content of the 200-NIF sample:
  - **FNV `material_kind >= 35%`** — actual **8.1%**. `material_kind` is set from `shader.shader_type` **only** on the `BSLightingShaderProperty` arm (`crates/nif/src/import/material/walker.rs:337`), plus engine-synthesized 101 (effect) / 102 (nolighting). FNV content uses `BSShaderPPLightingProperty`, which never sets `material_kind` — FNV legitimately classifies only its effect/nolighting meshes (8.1%). 35% was never achievable on this corpus.
  - **FO76 `texture_path >= 75%`** — actual **9.6%**. FO76 NIFs fully migrated texture references into BGSM (`material_path = 90.4%`); inline `texture_path` is nearly empty. Same architecture as FO4, except FO4 NIFs still carry inline paths (tex=100%) so the FO4 floor passes while FO76's identical floor cannot.
  - **Starfield `texture_path >= 75%`** — actual **0.0%**. `BSGeometry` carries no inline texture path; material lives in `material_path` (100%) / CDB. The floor's own sibling comment already acknowledges Starfield material is CDB-resolved, yet still asserts a 75% inline-texture floor.
- **Evidence**: Live run (table below) panics at `:383` — `[FNV] material_kind fill < 35% (got 8.1%)`. Walker arm confirmed at `walker.rs:337` (only `BSLightingShaderProperty` writes `material_kind`). Floor origin: `git log -L` → `4376f7a6 Fix #1320`.
- **Impact**: The cross-game completeness regression signal is **non-functional** — it panics on the first game before reaching the FO76/Starfield floors, and presents as a hard translation-regression failure when it is a stale-threshold artifact. A *real* future regression (e.g. FNV losing tangent synthesis) would be masked behind this pre-existing red. This is exactly the "unvalidated threshold" failure mode TD-D6/#1320 was filed to eliminate, recurring one tier up.
- **Related**: TD-D6/#1320 (CLOSED — added the floors), D7-A/#1362 (CLOSED — added the games)
- **Suggested Fix**: Recalibrate the three floors to measured ground truth with a conservative margin: FNV `material_kind >= 5%`; FO76 assert `material_path >= 75%` (the slot that actually carries FO76's material identity) instead of `texture_path`; Starfield drop the `texture_path` floor and assert `material_path >= 75%` + `tangents >= 65%` (both real) — its 0% inline texture is canonical for `BSGeometry`, not a gap. Document the *measured* value beside each floor (the #1320 fix added floors but not the measurements behind them).

### D5-NEW-01: Particle color / rate / force-field overlays duplicated inline at both load-path sites instead of routed through the shared apply helper
- **Severity**: LOW
- **Dimension**: Particles
- **Tier Violated**: single-boundary (§1 "exactly one site per category — no duplicate construction sites")
- **Game Affected**: all (any particle NIF)
- **Location**: `byroredux/src/scene/nif_loader.rs:531-554` and `byroredux/src/cell_loader/spawn.rs:411-434`
- **Status**: NEW
- **Description**: The particles slice centralised the *base-params* overlay into `apply_emitter_params` (checklist PASS). But the three adjacent authored overlays — `color_curve → start_color/end_color`, `emitter_rate → rate`, and `force_fields → convert_force_fields_zup_to_yup` — are written as literal inline blocks duplicated verbatim at both call sites, not behind the shared helper. The base-params helper exists precisely so "a field added in one place can no longer silently diverge the two load paths" (the rationale the spec uses for the Materials boundary, §3); the remaining three overlays do not get that guarantee.
- **Evidence**: `nif_loader.rs:531-534` (color), `:544-546` (rate), `:553-554` (force) — each mirrored at `spawn.rs:411-414`, `:423-425`, `:434`. Same shape, same field assignments, copy-pasted (only the source struct name differs, `emitter.*` vs `em.*`). Contrast `apply_emitter_params` (`particle.rs:29`) which both sites *do* call.
- **Impact**: Low today — each block is a trivial 1-3 line assignment from an already-centralised single-source value (`extract_first_color_curve` / `extract_emitter_rate` / `convert_force_fields_zup_to_yup`), so the divergence surface is small. But it is the "second construction site" smell the tier forbids: a future authored-overlay addition (e.g. wiring the #1333 local-transform fix, or a size-over-life curve) must be hand-mirrored across two files.
- **Related**: spec §3 (Materials de-duplication rationale)
- **Suggested Fix**: Fold the three overlays into `apply_emitter_params` (or a sibling `apply_emitter_overlays`) taking the common `color_curve / emitter_rate / force_fields / emitter_params` subset, so both load paths call one helper — matching the `translate_material` template.

### D1-NEW-01: Stale `resolve_classifier_overrides` symbol in `material-abstraction.md`
- **Severity**: LOW (doc-rot)
- **Dimension**: Material
- **Tier Violated**: none (documentation only)
- **Game Affected**: none
- **Location**: `docs/engine/material-abstraction.md:143,147` (also the `roughness_override = 0.10` framing at `:133,:150`)
- **Status**: NEW (the code-side rename was closed as #1309 / OB-D7-001, but #1309's body was repurposed to an unrelated wireframe-pipeline topic and these two doc lines were never fixed; no open issue tracks them. Prior audits flagged it as D1-01 / OB-D7-001 — re-confirmed STILL present.)
- **Description**: The function was renamed `resolve_classifier_overrides` → `resolve_pbr`. The code reference in `material_translate.rs` is already correct, but `material-abstraction.md` step 2 ("`resolve_classifier_overrides` collapses the `Option`s…") and step 3 ("…right after `resolve_classifier_overrides`") still cite the dead symbol, and the same region uses the pre-canonical `roughness_override = 0.10` framing (the canonical field is now `Material.roughness`, forced by `classify_glass_into_material`).
- **Evidence**: No symbol `resolve_classifier_overrides` exists in any `.rs` file (grep returns only docs + audit-report references). The live name is `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs:638`).
- **Impact**: A reader following `material-abstraction.md` greps for a symbol that does not exist and may believe the Option-collapse step is unimplemented. Pure doc-rot; zero runtime effect.
- **Related**: #1309 (CLOSED — code-side only)
- **Suggested Fix**: `s/resolve_classifier_overrides/resolve_pbr/` at lines 143, 147; update the `roughness_override = 0.10` framing at 133/150 to `Material.roughness` (forced glass-smooth). Note in the commit that #1309 closed only the code-side ref.

---

## Captured per-game fill-rate table (live harness run, 2026-06-13)

```
game             imported   tex%   mat_path%  m_kind%  metO%  rghO%   nrm%    tan%   consistent%
Oblivion     meshes=2117   99.9%     0.0%      0.0%   100%   100%    0.0%   99.9%   100.0%
FO3          meshes= 596   98.7%     0.0%      5.0%   100%   100%   94.1%  100.0%   100.0%
FNV          meshes= 629   95.1%     0.0%      8.1%   100%   100%   89.2%   97.3%   100.0%
SkyrimSE     meshes=  97  100.0%     0.0%     60.8%   100%   100%   84.5%  100.0%   100.0%
FO4          meshes= 269  100.0%    77.0%     30.5%   100%   100%   90.7%  100.0%   100.0%
FO76         meshes= 293    9.6%    90.4%      9.6%   100%   100%    5.5%  100.0%   100.0%
Starfield    meshes= 176    0.0%   100.0%      0.0%   100%   100%    0.0%  100.0%   100.0%
```

**Structural consistency = 100.0% on every game** (the hard invariant) — clean.
`metO`/`rghO` = 100% everywhere because PBR overrides are seeded at the translate
boundary for all NIF content (expected post-#1346). `tex`=0 for Oblivion is the
expected legacy `NiTexturingProperty` path (texture resolved at spawn, not on the
mesh struct); `tex`=0 / `mat_path`=100 for Starfield and `tex`≈10 / `mat_path`=90
for FO76 are the canonical external-material architectures (BGSM / CDB), **not**
leaks. The harness run is RED only because of the miscalibrated floors (D7-NEW-01),
not any extractor regression.

---

## Documented-limitation ledger (parked-not-leak — do NOT re-report next sweep)

- **Node passthroughs** (Dim 4 — all 4 re-confirmed PARKED, zero canonical consumers; whole-tree grep of `byroredux/` for the four field names and their payload types returned empty outside parser/import/test tiers): `bs_value_node` (→ M35 LOD selector), `bs_ordered_node` (→ `RenderOrderHint` + sort key), `tree_bones` (→ SpeedTree wind/bend), `range_kind` (→ destructible/blast/debris systems). The cell loader discards the `nodes` array entirely (`cell_loader/references.rs:1048-1061`), so the parked fields are structurally unreachable on that path. When a consumer feature lands, its slice translates the already-captured field.
- **Collision FO4+ NP blob** (`BhkNPCollisionObject`): Havok-serialised blob; decoder is a separate project. Consumer falls back to `cell_loader/spawn.rs::synthesize_static_trimesh`. `is::<BhkNPCollisionObject>` discriminator intact. NOT a leak.
- **Collision phantoms** (`BhkPCollisionObject` / `BhkSimpleShapePhantom` / `BhkAabbPhantom`): Skyrim+ trigger volumes; both phantom subclasses now drop uniformly at `import/collision.rs:535` via an explicit `is::<>` discriminator (D6-03 / #1363 fixed the prior inconsistency). Need a `TriggerVolume` ECS path, not a rigid body. NOT a leak.
- **Particle size-over-life curve** (Dim 5): the grow→steady→fade bell shape can't map to the linear `start_size → end_size`; only the authored *magnitude* (`initial_radius × base_scale`) is translated. Documented future work, NOT a leak.
- **Particle `initial_color`** (Dim 5): intentionally NOT applied — colour is owned by the `color_curve` override. Applying the white nif.xml default would be a no-fabrication-in-reverse regression.
- **Emissive normalization** (Dim 1, spec §4): the three `EmissiveSource` variants share a measured ~1.0 scale; NO normalization is applied or wanted. A future "emissive normalization constant" is a no-fabrication violation. Open question Q2 in material-abstraction.md is resolved no-op.
- **Lights** (Dim 3): point/spot cone + NIF-direct ambient/directional are flattened to point `LightSource` at spawn — a renderer feature-completeness gap, NOT a tier violation (the discriminator collapses at the single boundary with no downstream re-resolution).

---

## Existing OPEN issues touching this layer (tracked, not re-filed)

- **#1445 / LC-D9-02** — `extract_emitter_params` `all_finite` sweep (`walk/mod.rs:714-720`) omits `planar_angle` / `planar_angle_variation` from the NaN/Inf guard. Confirmed STILL OPEN. (The data has no canonical `ParticleEmitter` consumer, so it is a correct no-fabrication park; #1445 is purely the robustness-sweep gap.)
- **#1333 / NIF-2026-05-29-05** — modern `NiParticleSystem` local transform discarded → emitter ignores host-relative offset. Confirmed STILL OPEN. Arguably a NIFAL drop on the modern path, but already tracked.
- **#1334 / NIF-2026-05-29-06** — Skyrim SE `bhkPlaneShape` undispatched. Parser-tier gap (no `BhkPlaneShape` struct exists at all → cannot be a parsed-then-dropped no-leak finding); correctly owned by `/audit-nif`.
- **#1357 / D7-04** — `BGSM_*` flag-alias constants not yet renamed to canonical names. Cosmetic naming migration; bits resolved at the boundary and consumed game-agnostically. Not a tier violation.

---

## Regression Guards (verified HOLD this sweep)

- **Material**: single boundary (`translate_material`, exactly 2 callers — `nif_loader.rs:818`, `spawn.rs:872`; `cornell.rs` is a non-NIF test scene, not a third site); plain-f32 PBR; glass-once-after-resolve; emissive no-op; `resolve_pbr` idempotent + fills-only-NaN; no render-side `classify_pbr`/glass heuristic survives. #1346/#1365 hold.
- **Geometry**: three per-game decoders converge to one `Vec<[f32;3]>` + `Vec<u32>` (Y-up); coord conversion single-source (`import/coord.rs`); SVD repair once at parse (`rotation.rs`, two `stream.rs` read sites); `MeshRegistry::upload` format-agnostic. D2-NEW-01 doc fix (#1366) confirmed; D2-NEW-02 defensive coord SVD re-verified unreachable in production (incl. the FO4 CSG precombine path, which also sanitizes at parse).
- **Skinning**: `ImportedSkin` global bone indices (#613) remapped at extraction on all three BSTriShape sub-paths (`skin.rs:111/116/122`); u16-range warning intact (`skin.rs:147-153`); no downstream partition re-derivation.
- **Lights**: source-block discriminator collapsed to `LightKind` at one site (`walk/mod.rs:1121-1178`); radius genuinely derived from attenuation (`attenuation_radius`, `walk/mod.rs:1549`), single `2048.0` degenerate-clamp documented; zero `Ni*Light` downcasts in the renderer.
- **Collision**: 15 resolve arms cover the 15 dispatched solid shapes (coverage diff empty, guarded by `dispatch_coverage_tests::every_dispatched_bhk_shape_has_resolve_arm`); D6-01/#1360 (ConvexSweep) + D6-02/#1361 (Mesh) + D6-03/#1363 (uniform phantom drop) all in place; MultiSphere→Compound-of-Balls + ConvexList→Compound pins hold; `havok_scale` single-application (3 reads, all in `collision.rs`).
- **Particles**: typed `NiPSysEmitter`/`Ctlr`/`CtlrData`/`GrowFadeModifier` blocks; `read_emitter_base` reads (not skips); base-params route through `apply_emitter_params`; `sane()` now rejects FLT_MAX (`(0.0..3.0e38)`, D5-03/#1364 closed); `base_scale` finite-positive (#1434).
- **Cross-cutting**: `triangle.frag` has zero `if (game ==` branches; canonical `Material` `Option`s are absence-encodings or `material_kind`-gated payload slots, never re-resolved per-game.
