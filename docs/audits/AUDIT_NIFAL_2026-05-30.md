# NIFAL Audit — Canonical Translation Layer — 2026-05-30

**Scope**: All 7 dimensions of the NIF Abstraction Layer (canonical translation tier).
Spec: `docs/engine/nifal.md`. This audits the **translate** side (does each parsed category
reach one canonical representation through one boundary, with no leak / fabrication / render-time
fallback?) — distinct from `/audit-nif` which owns parse-side byte correctness.

First NIFAL-dedicated audit (the layer opened 2026-05-28). The completeness harness ran live
against all 7 installed game data dirs.

---

## Executive Summary

**The NIFAL discipline holds across all categories — with one genuine canonical-tier leak in
Collision and one verification-signal blind spot.**

Per-category convergence vs the spec §2 leak inventory:
- **Material** — converged ✓ (reference realisation; all 4 invariants pass)
- **Geometry / Transform** — converged ✓ (the clean template; three per-game decoders converge)
- **Skinning** — converged ✓ (global bone indices at extraction)
- **Lights** — converged ✓ (`LightKind` collapses the source-block discriminator at translate)
- **Nodes** — 4 parked passthroughs confirmed parked (zero canonical consumers); live fields consumed ✓
- **Particles** — converged ✓ (single `apply_emitter_params`; emissive/color/size no-ops honored)
- **Collision** — **no-leak VIOLATED**: 2 parsed `bhk*Shape` blocks drop silently at the resolve fallback

**Tier-invariant violation count**: single-boundary 0 · no-fabrication 0 · **no-leak 2** (D6-01, D6-02)
· no-render-time-fallback 0. Plus one completeness-signal coverage gap (D7-A).

**Severity tally**: 0 CRITICAL · **2 HIGH** (D6-01, D6-02) · **1 MEDIUM** (D7-A) · **6 LOW**.

The two HIGH findings share a root: **#1329 (CLOSED) fixed the parser-dispatch gap for
`BhkConvexSweepShape` + `BhkMeshShape` so they now land in the scene, but did not add resolve
arms — so the collision leak migrated from the parser tier to the canonical tier.** This is
exactly the leak class NIFAL exists to catch (bytes read fine, data dropped downstream).

---

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | ✅ | ✅ | ✅ | ✅ | `material_translate::translate_material` |
| Geometry / Transform | ✅ | ✅ | ✅ | ✅ | import extractors → `MeshRegistry::upload`; `import/coord.rs` |
| Skinning | N/A | ✅ | ✅ | N/A | `import/mesh/skin.rs` (global remap at extraction) |
| Lights | N/A | ✅ | ✅ | N/A | `import/walk/mod.rs::walk_node_lights` → `LightKind` |
| Nodes | N/A (by design, spec §2) | ✅ | ✅ (4 parked, 0 consumers) | N/A | dual spawn paths (documented) |
| Particles | ✅ | ✅ | ✅ | N/A | `systems/particle.rs::apply_emitter_params` |
| **Collision** | ✅ | ✅ | **❌ (D6-01, D6-02)** | N/A | `import/collision.rs::resolve_shape` |

Cross-cutting verdict (Dim 7): single-boundary / no-fabrication / no-render-time-fallback all
**PASS** across every category. `no-leak` passes everywhere except Collision.

---

## Findings

