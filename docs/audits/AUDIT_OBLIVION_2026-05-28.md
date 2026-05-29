# Oblivion (TES4) Compatibility Audit — 2026-05-28

**Scope**: full 7-dimension `/audit-oblivion` (NIF v20.0.0.5 parser, BSA v103, ESM coverage, Oblivion shader path, real-data validation, blockers, NIFAL material translation).
**Method**: per-dimension deep find (3 specialist + 4 general agents) → cross-dim dedup → adversarial per-finding verification against the current tree + real `Oblivion.esm` (277 MB) and `Oblivion - Meshes.bsa` (8032 NIFs). Ran in two passes: pass 1 covered Dims 2/3/5/6; the Dim 1/4/7 finders dropped their structured output and were re-run in pass 2. **14 findings confirmed, 5 refuted, 77 invariants verified clean.** All empirical claims reproduced against shipping data; code patched-and-reverted to measure recovery deltas (working tree restored clean).

## Executive Summary

Oblivion is in materially better shape than the ROADMAP claims:

- **NIF parse**: ~96.2% clean baseline (7835/8032). **Two HIGH parser bugs (`OBL-D1-01/02`) truncate ~43 meshes** via a phantom version-gated bool on `NiTriShapeData`/`NiTriStripsData` at v10.0.1.0/10.0.1.2 — fixing the gate (`V10_0_1_0`→`V10_0_1_3`) lifts clean to ~7878. A third HIGH (`OBL-D1-03`) corrupts `NiGeomMorpherController` morph weights on mainstream v20.0.0.4/5 facial-morph content.
- **BSA v103**: clean end-to-end (regression guard held — the old "v103 broken" framing stays dead). No findings.
- **ESM**: parses; three coverage gaps (`OBL-D3-*`) — a *mislabeled* INFO `response_type` (actually `EmotionType`, real response number dropped), 36-byte XCLL fog fields dropped, DIAL dialogue-type unparsed. Latent (no live consumer yet) but they enshrine wrong semantics.
- **Render end-to-end**: interiors *and exteriors* render today (`OBL-D6-NEW-01` — the ROADMAP "exterior blocked on TES4 worldspace + LAND" framing is itself stale; Tamriel grid 0,0 r1 = 9 cells / 4886 entities / 150.6 FPS on RTX 4070 Ti). Residual render-completeness gaps: **worldspace-default ocean water never renders** (`OBL-D6-NEW-02`), **Oblivion content renders with no normal maps** (`OBL-D4-NEW-01`, the highest-visual-impact item), and FX emitters drop on truncated particle NIFs (`OBL-D6-NEW-03`).
- **NIFAL material translation (Dim 7)**: the canonical contract holds for Oblivion — single `translate_material` boundary, plain-`f32` PBR resolved once, `EmissiveSource::Material`, `MAT_FLAG_PBR_BSDF` stays 0 (Disney lobe unreachable). One doc-rot link (`OB-D7-001`).

**Top priorities**: (1) `OBL-D1-01`+`OBL-D1-02` (HIGH, ~43 meshes, one-line gate fix each); (2) `OBL-D4-NEW-01` (HIGH, normal maps — largest visual gap); (3) `OBL-D1-03` (HIGH, morph corruption); (4) `OBL-D6-NEW-02` (ocean water).

## Findings

### HIGH

#### OBL-D1-01 — `NiTriStripsData` reads a phantom `has_points` bool at v10.0.1.0/10.0.1.2
- **Dim 1** · `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:545`
- **Issue**: gate is `version >= V10_0_1_0`; nif.xml has `Has Points` as `since="10.0.1.3"` (and nifly `StripsInfo::Sync` reads it only `>= V10_0_1_3`). At v10.0.1.0/10.0.1.2 a 1-byte field that isn't on disk is consumed, shifting the stream; with no Oblivion block-size table the whole NIF tail truncates.
- **Empirical**: `>=V10_0_1_3` patch → truncated 197→175 (22 files recovered, 429 fewer dropped blocks) on the real Meshes BSA.
- **Fix**: gate `>= NifVersion::V10_0_1_3` (constant exists, version.rs:73) + a v10.0.1.2 / num_strips>0 regression test.

