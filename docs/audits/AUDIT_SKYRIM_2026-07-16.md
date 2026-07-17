# Skyrim Special Edition Compatibility Audit — 2026-07-16

**Audit type**: Regression coverage (working system). Skyrim SE is the
engine's renderer control bench — cell load and rendering both already work
(Whiterun BanneredMare, 6 named equipped NPCs). This audit is not readiness
scoping; it targets the Skyrim-specific risk surface (packed geometry,
shader-type dispatch, NPC equip/FaceGen, multi-master load order) plus
regression coverage of prior fixes.

**Dimensions run**: 7/7 (BSTriShape geometry, shader-type dispatch, NPC
equip+FaceGen, multi-master load order, BSA v105, specialty blocks +
rendering, NIFAL canonical translation).

**Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux` (200-issue
snapshot at `/tmp/audit/issues.json`; Dimension 3 additionally cross-checked
against the full 1941-issue open+closed history).

## Executive Summary

Six of seven dimensions came back clean or near-clean, consistent with
Skyrim's status as the best-tested per-game path in the engine (BSTriShape
geometry, BSA v105/LZ4, specialty-block dispatch + real-data rendering, and
the NIFAL canonical material boundary all verified CORRECT with zero new
findings). The one dimension with a structural gap is **NPC Equip + FaceGen
(Dimension 3)**: the Skyrim+ prebaked-FaceGen NPC spawn path has no body-mesh
fallback, and empirical extraction of a real vanilla FaceGeom NIF confirms it
ships head-only (no torso/limb geometry) — contradicting the code's own doc
comment. Cross-referencing the 6 named BanneredMare control-bench NPCs against
their real OTFT/CNTO data shows at least 2 of the 6 (Hulda, Mikael) resolve to
feet-only biped coverage today, meaning they would spawn as skeleton +
floating head + boots. This is a HIGH finding because it's not a modded or
edge-case scenario — it fires on the audit's own reference control bench and
the M41 equip smoke test would not have caught it (component-count gate only,
no geometry-completeness assertion). A second MEDIUM finding (Dimension 4)
identifies that the closed #1660 "deleted-REFR" fix only covers the
single-plugin load path — the multi-master merge path does not remove a
DLC-tombstoned base REFR, so it still renders. Remaining findings are LOW
(FO4 skin-tint-alpha data-completeness gap, un-rendered per-NPC face tint,
slot-displaced armor double-render, one stale audit-skill doc pointer).

## Findings Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 1 |
| MEDIUM   | 2 |
| LOW      | 3 |
| **Total NEW** | **6** |

Plus 1 existing open issue re-confirmed (not refiled): #1897.

---

## Dimension 1: BSTriShape Packed Geometry + SSE Skinned Reconstruction

**Verdict: CLEAN — 0 findings.**

Heavily-audited subsystem (prior fixes #559/#613/#638/#795/#796/#889/#1201/
#1202/#1204/#1516/#1559 all verified still landed and holding). All four
checklist items confirmed correct against live code and cross-checked against
`nif.xml`'s `BSVertexData` struct:

- VF_* flag bits (`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:194-243`)
  match the nif.xml `BSVertexDesc` bitfield exactly; `half_to_f32`
  (`crates/nif/src/import/mesh/decode.rs:18-46`) is a correct IEEE-754
  binary16 decode across all edge classes (zero/subnormal/Inf-NaN/normal).
- Flag-combination coverage, index stride (u16 on-disk, correctly widened),
  and skinned bone extraction all verified against the GPU skin pipeline
  (inline, SSE global-buffer, and NiSkinData-densify sources).
- The "chrome/magenta" SSE tangent regression guard holds: positions/normals
  Z-up→Y-up converted; the on-disk **bitangent** triplet is correctly routed
  as the Y-up tangent (∂P/∂U) with no double-swap between the inline and SSE
  producers; the #1559 separate-gate stride fix is intact.
- Alpha-property cascade (#1201/#1202): `alpha_property_consumed` is set
  exactly once per shape and consulted at both gate sites; skinned Skyrim
  geometry inherits its authored `NiAlphaProperty` exactly once.

Test evidence: `cargo test -p byroredux-nif` — 53 BsTriShape tests + half-float
edge-class test, all green.

---

## Dimension 2: BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch

**Verdict: 1 NEW LOW finding. 1 existing open issue re-confirmed (not refiled).**

### SK-D2-01: FO4 Skin Tint alpha (Shader Type == 5) parsed then discarded, never reaching MaterialInfo
- **Severity**: LOW
- **Dimension**: BSLightingShaderProperty shader-type dispatch (FO4 era)
- **Location**: `crates/nif/src/blocks/shader.rs:1412-1424` (`parse_shader_type_data_fo4`, type 5 arm)
- **Status**: NEW
- **Description**: For FO4 (BSVER 130-139), nif.xml gives Shader Type 5 (Skin
  Tint) both a `Color3` and a trailing `Skin Tint Alpha` float. The parser
  reads the alpha to keep the stream aligned (`let _skin_tint_alpha =
  stream.read_f32_le()?;`) but binds it to `_` and drops it — no stream
  drift, pure data loss. `ShaderTypeData::SkinTint` only carries `[f32;3]`.
  The FO76 sibling path (`Fo76SkinTint`, Color4) *does* preserve this field,
  so the two shader-type-data producers are asymmetric.
- **Evidence**: `shader.rs:1419-1423` reads then discards; contrast with
  `shader_data.rs:185-190` which surfaces `skin_tint_alpha` for FO76 only.
- **Impact**: FO4 NPC/creature skin materials lose their authored skin-tint
  alpha at import. Small in practice — nif.xml annotates the field as
  "Overridden by game settings," and no vanilla Skyrim content reaches this
  arm (FO4-only BSVER band; tangential to this dimension's Skyrim scope but
  lives in an audited entry point).
- **Related**: Mirror of the FO76 `Fo76SkinTint` path that does preserve alpha.
- **Suggested Fix**: If FO4 fidelity is later wanted, add an optional
  `skin_tint_alpha` to `ShaderTypeData::SkinTint` (or reuse `Fo76SkinTint`)
  and populate `MaterialInfo.skin_tint_alpha` from the FO4 arm. Otherwise
  leave as-is with an explicit `// intentionally dropped: game-setting
  override` comment.

### Existing: #1897 (OPEN) — not refiled
`ShaderFlags<'a>` typed view (`crates/nif/src/shader_flags.rs:544-639`) and
`has_shader_property_fo3_fields` are fully tested but transitively dead in
production — the import path uses the free `is_decal_from_modern_shader_flags`
/ `is_two_sided_from_modern_shader_flags` helpers instead. Both code paths are
correct; this is a consolidation opportunity already tracked by #1897.

