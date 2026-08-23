# NIFAL Audit — 2026-08-23

Scope: all 9 dimensions, all games. Preset: `nif-deep` (part of
`/audit-suite --preset nif-deep`).

## Executive Summary

**3 NEW findings this sweep: 2 HIGH, 1 MEDIUM** (plus 1 LOW self-referential
doc-rot in the audit skill file itself). This is a materially more active
sweep than the immediately preceding ones — both HIGH findings are real,
previously-unflagged canonical-boundary defects surfaced by re-deriving
against current source rather than trusting prior PASS marks, and one MEDIUM
is a genuine (currently dormant) latent bug in the in-progress #3231
morph-target work.

⚠️ **Two HIGH findings, no render-time fallback masks either one:**
- **NIFAL-D3-2026-08-23-01** — placed NIF-embedded spot/directional lights
  aim in local space, not world space, on any REFR with non-identity rotation.
- **NIFAL-D8-2026-08-23-01** — a second, non-canonical BGSM→texture-role
  resolver binds the smoothness mask into the specular-colour role, and its
  reachability was *widened* by today's `900aa081` (#973 per-shape MSWP)
  commit.

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Notes |
|---|---|---|---|---|---|
| Material (Dim 1) | PASS (3 callers, all through `translate_material`) | PASS | PASS | PASS | 6 pre-existing open issues re-confirmed, not re-reported (#3073, #2330, #2572, #2573, #2687, #2641) |
| Geometry/Transform (Dim 2) | PASS | PASS | PASS | PASS | Cleanest category, re-verified not just trusted; new `morph_targets` field noted as parked, tracked by #3231 |
| Skinning (Dim 3) | PASS | — | documented gap (#2440, closed via doc, not code) | — | unchanged |
| Lights (Dim 3) | PASS (both load paths → `spawn_nif_lights` / `translate_light`) | **FAIL — NEW** | — | — | see NIFAL-D3-2026-08-23-01 |
| Nodes (Dim 4) | N/A by design | — | PASS (7 parked fields, 0 canonical consumers) | — | unchanged |
| Particles (Dim 5) | PASS (`apply_emitter_overlays`) | PASS | PASS | PASS | #2610 existing (hardcoded `effect_shader_flags: 0`), not re-reported |
| Collision (Dim 6) | PASS | PASS | PASS | — | 16/16 `bhk*Shape` resolve arms confirmed via automated `dispatch_coverage_tests` |
| Animation (Dim 7) | PASS (`convert_nif_clip`) | — | **FAIL — NEW (dormant)** | — | see NIFAL-D7-2026-08-23-01 |
| Shader flags/texture roles (Dim 8) | **FAIL — NEW** (second resolver diverges) | — | **FAIL — NEW** | PASS (zero `if game ==` in shaders) | see NIFAL-D8-2026-08-23-01 |
| Completeness signal (Dim 9) | — | — | — | PASS | harness itself needs updating (NIFAL-D9-01) |

## Findings

### NIFAL-D3-2026-08-23-01 — `spawn_nif_lights` rotates light position but not direction (HIGH)

- **Dimension**: Skinning/Lights · **Tier Violated**: no-fabrication (wrong
  canonical value from the single boundary, no fallback) · **Game Affected**:
  all games via the shared `spawn_nif_lights` boundary; manifests only on the
  cell-loaded path (loose-NIF path always passes `ref_rot = IDENTITY`)
- **Location**: `byroredux/src/cell_loader/spawn.rs:878-940` — position is
  correctly composed with `ref_rot` at line 909
  (`GlobalTransform::compose_translation`), but `light.direction` is passed
  straight to `LightSource::from_legacy_world_units` at line 936 with no
  rotation applied at all, and the spawned entity's own rotation is hardcoded
  `Quat::IDENTITY` (line 925) — nothing downstream gets a second chance to
  apply it.
- **Description**: The sibling ESM-LIGH boundary
  (`byroredux/src/systems/light_anim.rs::translate_light`, from the same
  #2439 fix cycle) already does this correctly:
  `let direction = (ref_rot * Vec3::new(1.0, 0.0, 0.0)).to_array();` — the
  established pattern simply was never applied to the older,
  structurally-distinct direct-embedded-NiLight boundary when
  kind/direction/outer_angle were wired through by #2205.
- **Impact**: Any placed REFR with non-identity rotation whose own mesh
  authors a `NiSpotLight`/`NiDirectionalLight` renders that light aimed in
  NIF-local space — pointed into a wall, away from its intended surface, or
  only coincidentally correct. Measured real-content carrier: the #2205
  investigation found 95 `NiDirectionalLight` blocks in Oblivion's
  `Meshes.bsa` alone (vines, statues, hair/ear kits — routinely placed at
  arbitrary rotations). Ambient/point lights unaffected (no direction); NPCs
  and their equipped lights unaffected (route through the correct
  `translate_light` boundary).
- **Suggested Fix**: `let world_direction = (ref_rot *
  Vec3::from_array(light.direction)).to_array();` in `spawn_nif_lights`,
  mirroring `translate_light`. `kind`/`outer_angle` need no correction.

### NIFAL-D7-2026-08-23-01 — Morph-target index space desyncs between the weight channel and the vertex-delta array (MEDIUM, → HIGH once a GPU consumer lands)

- **Dimension**: Animation/controllers · **Tier Violated**: no-leak · **Game
  Affected**: any FaceGen/`NiGeomMorpherController` content where a target's
  `.vectors.len()` mismatches the shape's vertex count (Skyrim+/FO4 FaceGen
  head morphs are the primary real-world carrier)
- **Location**: `crates/nif/src/import/mesh/morph.rs:60-97`
  (`extract_morph_targets`, new in `c1339301`) vs.
  `crates/nif/src/anim/channel.rs:16-30` (`resolve_morph_target_index`,
  existing since #262)
- **Description**: Both fns derive a "morph target index" from the same
  `NiMorphData.morphs` array, and the `GpuInstance` doc comment added by
  `5f4dea46` explicitly intends the two to share one stable slot number.
  `resolve_morph_target_index` returns each target's original, unfiltered
  position. `extract_morph_targets` `continue`s past any vertex-count
  mismatch and `break`s at `MAX_MORPH_TARGETS_PER_MESH` — both correct,
  documented fail-soft behaviors for the delta array in isolation — but the
  resulting `Vec` compacts around the gap, so every target after a dropped
  one shifts down by one with nothing recording the original index.
- **Impact**: Currently **dormant** — `ImportedMesh.morph_targets` has zero
  consumers outside `crates/nif` today, and `GpuInstance.morph_delta_address`
  /`morph_weight_address` are hardcoded to `0` per `5f4dea46`'s own commit
  message ("still a placeholder-zero follow-up"). The bug is already baked
  into the two landed pieces and will silently misapply facial-morph weights
  (or blend the wrong slider) the instant the announced GPU-consumer
  follow-up in #3231 wires the delta/weight buffers by this index — on
  exactly the malformed-vertex-count content the guard exists to make safe.
  The existing `drops_target_with_mismatched_vertex_count` test only asserts
  the surviving target's name, not its index alignment, so it cannot catch
  this.
- **Related**: Fold into #3231's follow-up work rather than file standalone.
- **Suggested Fix**: Give `ImportedMorphTarget` an explicit `original_index:
  u32`, or make `extract_morph_targets` emit a fixed-size
  `Vec<Option<ImportedMorphTarget>>` so position IS the stable index by
  construction. Land before or alongside #3231's GPU-consumer phase.

### NIFAL-D8-2026-08-23-01 — `fill_from_bgsm`'s `.bgsm` arm binds the smoothness mask into the `specular` role, never reads the real `specular_texture` field (HIGH)

- **Dimension**: Shader-flags/Effects (texture-role vocabulary) · **Tier
  Violated**: no-leak + single-boundary (a second BGSM→role resolver
  disagreeing with the canonical one) · **Game Affected**: FO4/FO76/Starfield
- **Location**: Defect at `byroredux/src/cell_loader/refr.rs:246-255`
  (`fill_from_bgsm`, `.bgsm` arm). Correct comparison point:
  `byroredux/src/asset_provider/material.rs:1331-1338` +`:1396-1406`
  (`merge_external_material`, the canonical boundary, keeps `smooth_spec` and
  `specular` distinct exactly per the role docs in
  `crates/nif/src/import/types.rs:314-324`). Gate that lets the wrong value
  reach a live role: `slot_role.rs:342`
  (`(TextureSlotLayout::Fallout4, 7) => Some(TextureRole::Specular)`,
  unconditional since #2998/#3186).
- **Description**: BGSM defines two distinct fields — `smooth_spec_texture`
  (always read, "smoothness in alpha, specular RGB") and `specular_texture`
  (`version > 2` only, "standalone specular, PBR-style separate"). The
  canonical `merge_external_material` boundary keeps them separate.
  `RefrTextureOverlay::fill_from_bgsm` — the second resolver that lets REFR
  `XATO`/`XTNM` overrides and, since today's `900aa081` (#973), per-shape
  `XMSP` material swaps reach texture roles — routes
  `smooth_spec_texture` into `self.specular` and never reads
  `specular_texture` at all in its `.bgsm` arm (the `.bgem` arm is correct;
  BGEM has only one `specular_texture` field). `smooth_spec_texture` is read
  unconditionally at every BGSM version, so this is the base/legacy slot, not
  a rare edge case. The regression-test fixture
  (`refr_texture_overlay_tests.rs:507-561`,
  `fill_from_bgsm_forwards_every_bgsm_texture_role`) currently pins the wrong
  behavior as correct — no fixture in the file ever sets `specular_texture`,
  so the dropped-field half of the bug has zero coverage in either direction.
- **Impact**: Any FO4/FO76/Starfield REFR resolving through `fill_from_bgsm`
  gets its specular-colour channel bound to a grayscale-ish smoothness mask
  (sampled as an RGB tint via `specColor *= texture(...).rgb` at
  `triangle.frag:383-388`) instead of modulating roughness via
  `glossMapIndex`, and loses its authored standalone specular-colour map
  outright when the BGSM authors one. Today's `900aa081` **widens
  reachability** from a single REFR-level `material_path` to every shape of a
  multi-shape mesh whose MSWP swap target is a `.bgsm` ("Raider armour colour
  variants, station-wagon rust patterns, Vault decay overlays" per that
  commit's own description). No render-time fallback masks this.
- **Suggested Fix**: Add `Self::fill(&mut self.specular,
  Some(f.specular_texture.as_str()), pool)` reading the correct field in the
  `.bgsm` arm; either add a `smooth_spec` field to `RefrTextureOverlay` for
  the gloss mask, or drop the `smooth_spec_texture` read entirely if a
  REFR-level gloss override is out of scope. Then fix the test fixture to set
  both fields to different values and assert each lands in its own role.

### NIFAL-D9-2026-08-23-01 — Cross-game completeness harness's metalness/roughness floors are stale against the #2707 no-fabrication fix (MEDIUM)

- **Dimension**: Completeness/cross-cutting · **Location**:
  `crates/nif/tests/translation_completeness.rs` (7 per-game `>= 99.9`
  floors for `metO`/`rghO`) vs.
  `crates/nif/src/import/material/mod.rs:1415-1424`
- **Description**: The floors (added 2026-08-06, #2304-#2307) assumed the
  constructor sets both fields unconditionally — invalidated a week later by
  `593ab134` (#2705-#2708/#2707), which correctly gates both on
  `has_no_pbr_classifier_signal()` so genuinely signal-less content defers to
  `Material::resolve_pbr`'s NaN-sentinel backstop instead of fabricating a
  guess. Live run confirms the hard-fail: FO3 94.3%, FNV 96.7%, SkyrimSE
  93.8%, FO4 99.4%, FO76 18.2%, Starfield 5.1% — all below the stale 99.9%
  floor (only Oblivion, at 100%, passes). Every other floor in the same
  closures passed with margin.
- **Impact**: The harness is opt-in/`--ignored`, so this can go unnoticed for
  a long time; the real risk is a well-intentioned "fix" that reverts
  `metalness_override`/`roughness_override` to unconditional `Some(..)` to
  make the assertion pass again — which would silently undo the #2707
  no-fabrication correctness fix.
- **Suggested Fix**: Lower the floors per-game to measured post-#2707 values
  with the same margin convention already used for every other metric in the
  file, and correct the "100% by construction" comment block to cite #2707.

### NIFAL-D9-2026-08-23-02 — `audit-nifal/SKILL.md` itself still claims "exactly two" `translate_material` callers (LOW, doc-rot in the audit tooling)

- **Location**: `.claude/commands/audit-nifal/SKILL.md:109`. Live count is
  three: `scene/nif_loader.rs:1013`, `cell_loader/spawn/mesh_instance.rs:753`,
  `cell_loader/placement_lod.rs:514`. Distinct from the already-open
  `NIFAL-D9-2026-08-16-01` (same 3-vs-2 count, but against `nifal.md`, a
  different document not touched by that fix).
- **Suggested Fix**: Update the SKILL.md line to name all three callers and
  say "MUST all route through this one fn," matching the phrasing already
  used correctly for the Particles/Animation dimensions.

## Documented-Limitation Ledger (re-verified this cycle, not re-reported)

- Skinning cell-loader gap (#2440, closed via documentation not code fix —
  cell-placed skinned geometry still renders in bind pose).
- Node passthroughs (`bs_value_node`, `bs_ordered_node`, `tree_bones`,
  `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`) — all still
  zero canonical consumers.
- `BhkNPCollisionObject`/`BhkPCollisionObject` phantom/packed-Havok
  limitations — unchanged, still accurately documented.
- `#2610` — particle `DrawCommand.effect_shader_flags` still hardcoded `0`.
- `#3187` — `RefrTextureOverlay::apply_slot_swap` is still a game-agnostic
  flat slot table (FO4 slot-5 reads the wrong lane); unrelated to
  NIFAL-D8-2026-08-23-01 above but in the same file/function family.

## Prioritized Fix Order

1. **NIFAL-D8-2026-08-23-01** (HIGH) — live, widened-reachability texture bug on FO4-family content shipping today.
2. **NIFAL-D3-2026-08-23-01** (HIGH) — live lighting-direction bug, silent, no fallback.
3. **NIFAL-D7-2026-08-23-01** (MEDIUM, dormant) — land the index-preservation fix before/alongside #3231's GPU-consumer phase, not after.
4. **NIFAL-D9-2026-08-23-01** (MEDIUM) — test-hygiene, but risks reverting a real correctness fix if mishandled.
5. NIFAL-D9-2026-08-23-02 (LOW) — doc fix.

Suggest: `/audit-publish docs/audits/AUDIT_NIFAL_2026-08-23.md`