#### OBL-D1-02 — `NiTriShapeData` reads a phantom `has_triangles` bool (same wrong boundary)
- **Dim 1** · `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:483`
- **Issue**: same defect class for the flat-triangle block; `Triangles` is unconditional `until="10.0.1.2"` and cond-gated `since="10.0.1.3"` (OpenMW `data.cpp:182` reads the bool only `> VER_OB_OLD` = `>10.0.1.2`). Misaligns triangle list + num_match_groups → tail truncation.
- **Empirical**: this fix alone recovers a further 21 files (truncated 175→154).
- **Fix**: gate `>= NifVersion::V10_0_1_3`.

#### OBL-D1-03 — `NiGeomMorpherController` reads 8-byte MorphWeight unconditionally; pre-v20.1.0.3 has no per-element float
- **Dim 1** · `crates/nif/src/blocks/controller/morph.rs:37-45`
- **Issue**: on mainstream Oblivion v20.0.0.4/5 morph content (facial morphs, animated gates) the interpolator-ref/weight stride is wrong — refs misassigned, weights are garbage floats reinterpreted from block-ref bytes.
- **Fix**: read the per-element `weight` f32 only when `version >= V20_1_0_3` (constant exists, version.rs:121), else 1.0/none; extend the existing `nigeommorpher_oblivion_consumes_*` test to pin the boundary.

#### OBL-D4-NEW-01 — Oblivion renders with no normal maps (no `_n.dds` load-time convention)
- **Dim 4** · `crates/nif/src/import/material/walker.rs:596-601`; `byroredux/src/asset_provider.rs` (no implicit derivation); `byroredux/src/render/static_meshes.rs:179-183`
- **Issue**: `normal_map = normal_texture.or_else(bump_texture)`, but ~all Oblivion `NiTexturingProperty` leave both slots empty (Oblivion ships normal maps by the `<base>_n.dds` filename convention, not as a NIF texture slot). With `normal_map_index==0` everywhere, the shader's TBN perturbation is bypassed → every Oblivion surface lit by its flat vertex normal. Largest visual regression vs the original engine in an RT-PBR pipeline.
- **Fix**: implement the Bethesda load-time convention — when a NIF arrives with a base texture but no normal_map, derive `<base_stem>_n.dds` and bind it if present in the archive/texture provider.

### MEDIUM

#### OBL-D3-…-01 — INFO `TRDT` byte[0] captured as `response_type` is actually `EmotionType`; real response number (offset 12) dropped
- **Dim 3** · `crates/plugin/src/esm/records/misc/ai.rs:300-304,343-345`
- **Issue**: `out.response_type = sub.data[0]` is the low byte of `EmotionType` (OpenMW `TargetResponseData`: emoType@0, emoValue@4, unknown@8, responseNo@12). Verified on real `Oblivion.esm`: all 23,877 `TRDT` subrecords are 16 bytes; byte[0] histogram `{0:9634,1:3288,2:1964,3:1568,4:1444,5:4475,6:1504}` == the emoType histogram (0–6 = Neutral..Surprise). The real responseNo@12 (`{1:18956,…,10:44}`) is never read → unrecoverable. The unit test `tests.rs:100/168` (`TRDT=[3,0,0,0]`→`response_type==3`) enshrines the wrong semantics (3 = EMO_Fear).
- **Fix**: rename to `emotion_type: u8`; also capture `response_number = sub.data[12]` when `len>=13`; fix the doc + test.

#### OBL-D3-…-02 — 36-byte Oblivion XCLL drops `fogDirFade`/`fogClipDist` via a `len >= 40` decode gate
- **Dim 3** · `crates/plugin/src/esm/cell/walkers.rs:508-516`
- **Issue**: same defect class as the just-fixed Starfield #1291 XCLL-size bug, for Oblivion's 36-byte body. The size table accepts 36 (no warn) but the field-decode gate requires `>=40`, so `dir_fade`(@28) + `fog_clip`(@32) are `None`. OpenMW `loadcell.cpp:185` reads exactly 36 for TES4.
- **Fix**: split the gate — `if len>=36 { dir_fade@28, fog_clip@32 } ; if len>=40 { fog_power }`; fix the `walkers.rs:15-16` doc.