### D6-01: `BhkConvexSweepShape` parsed but has no resolve arm — authored collision silently dropped
- **Severity**: HIGH
- **Dimension**: Collision
- **Tier Violated**: no-leak (canonical-resolution completeness)
- **Game Affected**: Oblivion (NIF v10.0.1.0); any title authoring a convex-sweep collider
- **Location**: `crates/nif/src/import/collision.rs` `resolve_shape_inner` (no arm; falls through to the unsupported-shape `None` at ~L447-452). Block parsed at `crates/nif/src/blocks/collision/shape_compound.rs:204`, dispatched at `crates/nif/src/blocks/mod.rs:1104`.
- **Status**: NEW (#1329 CLOSED fixed only the parser-dispatch gap, not the resolve arm)
- **Description**: `BhkConvexSweepShape { shape_ref, material, radius }` is a wrapper shape carrying a child `shape_ref`. There is no `downcast_ref::<BhkConvexSweepShape>()` arm in `resolve_shape_inner`, so the block reaches the unsupported-shape fallback and returns `None` — the wrapped child shape is never resolved. #1329 added the parser arm (block now lands in the scene), but the collider that was previously lost to file truncation is now lost to the resolve fallback instead.
- **Impact**: Oblivion convex-sweep colliders silently vanish — the mesh renders but has no collision.
- **Suggested Fix**: mirror the `BhkMoppBvTreeShape` recurse-into-wrapped-shape arm:
  `if let Some(s) = block.as_any().downcast_ref::<BhkConvexSweepShape>() { return resolve_shape(scene, s.shape_ref, visited); }` (the `radius` convex-inflation is a refinement).

### D6-02: `BhkMeshShape` parsed but has no resolve arm — Oblivion mesh collision silently dropped
- **Severity**: HIGH
- **Dimension**: Collision
- **Tier Violated**: no-leak (canonical-resolution completeness)
- **Game Affected**: Oblivion (TES4) — the block exists only at v10.0.1.0
- **Location**: `crates/nif/src/import/collision.rs` `resolve_shape_inner` (no arm). Block parsed at `crates/nif/src/blocks/collision/shape_mesh.rs:216`, dispatched at `crates/nif/src/blocks/mod.rs:1105`.
- **Status**: NEW (same #1329 caveat as D6-01)
- **Description**: `BhkMeshShape { radius, scale: [f32;4], data_refs: Vec<BlockRef> }` where `data_refs` reference `NiTriStripsData` — directly resolvable as a TriMesh exactly like `BhkNiTriStripsShape` (which `resolve_tri_strips_collision` at ~L456 already handles). No resolve arm exists, so the block drops at the unsupported fallback despite the geometry being fully readable with existing code.
- **Impact**: Oblivion mesh-collision (the analogue of the handled `BhkNiTriStripsShape`) silently dropped.
- **Suggested Fix**: add an arm that builds a strips merge over `s.data_refs` — refactor `resolve_tri_strips_collision` to take `&[BlockRef]` + scale, or inline the same loop. Fold `BhkMeshShape.scale` (`[f32;4]`, per-axis) in alongside `havok_scale`.

### D7-A: Translation-completeness harness is blind to Starfield (0 meshes) and silently omits FO76
- **Severity**: MEDIUM
- **Dimension**: Completeness
- **Tier Violated**: completeness-signal (cross-cutting)
- **Game Affected**: Starfield (critical), FO76 (omitted)
- **Location**: `crates/nif/tests/translation_completeness.rs:199-205` (hard-coded 5-game `games` array)
- **Status**: NEW (#1320 / TH6-NEW-02 covers only the empty fill-rate thresholds, not the missing-games coverage)
- **Description**: `cross_game_translation_completeness` iterates only Oblivion/FO3/FNV/SkyrimSE/FO4. `common::Game` + `open_mesh_archive` fully support FO76 + Starfield and both data dirs are installed, yet the harness — whose entire purpose is to be the cross-game coverage signal that catches unverified-game leaks — excludes the two games most likely to harbor one. A throwaway probe through the identical `collect_stats` logic measured FO76 = 293 meshes / healthy fill (pure omission, would pass) and **Starfield = 0 imported meshes from 200 parsed NIFs**. Starfield's 0 is real: its geometry is external `.mesh` companion files (#1292) and the harness calls the no-resolver `import_nif`, so `extract_bs_geometry` (`crates/nif/src/import/mesh/bs_geometry.rs:54`, `let resolver = resolver?;`) returns `None` for every external LOD. The completeness signal therefore **cannot see any regression in the Starfield translation path**.
- **Impact**: A future break in Starfield BSGeometry / `.mat` / SkinAttach translation would be invisible to the one harness designed to catch it.
- **Suggested Fix**: (1) add FO76 + Starfield to the `games` array (FO76 is free); (2) for Starfield, run `import_nif_with_resolver` with a `MeshResolver` backed by the `Starfield - Meshes01.ba2` + `geometries` chain (the resolver impl already exists for the cell loader, SF-D4-02 Stage B), OR add an explicit `inline-geometry-only` Starfield row + comment so the 0% isn't mistaken for a regression.

### D6-03: `BhkAabbPhantom` not resolved while sibling `BhkSimpleShapePhantom` is — phantom-handling inconsistency
- **Severity**: LOW
- **Dimension**: Collision
- **Tier Violated**: no-leak (consistency of the parked-phantom decision)
- **Game Affected**: Oblivion / FO3 / FNV / Skyrim+
- **Location**: `crates/nif/src/import/collision.rs` `resolve_shape_inner` — `BhkSimpleShapePhantom` resolves its inner shape (~L443) but `BhkAabbPhantom` (dispatched at `mod.rs:1159`, also carries a `shape_ref`) has no arm and drops.
- **Status**: NEW
- **Description**: Internal inconsistency, not a clean leak. Both are phantom (trigger-volume) subclasses. `extract_from_phantom` (~L228) deliberately returns `None` to avoid mis-promoting a trigger into a solid collider — yet the `BhkSimpleShapePhantom` arm *does* promote its inner shape when reached as a child. The two phantom subclasses are treated oppositely.
- **Suggested Fix**: pick one policy. Per the docstring's "don't mis-promote triggers into solid colliders" rationale, dropping both to `None` until the `TriggerVolume` ECS path lands is the documented intent (making the L443 arm the outlier to remove); document whichever is chosen.

### D5-03: `extract_emitter_rate` docstring claims FLT_MAX is rejected, but `sane()` doesn't reject it
- **Severity**: LOW
- **Dimension**: Particles
- **Tier Violated**: none (doc/code precision drift)
- **Game Affected**: all (latent; no vanilla content observed hitting it)
- **Location**: `crates/nif/src/import/walk/mod.rs:706-717`
- **Status**: NEW
- **Description**: The docstring says a "FLT_MAX sentinel … rejected", but `sane(r) = r.is_finite() && r >= 0.0` does NOT reject `f32::MAX` (it's finite + positive). The FLT_MAX bypass works only because the keyed-`NiFloatData` branch is preferred. A NIF authoring `value == FLT_MAX` with a NULL `data_ref` would return `Some(3.4e38)` as the rate — cap-spawning every frame.
- **Suggested Fix**: either tighten `sane()` to reject `r >= 3.0e38` (matching the FLT_MAX-sentinel convention used elsewhere in the codebase, e.g. the B-spline pose gate), or correct the docstring to state the sentinel is dodged by branch preference, not rejected.

### D7-B: Stale render-time-fallback doc comment on `ImportedMesh.metalness_override` (pre-#1346 architecture)
- **Severity**: LOW (doc rot)
- **Dimension**: Material / Completeness
- **Tier Violated**: none (comment only)
- **Game Affected**: all
- **Location**: `crates/nif/src/import/types.rs:456-472` (esp. 464-468)
- **Status**: NEW (closely related to closed D7-01 / #1346, which fixed the spec doc but missed this code comment)
- **Description**: The field doc still describes the deleted Option-leak architecture ("The renderer reads `Material.metalness_override` (preferred) before falling back to keyword classify_pbr…"). Post-#1346 the canonical `Material` has no `metalness_override` field (it's `metalness: f32`, resolved at the boundary), and the renderer reads `m.metalness` directly with no render-time classify_pbr fallback. The field itself (on the raw-tier `ImportedMesh`) is correct; only the prose re-describes the closed leak.
- **Suggested Fix**: rewrite to "read only at the `translate_material` boundary; the renderer never sees this field."

### D1-01: Stale `resolve_classifier_overrides` references in material-abstraction.md
- **Severity**: LOW (doc rot)
- **Dimension**: Material
- **Tier Violated**: none (comment only)
- **Game Affected**: all
- **Location**: `docs/engine/material-abstraction.md:143,147`
- **Status**: Existing: #1309 (CLOSED — partial landing; the two doc lines #1309 named were not fixed)
- **Description**: The symbol `resolve_classifier_overrides` was renamed to `resolve_pbr`; the code reference was fixed under #1309 but these two doc lines were missed (independently re-confirmed stale by `AUDIT_NIF_2026-05-29.md` and `AUDIT_OBLIVION_2026-05-28.md`).
- **Suggested Fix**: `s/resolve_classifier_overrides/resolve_pbr/` at both lines; consider reopening #1309 or filing a follow-up.

### D2-NEW-01: nifal.md attributes SVD rotation repair to `transform.rs`; it lives in `rotation.rs`
- **Severity**: LOW (doc rot)
- **Dimension**: Geometry / Transform
- **Tier Violated**: none (doc citation)
- **Game Affected**: all
- **Location**: `docs/engine/nifal.md:80-81`
- **Status**: NEW
- **Description**: The spec attributes degenerate-rotation SVD repair to `transform.rs`, but it actually lives in `crates/nif/src/rotation.rs` (`sanitize_rotation` / `repair_rotation_svd_or_identity`), called once at parse from `stream.rs:624/647`. `transform.rs::compose_transforms` does no repair and explicitly assumes sanitized rotations. The "repair fires once" invariant HOLDS — only the file citation is wrong.
- **Suggested Fix**: update the nifal.md §2 Geometry citation `transform.rs` → `rotation.rs`.

### D2-NEW-02: Defensive second SVD path in `coord.rs::zup_matrix_to_yup_quat` is effectively unreachable
- **Severity**: LOW (informational / dead-defensive-code)
- **Dimension**: Geometry / Transform
- **Tier Violated**: none
- **Game Affected**: all
- **Location**: `crates/nif/src/import/coord.rs` (`svd_repair_to_quat` inside `zup_matrix_to_yup_quat`)
- **Status**: NEW
- **Description**: A second SVD-repair path inside the Z-up→Y-up quaternion conversion is unreachable in production — inputs are already sanitized at parse and the axis-swap preserves determinant (det=+1). Reachable only from direct unit-test callers. Not a correctness leak; flagged so a future reader doesn't mistake it for a live second repair site (which would contradict the "repair once at parse" invariant).
- **Suggested Fix**: leave as-is (harmless defense) or add a comment that it's a test-only fallback; not action-required.

---

## Documented-limitation ledger (parked-not-leak — do NOT re-report next sweep)

- **Node passthroughs** (Dim 4 — all 4 confirmed PARKED, zero canonical consumers): `bs_value_node` (→ M35 LOD selector), `bs_ordered_node` (→ `RenderOrderHint` + sort key), `tree_bones` (→ SpeedTree wind/bend), `range_kind` (→ destructible/blast/debris systems). Each captured on the raw `ImportedNode`; blocked on a not-yet-existing consumer feature. When that feature lands, its slice translates the parked field (no parser change needed).
- **Collision FO4+ NP blob** (`BhkNPCollisionObject`): Havok-serialised blob; decoder is a separate project. Consumer falls back to `cell_loader/spawn.rs::synthesize_static_trimesh` for Architecture. Discriminator intact. NOT a leak.
- **Collision phantoms** (`BhkPCollisionObject`): Skyrim+ trigger volumes; need a `TriggerVolume` ECS path, not a rigid body. Discriminator intact. NOT a leak. (D6-03 is about the *inconsistency* with `BhkSimpleShapePhantom`, not the parked status.)
- **Particle size-over-life curve** (Dim 5): the grow→steady→fade bell shape can't map to the linear `start_size → end_size`; only the authored *magnitude* (`initial_radius × base_scale`) is translated. Documented future work, NOT a leak.
- **Particle `initial_color`** (Dim 5): intentionally NOT applied — colour is owned by the `color_curve` override. Applying the white nif.xml default would be a no-fabrication-in-reverse regression.
- **Emissive normalization** (Dim 1, spec §4): the three `EmissiveSource` variants share a measured ~1.0 scale; NO normalization is applied or wanted. A future "emissive normalization constant" is a no-fabrication violation. Open question Q2 in material-abstraction.md is resolved no-op.

---

## Regression Guards (verified HOLD this sweep)

- Material: single boundary (`translate_material`, exactly 2 callers), plain-f32 PBR, glass-once-after-resolve, emissive no-op, `resolve_pbr` idempotent. Recent fixes #1346/#1350/#1352/#1353 all hold.
- Geometry: three per-game decoders converge to one `Vec<[f32;3]>` + `Vec<u32>` (Y-up); coord conversion single-source (#1044); renderer format-agnostic.
- Skinning: `ImportedSkin` global bone indices (#613), u16-range warning intact, no downstream partition re-derivation.
- Lights: source-block discriminator collapsed to `LightKind` at one site (`walk_node_lights`); no consumer matches source block type.
- Collision: MultiSphere→Compound-of-Balls + ConvexList→Compound regression pins hold; `havok_scale` single-application (3 reads, all in collision.rs; no consumer re-applies).
- Particles: typed `NiPSysEmitter`/`Ctlr`/`CtlrData`/`GrowFadeModifier` blocks; `read_emitter_base` reads (not skips); both spawn paths route through `apply_emitter_params`.