### Verified CORRECT
- Every numeric Skyrim shader type (`parse_shader_type_data`,
  `crates/nif/src/blocks/shader.rs:1304-1380`) dispatches to the exact
  trailing-field count nif.xml specifies (EnvironmentMap/SkinTint/HairTint/
  ParallaxOcc/MultiLayerParallax/SparkleSnow/EyeEnvmap); the 14 no-trailing-data
  types read zero trailing bytes — no silent over-read.
- FO76's `BSShaderType155` mapping (`parse_shader_type_data_fo76`) is a fully
  separate function from the Skyrim/FO4 dispatchers — no cross-contamination
  (verified type 4 = Fo76SkinTint Color4, type 5 = HairTint Color3, matching
  nif.xml).
- Skyrim flag-bit decode (`shader_flags.rs`) matches nif.xml, including the
  explicit bit-21 three-way collision guard (Alpha_Decal FO3/FNV vs Cloud_LOD
  Skyrim vs Anisotropic_Lighting Skyrim/FO4) so the modern decal helper never
  misreads bit 21.
- `BSEffectShaderProperty` field layout (`parse_inner`, `shader.rs:1661-1836`)
  matches nif.xml field-for-field across Skyrim/FO4/FO76 eras.
- #1241 PBR scalars (smoothness/specular_strength/etc.) correctly flow into
  `MaterialInfo` (`walker.rs:354-385`).