#### OBL-D6-NEW-01 — ROADMAP "exterior blocked on TES4 worldspace + LAND wiring" is stale; exterior renders end-to-end today
- **Dim 6** · `ROADMAP.md:161,229,304` vs `byroredux/src/cell_loader/exterior.rs:54-365` + `crates/plugin/src/esm/cell/wrld.rs:15-454`
- **Issue**: that work shipped (WorldspaceRecord #965, exterior cell loader, LAND→terrain). Empirical: Tamriel grid 0,0 r1 = 9 cells / 4886 entities / 150.6 FPS. The ROADMAP still lists the parse/wiring as the top open blocker.
- **Fix**: update ROADMAP:161/229/304 to "Oblivion exterior renders end-to-end; residual gaps are render-completeness (ocean water OBL-D6-NEW-02, normal maps OBL-D4-NEW-01)."

#### OBL-D6-NEW-02 — Tamriel ocean never renders — worldspace-default water (NAM2/DNAM) has no cell-level fallback
- **Dim 6** · `byroredux/src/cell_loader/exterior.rs:259-278` (gates on `cell.water_height`); `crates/plugin/src/esm/cell/wrld.rs:120-123` (NAM2 stored on `record.water_form`, never propagated); `cell/mod.rs:684-715` (DNAM unparsed)
- **Issue**: every coastal/sea cell (Abecean, Niben Bay, Lake Rumare) renders dry seabed — the worldspace default water height/form isn't resolved as a per-cell fallback.
- **Fix**: parse WRLD DNAM (default land/water height); resolve worldspace `water_form`→WATR + default height once in `build_exterior_world_context`; fall back to it in `load_one_exterior_cell`.

#### OBL-D6-NEW-03 — Oblivion FX emitters drop from the render on truncated particle NIFs
- **Dim 6** · `crates/nif/src/blocks/particle.rs` (NiPSysData) → `blocks/mod.rs:927-939`; render-side drop at `cell_loader/references.rs:930` → `spawn.rs:378`
- **Issue**: fire/torch/brazier flames, smoke, fireflies, magic FX on the modern NiPSys path: when the NIF truncates before the emitter, `import_nif_particle_emitters` returns nothing and the FX silently doesn't spawn. The drift enters at the `NiPSysData`→`NiPSysAgeDeathModifier` boundary on v20.0.0.4/5 (the specific 4-byte under-read offset is not yet pinned — the narrower OBL-D1-05 modifier-mis-gate hypothesis was *refuted*, so the exact field remains to localize).
- **Fix**: pin the exact NiPSysData trailing field that under-reads at v20.0.0.4/5; add a regression test on a real Oblivion FX NIF.

### LOW

- **OBL-D3-…-03** (`ai.rs:269-284,314-332`) — DIAL `DATA` dialogue-type byte (Topic/Greeting/Combat/Service/…) unparsed for all 3817 Oblivion DIAL records. Fix: add `dial_type: u8 = sub.data[0]` (byte 0 is cross-game safe vs FO3+'s 4-byte DATA).
- **OBL-D4-NEW-03** (`import/walk/mod.rs:520-543,1182-1204`) — dead legacy-particle (`NiPSysBlock.original_type`) match arms + module/comment claims that contradict real Oblivion data (Oblivion uses modern `NiParticleSystem`, not the legacy stack). Tech-debt; delete or redirect to the typed structs.
- **OBL-D6-NEW-04** (`esm/reader.rs:256-277`) — single-plugin `Oblivion.esm` emits mod-index-01 FormIDs (a Bethesda authoring artifact) that resolve to nothing; pass-through leaves dangling cross-refs + per-form warn spam. Fix: recognize the index-01 case, clamp to self or tag engine-injected.
- **OB-D7-001** (`byroredux/src/material_translate.rs:59`; `docs/engine/material-abstraction.md:143,147`) — broken doc reference to `Material::resolve_classifier_overrides` (renamed to `resolve_pbr`). Same stale symbol the FO3/FO4 audits flagged. Fix: replace all three references.

### INFO

- **OBL-D4-NEW-04** (`crates/renderer/src/vulkan/pipeline.rs:246-255`) — stale comment claims #869 wireframe is unimplemented; it's fully wired through `PipelineKey` (`draw.rs:1589-1599`) + `triangle.frag:1034` flat-shading. Doc-rot only; update the comment.

## Blocker Chain (corrected)

Interiors **and exteriors render today** (the pre-#699 "BSA v103" framing and the "TES4 worldspace + LAND" framing are both dead — `OBL-D6-NEW-01`). The remaining chain to *visually-faithful* Oblivion is render-completeness, not bring-up:

1. **Normal maps** (`OBL-D4-NEW-01`, HIGH) — the `_n.dds` load-time convention; biggest visual gap.
2. **Parser truncation** (`OBL-D1-01/02`, HIGH) — recover ~43 meshes via the version-gate fix.
3. **Ocean water** (`OBL-D6-NEW-02`, MED) — worldspace-default water fallback.
4. **Morph corruption** (`OBL-D1-03`, HIGH) — facial-morph stride fix.
5. **FX emitters** (`OBL-D6-NEW-03`, MED) — pin the NiPSysData under-read.

## Regression Guard List (verified still correct — 77 invariants checked)

Confirmed holding: BSA v103 opens + extracts cleanly (dead-framing stays dead); `NiTexturingProperty` reads u32 count directly (no bool gate); `BSStreamHeader` cond = `version==10.0.1.2 || user_version>=3`; `user_version` threshold 10.0.1.8; no-block-size path doesn't rely on `block_size`; `as_ni_node` unwraps every NiNode subclass; pre-Gamebryo v3.3.0.13 → empty NifScene + debug log; `BhkMultiSphereShape`/`BhkConvexListShape` now translate to `CollisionShape`. **NIFAL (Dim 7)**: single `translate_material` boundary (both spawn callers route through it); `Material.metalness/roughness` plain `f32`, resolved once by `resolve_pbr`, no per-draw `classify_pbr` method; `EmissiveSource::Material` for Oblivion legacy; `MAT_FLAG_PBR_BSDF` empirically 0 across the all-legacy Oblivion material universe (Disney lobe unreachable). NIMaterialProperty raw monitor-space color; `NiAlphaProperty` blend routing; `NiWireframeProperty`→LINE + `NiShadeProperty.flat_shading` consumed.

## Refuted / cleared (5 — do not re-report)

- **OBL-D5-01** — `NiGeometryData` group-id gate (#326, `ni_tri_shape.rs:320-326`) is correct as-is (nif.xml-faithful); do not change.
- **OBL-D5-02** — Oblivion magic-effect particle truncation is real but root-caused to the NiPSysEmitter base parser, not a separate NiParticleSystem under-consume (folds into `OBL-D6-NEW-03`).
- **OBL-D1-04** — `NiControllerSequence` ControlledBlock string-palette gate is *not* mis-gated at v10.1.0.106.
- **OBL-D1-05** — the specific `NiPSysPositionModifier`/`NiPSysBoundUpdate` v20.0.0.4 drift hypothesis does not hold (the FX-drop root cause `OBL-D6-NEW-03` lies elsewhere in NiPSysData).
- **OBL-D4-NEW-02** — Oblivion particle emitters are NOT *systemically* dropped (only the truncated-NIF subset per `OBL-D6-NEW-03`).

## Methodology note

Pass 1 (Dims 2/3/5/6) wedged mid-Verify (orchestrator runtime died, 6 verifiers orphaned at 13/19) and was recovered via journal resume (cached results replayed, orphans re-run). Pass 2 re-ran Dims 1/4/7 after their specialist-agent finders dropped the structured-output call. All code patches made to measure recovery deltas were reverted (`git diff` clean).

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-05-28.md`