- **Disney/Burley lobe pin**: `MAT_FLAG_PBR_BSDF` is OR'd only by
  `pack_bgsm_material_flags` when `mesh.is_pbr` is true, which is set only in
  the BGSM/.mat merge path. Vanilla Skyrim ships inline
  `BSLightingShaderProperty` with no external material reference
  (`root_material_path` gated FO4+ only) — so `is_pbr` stays false and the
  principled BRDF branch stays structurally unreachable for vanilla Skyrim
  content, confirmed at the data-flow level.

Test evidence: `cargo test -p byroredux-nif` — 876 tests green.

---

## Dimension 3: NPC Equip + FaceGen (M41)

**Verdict: 4 NEW findings (1 HIGH, 1 MEDIUM, 2 LOW).**

### SKY-D3-NEW-01: Skyrim+ prebaked NPC spawn has no body-mesh fallback — FaceGeom NIF is head-only, RACE skin (WNAM) never parsed
- **Severity**: HIGH
- **Dimension**: NPC Equip + FaceGen (M41)
- **Location**: `byroredux/src/npc_spawn.rs:1801-1807` (doc comment),
  `byroredux/src/npc_spawn.rs:1910-1944` (facegen-only load, no body load),
  `crates/plugin/src/esm/records/actor.rs` (`parse_race` — no `WNAM`/skin
  handling anywhere in the file)
- **Status**: NEW
- **Description**: `spawn_prebaked_npc_entity` (the Skyrim/FO4/FO76/Starfield
  NPC spawn path) loads exactly three mesh sources: skeleton, the per-NPC
  FaceGeom NIF, and whatever armor resolves from OTFT/CNTO. Its own doc
  comment claims "the per-NPC head **and** body in one already-skinned mesh."
  This was empirically disproven: a real vanilla FaceGeom NIF extracted from
  `Skyrim - Meshes0.bsa`
  (`meshes\actors\character\facegendata\facegeom\skyrim.esm\0004d8d1.nif`)
  contains only head/face/hair/eye shapes — no body, torso, hand, arm, or leg
  geometry, matching Bethesda's actual FaceGen SDK convention (head-only
  bake). Separately, `RaceRecord`/`parse_race` never reads Skyrim's RACE
  `WNAM` sub-record (the race's default "naked skin" ARMO every actor
  implicitly wears beneath other layers). Parsing live `Skyrim.esm` and
  expanding the 6 named BanneredMare control-bench NPCs' real OTFT/CNTO/LVLI
  data shows **Hulda** and **Mikael** resolve to `biped_flags=0x80`
  (Feet-only) combined coverage — no head/hair/body/hands/forearms armor at
  all. With no body sub-mesh in the FaceGeom NIF and no RACE-skin fallback,
  these NPCs have zero mesh source for torso/arms/hands/legs today.
- **Evidence**: FaceGeom shape-name dump (head/face/hair/eye shapes only,
  extracted from real Meshes0.bsa); live-ESM OTFT/CNTO/LVLI expansion for
  Hulda (`00013BA3`) and Mikael (`0001A670`), both resolving to `0x80` Feet
  only; `crates/plugin/src/equip.rs:69-82` biped-bit table confirms bit 7 =
  Feet; `npc_spawn.rs:1946-1959` explicitly documents no body-suppression on
  this path, premised on a FaceGen body that doesn't exist.
- **Impact**: Every Skyrim SE/FO4/FO76/Starfield NPC spawned through this
  path risks missing body geometry wherever OTFT/CNTO doesn't explicitly
  claim the corresponding biped bit — confirmed on 2 of the 6 control-bench
  named NPCs in the worst way. The M41 equip smoke test
  (`docs/smoke-tests/m41-equip.sh`) would not catch this: it only asserts
  `Inventory`/`EquipmentSlots` component counts ≥ 6, with no geometry-
  completeness assertion.
- **Related**: Distinct from (and one layer above) the already-fixed
  component-population issues #1658/#1560.
- **Suggested Fix**: Parse RACE `WNAM` into `RaceRecord` and auto-equip the
  resolved skin ARMO as the lowest-priority layer before OTFT/CNTO apply
  (mirrors real engine behavior); or, at minimum, load the race's generic
  body NIF as an always-present base layer on the prebaked path. Fix the
  stale "head and body in one mesh" doc comment regardless.

### SKY-D3-NEW-02: Slot-displaced armor pieces still render — no mesh-level exclusion on overlapping biped slots
- **Severity**: MEDIUM
- **Dimension**: NPC Equip + FaceGen (M41)
- **Location**: `byroredux/src/npc_spawn.rs:629` (`build_npc_equip_state`,
  prebaked path), `byroredux/src/npc_spawn.rs:1331-1344` (`spawn_npc_entity`,
  kf-era path)
- **Status**: NEW
- **Description**: `EquipmentSlots::equip()` returns the inventory indices
  displaced when a new item claims a biped bit another item already
  occupied, specifically so callers can drop the displaced mesh from the
  render set. Neither spawn path does this — the prebaked path discards the
  return value (`let _ = equipment_slots.equip(...)`), the kf-era path only
  logs it at `debug!`. Every armor whose mesh resolves gets pushed to the
  render list regardless of later displacement. Reachable via the
  in-scope multi-pick LVLI mechanic (bit `0x02`), which intentionally
  expands every eligible entry — two entries claiming overlapping biped bits
  (a common alternate-variant authoring pattern) both render simultaneously.
- **Evidence**: `build_npc_equip_state` loop (`npc_spawn.rs:613-639`) pushes
  to `armor_to_spawn` unconditionally per resolved form ID; no post-loop
  filter against `equipment_slots.occupants` exists.
- **Impact**: Visual z-fight / double-geometry overlap for NPCs whose gear
  list produces overlapping biped-slot armor (multi-pick LVLI outfits,
  mod-added CNTO overlapping a default OTFT slot).
- **Related**: Sibling gap to the already-fixed `body_covered`/
  `armor_covers_main_body` upperbody-skip mechanism (base-body-vs-armor case);
  this is the armor-vs-armor case, left unaddressed.
- **Suggested Fix**: After building the full expanded equip list and running
  every entry through `equipment_slots.equip()`, do a second pass over
  `armor_to_spawn` dropping any entry whose inventory index no longer appears
  in `equipment_slots.occupants`.

### SKY-D3-NEW-03: Per-NPC FaceTint DDS resolved but never loaded or applied
- **Severity**: LOW
- **Dimension**: NPC Equip + FaceGen (M41)
- **Location**: `byroredux/src/npc_spawn.rs:2001-2011`
- **Status**: NEW (explicitly deferred in-code, but untracked as an issue)
- **Description**: `prebaked_facegen_tint_path` computes the correct
  per-NPC face-tint texture path but the result (`_tint_path`) is dropped —
  never fetched or bound to the head material's diffuse slot. Comment frames
  this as an explicit Phase 4 deferral.
- **Impact**: Every Skyrim+/FO4+ NPC head renders with the FaceGeom NIF's
  base diffuse, not Bethesda's per-NPC baked tint blend. Visual-only; does
  not block spawn or equip.
- **Suggested Fix**: Wire through the existing `RefrTextureOverlay`
  machinery the code comment already points at.

### SKY-D3-NEW-04 (tech-debt): audit-skyrim skill mischaracterizes the Dimension-3 skinning-consumer entry point
- **Severity**: LOW
- **Dimension**: Documentation
- **Location**: `.claude/commands/audit-skyrim/SKILL.md` (Dimension 3 entry-point list)
- **Status**: NEW
- **Description**: The skill names `byroredux/src/systems/character.rs` as
  "skinning consumer for heads/bodies." That file is actually the player
  camera/movement controller; the real skinning consumer is
  `byroredux/src/render/skinned.rs`.
- **Impact**: Low — misdirects future audit runs to the wrong file.
- **Suggested Fix**: Update the skill's entry-point line to
  `byroredux/src/render/skinned.rs`.

### Verified CORRECT
- 6 named BanneredMare NPCs land `Inventory`+`EquipmentSlots` and spawn
  equipped via OTFT.items + LVLI dispatch (component-population layer;
  20 passing npc_spawn tests, `docs/smoke-tests/m41-equip.sh` hard gate
  intact) — though see SKY-D3-NEW-01 for the geometry-completeness caveat.
- `resolve_armor_mesh` (ARMO → ARMA → worn-mesh, race/gender dispatch)
  correctly implemented for Skyrim+ (6 passing unit tests). Note: the
  checklist's "upperbody.nif pre-scan skip" premise describes the FO3/FNV/
  Oblivion kf-era path only — Skyrim+ always takes the prebaked path, which
  has no analogous mechanism (correctly so, pending SKY-D3-NEW-01's fix).
- LVLI flattening (`expand_leveled_form_id`) — single-pick vs multi-pick,
  recursion cap, circular-reference guard — all correct (8 passing tests).
- FaceGen heads parse via `BSDynamicTriShape` + the facegen crate with no
  runtime morph evaluator on the Skyrim+ prebaked path, matching the SDK's
  pre-applied-slider convention (expected, not a gap).
- `BSDismemberSkinInstance` partition data correctly reaches the skinning
  pipeline's bone-remap; the dismemberment-semantic consumer is honestly
  documented as future work (#1659), not a silent regression.

Test evidence: `cargo test -p byroredux-nif --lib skin/dismember`,
`byroredux-facegen`, and `byroredux npc_spawn` suites — 117 tests total,
0 failures.

---

## Dimension 4: Multi-Master Load Order + TES5 Cell-Load Regression

**Verdict: 1 NEW MEDIUM finding.**

### DIM4-SKY-01: Deleted-REFR tombstone in a DLC override cell does not survive the cross-plugin merge — base copy still renders
- **Severity**: MEDIUM
- **Dimension**: Multi-Master Load Order + TES5 Cell-Load
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:792-805`
  (`parse_refr_group`, `continue` branch), `crates/plugin/src/esm/cell/mod.rs:948-982`
  (`merge_cell_references`), struct `crates/plugin/src/esm/cell/mod.rs:338` (`PlacedRef`)
- **Status**: Regression/incomplete-fix of #1660 (closed via `2dc43106`, but
  only the single-plugin-load half of the issue's own suggested fix landed)
- **Description**: `parse_refr_group` correctly skips building a `PlacedRef`
  for a REFR carrying the header Deleted flag (0x20) — correct for a
  single-plugin load. But in a multi-master load (this dimension's actual
  scope, and #1660's own repro case), a DLC deletes a base-master REFR by
  re-emitting an override CELL with the same REFR FormID and the Deleted
  flag set. Because the walker fully discards that record — `PlacedRef` has
  no `deleted: bool` field to carry a tombstone forward — the override
  cell's `references` Vec simply doesn't mention that FormID at all.
  `merge_cell_references` (the #1546 fix) is deliberately designed so a
  FormID the override doesn't mention keeps the base's copy — the two fixes
  are in direct architectural conflict, and #1660 currently loses: the base
  copy renders even though a DLC explicitly deleted it. The
  `merge_cell_references` doc comment's claim that #1660 prevents this is
  false for the multi-master path (true only for single-plugin).
- **Evidence**: Empirically confirmed via a temporary probe test (added, run,
  then reverted — `git status` confirmed clean) that routed a real
  Deleted-REFR record through the actual production `parse_refr_group` →
  `merge_cell_references` path: `over_refs after parsing Deleted REFR: []`,
  `merged references: [(65536, 170)]` — the base's REFR `0x10000` was NOT
  suppressed by the DLC's deletion tombstone.
- **Impact**: A Skyrim SE cell where a DLC (Dawnguard/Dragonborn/HearthFires,
  or any third-party master chain) deletes a base-`Skyrim.esm` REFR renders
  that object anyway under `--master Skyrim.esm --esm <DLC>`. Bounded to
  over-render of individual deleted objects (the #1546 fix's own
  empty-cell protection is not at risk). Vanilla single-plugin loads
  (the control bench) are unaffected — multi-master only.
- **Related**: #1660 (closed, incompletely); architecturally entangled with
  #1546.
- **Suggested Fix**: Add `deleted: bool` to `PlacedRef` (or a sibling
  `deleted_form_ids: HashSet<u32>` on `CellData`), set it instead of
  `continue` when `RECORD_FLAG_DELETED` is seen, propagate into the
  override's tombstone set, and have `merge_cell_references` remove the
  corresponding base entry by FormID. Add a regression test spanning both
  `parse_refr_group` AND `merge_cell_references` together — the exact gap
  the original #1660 issue's own unchecked "TESTS" completeness box called
  for.

### Verified CORRECT
- Repeatable `--master <path>` FormID remap (M46.0/#561): per-plugin master
  resolution, named-plugin diagnostics on missing masters, last-write-wins
  collision semantics — all correct.
- `.STRINGS` loader wired per-plugin (not last-`--esm`-only) via a proper
  RAII `StringsTableGuard` — verified end-to-end, 2 passing tests.
- ESL/light-master FormID decode (#1554): `GlobalSlot::compose` matches spec
  exactly, per-plugin slot assignment independent of the flat top-byte space
  — verified both directions with passing tests.
- Deleted-REFR tombstones correctly skip at the single-plugin walk (#1660
  partial); the audit brief's doc-rot watch item ("not captured by the
  parser yet" language) checked and found clean.
- `parse_real_skyrim_esm` guard: PASS against real `Skyrim.esm` (590 cells,
  18,113 statics, 37 worldspaces; Winking Skeever found with 981 refs).
- TES5 compressed-record decompression: verified live against real
  `Skyrim.esm` (15,966 top-level NAVM + 766 HDPT records, zero errors).
- Minimum interior-render record set (CELL/STAT/LIGH/WEAP/ARMO/LAND/LTEX/
  TXST/ADDN) and out-of-scope-but-must-not-error set (NAVM/HDPT/
  BSBehaviorGraphExtraData) all dispatch correctly.
- Control-bench guard: not independently re-benchmarked this pass (another
  engine instance was active for a sibling dimension; project policy
  prohibits parallel engine launches) — relied on ROADMAP's live figure
  (Whiterun BanneredMare: 3216 entities @ 362.8 FPS / 2.76 ms / 1299 draws /
  fence=0.98, tag R6a-stale-14). No `bhk`/collider-classification commits
  since that bench date give reason to suspect regression, but a fresh
  on-device re-bench (already flagged in ROADMAP as 613 commits stale as of
  Session 56) would be the more authoritative source.

Test evidence: `cargo test -p byroredux-plugin` — 566 tests green.

---

## Dimension 5: BSA v105 (LZ4)

**Verdict: CLEAN — 0 findings.** Matches prior `AUDIT_SKYRIM_2026-07-04.md`
result; no commits have touched `crates/bsa/` or `asset_provider/` since,
confirmed via `git log`.

New evidence this pass: a full 11-archive / 65,637-file extraction +
magic-byte-content sweep across all real vanilla v105 archives (Meshes0,
Meshes1, Textures0-8) — 0 extraction errors, 0 content-magic mismatches,
~21 GB. A `dev`-profile run with hash validation enabled produced zero WARN
lines (folder-name hash, file-name hash, folder-offset, and both running
name-length totals all match on-disk values across every real entry).

Key verified facts:
- v105 uses LZ4 **frame** (`lz4_flex::frame::FrameDecoder`), not LZ4 block —
  `lz4_flex::block` is the Starfield BA2 path. The audit checklist's
  "LZ4 block" phrasing is stale prose (already flagged non-actionably by the
  prior audit); the code itself is correct.
- Folder record size (24B v105 vs 16B v103/v104), embedded-name flag
  semantics, and per-file compression toggle — the archive-level and
  per-file compression flags **XOR**, not "win priority" (the checklist's
  framing presupposes an override model that doesn't exist in the real
  format).
- Zero-based sibling auto-load (`open_with_numeric_siblings`,
  `821a425b` lineage) correctly discovers `Textures1..8.bsa` and
  `Meshes1.bsa` from their `…0.bsa` base archives, with the `…10.bsa`
  false-positive guard intact — confirmed via the full sweep above.

---

## Dimension 6: Specialty Blocks + Real-Data Rendering

**Verdict: CLEAN — 0 findings.** `cargo test -p byroredux-nif --lib` — 876
tests green.

Key verified facts:
- `BSLODTriShape` routes through `NiLodTriShape`, NOT `BsTriShape` (#838
  guard intact) — three distinct dispatch arms confirmed for `BSLODTriShape`
  / `BSMeshLODTriShape` / `BSSubIndexTriShape`.
- `BsLagBoneController` + `BsProceduralLightningController` (#837) have
  dedicated parsers correctly dispatched — no block_size WARN burst.
- Container-node unwrapping (`BSTreeNode` wind-bone lists,
  `BSPackedCombined[Shared]GeomDataExtra`, `BSFadeNode`/`BSBlastNode`/
  `BSMultiBoundNode`) all correct in the import walker.
- M35 `.btr` prebaked distant-terrain LOD parses/bakes/uploads/resolves
  diffuse correctly through the zero-based sibling archives, no double-draw.
- Meshes0 sweep baseline (100% clean, 18,862 NIFs) confirmed still accurate
  per ROADMAP's live compat matrix.
- `.bto` object LOD (Session 45 EXAL step 6) loads/unloads correctly with
  the streaming ring, gated `GameKind::Skyrim | Fallout4`, level-4 quads,
  16-cell radius.
- VWD full-model culling (#1731) remains documented forward scope, not a
  regression — matches known state, not re-filed.
- Real-data render traces (tree-LOD static mesh, skinned creature/NPC head,
  BSEffectShaderProperty magic effect) all trace cleanly through
  `import_nif_scene` → `translate_material` → the static/skinned draw
  builders.

---

## Dimension 7: NIFAL Canonical Material Translation (Skyrim slice)

**Verdict: CLEAN — 0 findings.** No regressions of the closed material-
boundary issue history (#1480, #1522, #1535, #1624, #1873, #1280).

Key verified facts:
- `translate_material` (`byroredux/src/material_translate.rs:73-170`) is the
  sole site constructing a `Material` from raw per-game shader data; no
  second translation path or render-time fabrication exists (repo-wide grep
  confirms the only other `Material {}` literals are the `--cornell`
  hand-authored reference scene and `#[cfg(test)]` fixtures).
- `Material.metalness`/`roughness` are plain resolved `f32` fields seeded
  from BGSM/BGEM scalars or an `f32::NAN` sentinel, filled by
  `Material::resolve_pbr` → `classify_pbr_keyword`. The old per-draw
  `Material::classify_pbr` is genuinely deleted — confirmed via grep, every
  remaining textual hit is a comment describing its removal.
- `resolve_pbr()` runs strictly before `classify_glass_into_material`, so
  forced-glass roughness correctly wins over the keyword default.
- Skyrim `BSLightingShaderProperty.emissive_multiple` correctly routes
  through `EmissiveSource::Lighting` (genuine emissive scalar);
  `BSEffectShaderProperty` routes through `Effect` (diffuse-tint
  conflation) — kept distinct, and the `Lighting` source is never
  downgraded when both properties are present on one mesh.

Test evidence: `cargo test -p byroredux-core --lib material` (20 passed),
`cargo test -p byroredux-nif --lib emissive` (12 passed),
`cargo test -p byroredux material_translate` (7 passed).

---

## Shader-Type Coverage Matrix

Legend: **P** = parse arm correct vs nif.xml · **I** = import surfaces field(s)
to MaterialInfo/ImportedMesh · **R** = renderer consumes (or documented-deferred).

### Skyrim `BSLightingShaderType` — `parse_shader_type_data` (BSVER 83-129)

| Type | Name | ShaderTypeData variant | Trailing fields | P | I | R |
|-----|------|------------------------|-----------------|---|---|---|
| 0 | Default | None | — | ✓ | n/a | legacy path |
| 1 | Environment Map | EnvironmentMap | env_map_scale (f32) | ✓ | ✓ | ✓ |
| 2 | Glow Shader | None | — | ✓ | n/a | ✓ |
| 3 | Parallax | None | — | ✓ | n/a | ✓ |
| 4 | Face Tint | None | — | ✓ | material_kind=4 | ✓ |
| 5 | Skin Tint | SkinTint | Color3 | ✓ | ✓ | ✓ materialKind==5 |
| 6 | Hair Tint | HairTint | Color3 | ✓ | ✓ | ✓ |
| 7 | Parallax Occ | ParallaxOcc | max_passes, scale | ✓ | ✓ | deferred |
| 8 | Multitexture Landscape | None | — | ✓ | n/a | ✓ |
| 9 | LOD Landscape | None | — | ✓ | n/a | ✓ |
| 10 | Snow | None | — | ✓ | n/a | ✓ |
| 11 | MultiLayer Parallax | MultiLayerParallax | thickness, refr_scale, TexCoord[2], envmap_strength | ✓ | ✓ | deferred |
| 12 | Tree Anim | None | — | ✓ | n/a | ✓ |
| 13 | LOD Objects | None | — | ✓ | n/a | ✓ |
| 14 | Sparkle Snow | SparkleSnow | Vector4 | ✓ | ✓ | deferred |
| 15 | LOD Objects HD | None | — | ✓ | n/a | ✓ |
| 16 | Eye Envmap | EyeEnvmap | cubemap_scale, L center, R center | ✓ | ✓ | deferred |
| 17 | Cloud | None | — | ✓ | n/a | ✓ |
| 18 | LOD Landscape Noise | None | — | ✓ | n/a | ✓ |
| 19 | Multitexture Landscape LOD Blend | None | — | ✓ | n/a | ✓ |
| 20 | FO4 Dismemberment | None | — (None on Skyrim) | ✓ | n/a | ✓ |

### FO4 `parse_shader_type_data_fo4` (BSVER 130-154) — deltas from Skyrim

| Type | Delta vs Skyrim | P | I | Note |
|-----|-----------------|---|---|------|
| 1 | +2 SSR bools (130-139), intentionally dropped | ✓ | ✓ | |
| 5 | +Skin Tint Alpha f32 (130-139) | ✓ | **DROPPED** | SK-D2-01 |
| 6,7,11,14,16 | identical to Skyrim | ✓ | ✓ | |
| others | None | ✓ | n/a | |

### FO76 `BSShaderType155` — `parse_shader_type_data_fo76` (BSVER 155-171)

| Type | Name | Variant | Trailing | P | I | R |
|-----|------|---------|----------|---|---|---|
| 0 | Default | None | — | ✓ | n/a | ✓ |
| 2 | Glow | None | — | ✓ | n/a | ✓ |
| 3 | Face Tint | None | — | ✓ | n/a | ✓ |
| 4 | Skin Tint | Fo76SkinTint | Color4 (incl. alpha) | ✓ | ✓ | ✓ |
| 5 | Hair Tint | HairTint | Color3 | ✓ | ✓ | ✓ |
| 12 | Eye Envmap | None | — | ✓ | n/a | ✓ |
| 17 | Terrain | None | — | ✓ | n/a | ✓ |

Starfield (BSVER ≥ 172) forces `shader_type = 0` and captures undocumented
trailing bytes via `starfield_tail` (#1606); out of Skyrim scope.

---

## Cell-Load Regression Status

TES5 cells parse cleanly through the unified `esm/cell/` walker, including
compressed-record decompression (verified live against real `Skyrim.esm`:
15,966 top-level NAVM + 766 HDPT records, zero errors). `parse_real_skyrim_esm`
finds `SolitudeWinkingSkeever` with 981 refs, lighting fully populated.

**Whiterun BanneredMare control bench** (per ROADMAP, not independently
re-run this pass — see Dimension 4): **3216 entities @ 362.8 FPS / 2.76 ms /
1299 draws / fence=0.98**, tag R6a-stale-14 (`1c26bc25`, 2026-06-03). No
`bhk`/collider-classification commits since that date give reason to suspect
regression, but ROADMAP itself flags the bench as significantly stale
(613 commits as of Session 56) — a fresh on-device re-bench remains the more
authoritative source than this audit's read of the figure.

**Multi-master regression** (DIM4-SKY-01): the only cell-load correctness gap
found this pass, isolated to the multi-master merge path (DLC-deleted base
REFRs). Single-plugin loads, including the control bench, are unaffected.

---

## Dedup Notes

- #1897 (ShaderFlags typed view dead in production) — re-confirmed open,
  matches Dimension 2's SK-D2-02 observation; not refiled.
- #1660 — closed via `2dc43106` but the fix is incomplete for multi-master
  merges; DIM4-SKY-01 is filed as a regression/incomplete-fix, not a
  duplicate.
- No other findings in this report overlap an open issue in the 200-issue
  snapshot (`/tmp/audit/issues.json`) or, for Dimension 3, the full
  1941-issue open+closed history.

---

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_SKYRIM_2026-07-16.md
```
