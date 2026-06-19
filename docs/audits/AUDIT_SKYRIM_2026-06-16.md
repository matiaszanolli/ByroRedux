# Skyrim SE Compatibility Audit — 2026-06-16

**Type**: Per-game compatibility audit (Skyrim Special Edition)
**Branch**: `main` · HEAD at audit time `2aac5351`
**Scope**: 7 dimensions — BSTriShape packed geometry + SSE skinned reconstruction,
BSLightingShaderProperty/BSEffectShaderProperty shader-type dispatch, NPC equip + FaceGen (M41),
multi-master load order + TES5 cell-load regression, BSA v105 (LZ4), specialty blocks + real-data
rendering, NIFAL canonical material translation (Skyrim slice).
**Dedup baseline**: `/tmp/audit/issues.json` (29 open issues) + prior `docs/audits/AUDIT_SKYRIM_2026-06-14.md`.
**Real data**: `Skyrim Special Edition/Data/` present (Skyrim.esm, Dawnguard.esm, Dragonborn.esm,
Meshes0 + Textures0–8 BSAs). Real-data probes executed where feasible (ESM walk, BSA sweep, Meshes0 NIF sweep).

---

## Executive Summary

Skyrim SE is the engine's renderer **control bench** — Whiterun BanneredMare loads as a full cell
with 6 named equipped NPCs, and both loose-mesh and cell rendering work. This audit is therefore
**regression coverage** plus the Skyrim-specific geometry/shader/equip risk surface, not readiness
scoping.

**The audit is clean.** No CRITICAL or HIGH findings. The single most consequential prior finding —
`SK-D4-01` (2026-06-14, HIGH: cross-plugin cell override stomping the entire base REFR list) — is
now **fixed under #1546** with 7 green regression tests. The prior `SK-D2-01` (FO4 `env_map_scale`
over-read) is likewise **fixed** in `shader.rs:1336-1340`. The remaining surface is one MEDIUM
defense-in-depth gap (deleted-REFR tombstones) and four LOW items (NPC-equip hardening + a BSA CLI
footgun).

Two heavy real-data validations passed cleanly:
- **Meshes0 NIF sweep**: 18,862 NIFs, 100.00% clean, 0 truncated / 0 recovered / 0 failures /
  0 realignment WARNs — matches baseline exactly.
- **BSA v105 extraction sweep**: Meshes0 + Textures0–8 (~52k files, ~19.4 GB decompressed), 0 errors.

### Finding Tally

| Severity | Count | IDs |
|----------|-------|-----|
| CRITICAL | 0     | — |
| HIGH     | 0     | — |
| MEDIUM   | 1     | SKY-D4-01 |
| LOW      | 4     | SKY-D3-01 (Existing #1560), SKY-D3-02, SKY-D3-03, SKY-D5-01 |
| **Total**| **5** | (3 NEW, 1 existing, plus 2 RESOLVED verifications) |

**Verified RESOLVED (no longer findings):** SK-D2-01 (FO4 env_map_scale over-read → fixed,
`shader.rs:1336-1340`), SK-D4-01 (cross-plugin REFR stomp → fixed under #1546).

---

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction: CLEAN (0 findings)

All four checklist items verified clean. `VF_*` flag bits map to nif.xml `BSVertexDesc.VertexAttribute`
and `half_to_f32` is IEEE-754 binary16-correct (subnormals/inf/NaN/signed-zero pinned by test).
`decode_bs_vertex_stream` handles all flag combinations with a self-correcting stride and a hard
error (not wrapping-skip) on overrun; u16 index stride correct; skinned indices flow through the
partition-local→global remap (#613) + weight renormalization (#889). SSE reconstruction
(`decode_sse_packed_buffer`) applies the `(x,z,-y)` Z-up→Y-up swap to positions/normals and routes
the on-disk "bitangent" triplet as the Y-up tangent (∂P/∂U) with #1559's `VF_TANGENTS`-only gate and
#1204's `synthesize_tangents_yup` fallback — reconstructed bodies don't read magenta/chrome.
`alpha_property_consumed` is set unconditionally (`mod.rs:1092`) and consulted at both walker gate
sites (`walker.rs:496,572`), so skinned geometry inherits the parent `NiAlphaProperty` exactly once.
Heavily-audited subsystem — every prior hazard (#559, #613, #621, #889, #1201, #1202, #1204, #1547,
#1559) has a landed fix.

### Dimension 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch: CLEAN (0 findings)

All six checklist items verified field-for-field against nif.xml. Every numeric Skyrim/FO4 shader
type routes to the correct `ShaderTypeData` arm with the right trailing-field count; all no-data
types (0/2/3/4/8–10/12–13/15/17–19) fall through to the catch-all `None` reading zero bytes (no
silent over-read). FO76 `BSShaderType155` numbering is isolated in `parse_shader_type_data_fo76`
dispatched only from `parse_fo76_plus` — the two enums cannot cross-contaminate. Skyrim flag-bit
positions live in a separate `shader_flags.rs` namespace from FO4. `BSEffectShaderProperty` matches
nif.xml field-for-field. PBR scalars (#1241) surface into `MaterialInfo`. The Disney/Burley lobe
(`MAT_FLAG_PBR_BSDF = 32u`) is provably unreachable for vanilla Skyrim — `is_pbr` is set only via the
BGSM/BGEM/.mat merge paths, which vanilla Skyrim (no BGSM) never exercises.

**Verified RESOLVED — SK-D2-01**: the 2026-06-14 report flagged `parse_shader_type_data_fo4` reading
`env_map_scale` unconditionally for `shader_type == 1` (nif.xml gates it `BSVER <= 139`). Current code
(`shader.rs:1336-1340`) reads it only when `bsver < FO4_DLC_UPPER` (140). Skyrim was never reachable
(routes through `parse_skyrim`), so this was a dead/dev-only band — now fixed regardless.

### Dimension 3 — NPC Equip + FaceGen (M41): 3 LOW

The named-NPC equip path is functionally correct and not regressed (`spawn_prebaked_npc_entity` →
`build_npc_equip_state` walks `OTFT.items` through `expand_leveled_form_id` and inserts
`Inventory`+`EquipmentSlots` before archive I/O). LVLI flattening, FaceGen parse, and
`BSDismemberSkinInstance` bone-palette routing are clean. Three LOW gaps:

#### SKY-D3-01: Skyrim prebaked NPC equip path has no spawn/equip count guard
- **Severity**: LOW
- **Dimension**: NPC Equip + FaceGen
- **Location**: `docs/smoke-tests/m41-equip.sh:198-205` · `byroredux/src/cell_loader/references.rs:336-358`
- **Status**: Existing: #1560 (re-confirmed unchanged 2026-06-16)
- **Description**: The only verification gate for the named Bannered Mare NPCs landing
  `Inventory`+`EquipmentSlots` is the smoke test, which emits a soft `WARN` (not a hard fail) when
  `Inventory=0` / `EquipmentSlots=0`. Hard floors key on total `entities`/`draws` only — a regression
  zeroing all OTFT/LVLI resolution passes CI silently.
- **Evidence**: `m41-equip.sh:200` emits the WARN with no `hard_fail=1`; `hard_fail` set only from
  `entities`/`draws` floors (lines 156, 162).
- **Impact**: Equip-pipeline regression on Skyrim+ is not gated; relies on a human reading the log.
- **Suggested Fix**: Promote `Inventory==0` / `EquipmentSlots==0` to hard fails for the Skyrim cell,
  or add an `EquipmentSlots >= 5` floor for WhiterunBanneredMare. Per #1560.

#### SKY-D3-02: Prebaked (Skyrim) equip state ignores TPLT inventory inheritance
- **Severity**: LOW
- **Dimension**: NPC Equip + FaceGen
- **Location**: `byroredux/src/npc_spawn.rs:290-366` (`build_npc_equip_state`) vs `:497-498` (kf-era `resolve_inherited_inventory`)
- **Status**: NEW
- **Description**: The kf-era spawn path resolves effective inventory through
  `resolve_inherited_inventory`, which walks the `TPLT` chain when
  `template_flags & TEMPLATE_FLAG_USE_INVENTORY` is set. The Skyrim/prebaked path reads `npc.inventory`
  / `npc.default_outfit` directly with no TPLT walk. Skyrim's `Use Inventory` ACBS template flag is the
  same mechanism; leveled/templated Skyrim NPCs (LCharacter-driven guards/bandits) inherit gear this
  way and will spawn with empty `Inventory`/`EquipmentSlots`. The 6 named Bannered Mare NPCs author
  their own OTFT/CNTO, so they are unaffected — generic templated Skyrim cell population is not.
- **Evidence**: `build_npc_equip_state` (lines 300-331) iterates `npc.default_outfit`/`npc.inventory`
  only; `resolve_inherited_inventory` call sites are 497-498, inside `spawn_npc_entity` only.
- **Impact**: Render-only naked actors for templated Skyrim NPCs more broadly; low for the named-NPC target.
- **Suggested Fix**: Seed `build_npc_equip_state`'s inventory from
  `resolve_inherited_inventory(npc, npc.level, index)`, identical to the kf-era path (already game-agnostic).

#### SKY-D3-03: BSDismemberSkinInstance per-partition body-part flags parsed but discarded
- **Severity**: LOW
- **Dimension**: NPC Equip + FaceGen
- **Location**: `crates/nif/src/import/mesh/skin.rs:36-43,135-142` · `byroredux/src/npc_spawn.rs:1320-1364`
- **Status**: NEW (documented limitation, verified not stale)
- **Description**: On Skyrim, head+body ship as one combined prebaked FaceGen NIF
  (`humanoid_body_paths(Skyrim)` returns `&[]`), so the kf-era `upperbody.nif` body-skip is N/A.
  Suppressing the Skyrim body under armor would require hiding FaceGen body sub-shapes via their
  `BSDismemberSkinInstance` partition `body_part_type` flags. Those flags ARE parsed
  (`blocks/skin.rs`, `partitions: Vec<BodyPartInfo>`) but the import layer reads only
  `base.bone_refs`/`base.data_ref`, never the body-part-type table — so Skyrim NPC armor renders over
  the full FaceGen body with no slot-based suppression (acknowledged in-code, deferred to "Phase B.2").
- **Evidence**: `skin.rs:36` accepts `BsDismemberSkinInstance` but extracts `inst.base.*` only;
  `inst.partitions` never read in `import/mesh/skin.rs`. `npc_spawn.rs:1330-1333` defers explicitly.
- **Impact**: Cosmetic — armored Skyrim NPCs show body/skin clipping through equipped armor at seams.
  No correctness/UB issue.
- **Suggested Fix**: Surface `BodyPartInfo` partition flags into `ImportedMesh` so the spawn layer can
  hide FaceGen body sub-shapes whose `body_part_type` overlaps an equipped armor's biped slot. Track as Phase B.2.

### Dimension 4 — Multi-Master Load Order + TES5 Cell-Load Regression: 1 MEDIUM

All six checklist items verified against live code. The #1546 per-REFR merge
(`EsmCellIndex::merge_from` → `merge_cell_references`, `cell/mod.rs:942-961`) is in place with 7 green
regression tests; `parse_real_skyrim_esm` finds SolitudeWinkingSkeever (981 refs, XCLL OK); TES5
compressed-record decompression is green; the interior-render record set (CELL/REFR/STAT/LIGH/WEAP/
ARMO + LAND ×8.0 scale / LTEX / TXST / ADDN) all dispatch; out-of-scope records (NAVM / HDPT /
BSBehaviorGraphExtraData) parse without error; the single-plugin Whiterun control-bench load path is
untouched by the merge logic. A hypothesized no-EDID interior-override-drop bug was disproven on real
`Dawnguard.esm` data (all 114 interior CELLs carry EDID; 0 dropped).

#### SKY-D4-01: Deleted-REFR tombstones (0x20 flag) not captured — DLC-deleted base REFRs over-render
- **Severity**: MEDIUM
- **Dimension**: Multi-Master Load Order + TES5 Cell-Load
- **Location**: `crates/plugin/src/esm/cell/walkers.rs` (REFR parse — record flags never read) · merge contract `crates/plugin/src/esm/cell/mod.rs:935-938,942-961`
- **Status**: NEW (documented in-code only; no matching open/closed issue)
- **Description**: The #1546 per-REFR merge keeps base REFRs the DLC didn't re-emit and overlays the
  ones it did. But a Bethesda override CELL can also *delete* a base REFR by re-emitting it with the
  record-level Deleted flag (0x20). The cell walker never reads REFR record flags into
  `PlacedReference` (the struct carries only `form_id`/`base_form_id`/placement data — no flags field),
  so `merge_cell_references` has no way to distinguish a delete from an edit. A deleted REFR therefore
  survives the merge as its base copy.
- **Evidence**: `PlacedReference` (`cell/mod.rs:345+`) has no record-flags field; `walkers.rs` parses
  REFR sub-records but never `header.flags & 0x20`; `merge_cell_references` (`cell/mod.rs:950-959`) has
  no delete branch. Acknowledged in the doc comment at `cell/mod.rs:935-938`.
- **Impact**: A handful of base-game REFRs a DLC deletes render twice / in the wrong place under
  `--master Skyrim.esm --esm Dawnguard.esm --cell <overridden>`. Bounded — over-render of individual
  objects, never an empty/near-empty cell. Vanilla single-plugin loads (control bench) unaffected.
- **Suggested Fix**: Add a `flags: u32` (or `deleted: bool`) field to `PlacedReference`, set from the
  REFR `RecordHeader.flags` at parse time, and in `merge_cell_references` drop both the over-ref and the
  matching base entry when the override carries 0x20. Low effort; the FormID key already exists.

**Verified RESOLVED — SK-D4-01 (prior, distinct from SKY-D4-01 above)**: the 2026-06-14 HIGH
(whole-value `HashMap::extend` stomping the base REFR list on cross-plugin override) is fixed under
#1546 with 7 dedicated regression tests, all green.

### Dimension 5 — BSA v105 (LZ4): 1 LOW

All three checklist items verified clean against real Skyrim SE archives. v105 header /
24-byte folder records / u64 offsets are correct; the codec dispatch uses
`lz4_flex::frame::FrameDecoder` (LZ4 **frame** format, which is what Skyrim SE ships — the prompt's
"lz4_flex::block" is a wording quirk, the frame decoder is the correct call) and produces a
byte-correct sweetroll NIF. The hash table is unchanged from v104 (single shared impl, zero
hash/offset/name-length mismatches on a real-data debug run). Compression and embed-name flags use the
correct relative-toggle (XOR) model — no archive-vs-file "priority" to disagree on. A full extraction
sweep of Meshes0 + Textures0–8 (~52k files, ~19.4 GB) hit 0 errors.

#### SKY-D5-01: Numeric-sibling auto-load skips `Skyrim - Textures0.bsa` siblings (digit suffix gates it off)
- **Severity**: LOW
- **Dimension**: BSA v105 (LZ4)
- **Location**: `byroredux/src/asset_provider.rs:398-400` (`open_with_numeric_siblings`)
- **Status**: NEW
- **Description**: `open_with_numeric_siblings` auto-loads `<stem>2.bsa`..`<stem>9.bsa` only when the
  named archive's stem does not already end in a digit. Skyrim's base textures ship as
  `Skyrim - Textures0.bsa`..`Textures8.bsa` — the stem `Skyrim - Textures0` ends in `0`, so passing
  `--textures-bsa "Skyrim - Textures0.bsa"` loads only archive 0; Textures1–8 are silently not loaded.
  The user must list all nine archives explicitly. Documented intentional behavior (the FNV
  `Foo.bsa`/`Foo2.bsa` unnumbered-base case is the feature's target).
- **Evidence**: `asset_provider.rs:398`: `if stem.chars().last().is_some_and(|c| c.is_ascii_digit()) { return; }`.
  Confirmed the eight sibling files exist and all extract cleanly when opened directly.
- **Impact**: If a user passes only `Textures0.bsa`, ~26k of ~32k texture entries resolve to the
  missing-texture checkerboard → "chrome/posterized" surfaces. Operator UX, not data corruption.
- **Suggested Fix**: Optionally, when a digit-suffixed base is passed, also sweep the remaining
  `<stem>0..9.bsa` siblings (dedup on resolved path). Or keep the docs and treat as WONTFIX.

### Dimension 6 — Specialty Blocks + Real-Data Rendering: CLEAN (0 findings)

All static checklist items verified clean and confirmed against the full Meshes0 corpus. The #838
BSLODTriShape regression guard is intact (`blocks/mod.rs:452` routes `BSLODTriShape` →
`NiLodTriShape::parse`, distinct from `BSMeshLODTriShape` → `BsTriShape::parse_lod`). The #837
`BsLagBoneController` + `BsProceduralLightningController` parsers exist and are dispatched. The import
walker correctly unwraps `BSFadeNode`/`BSBlastNode`/`BSMultiBoundNode`/`BSTreeNode` via `as_ni_node()`
and downcasts `NiLodTriShape`. The static + skinned render trace is fully wired
(`import_nif_scene` → `translate_material` → intern GpuMaterial + resolve texture handle).
**Meshes0 sweep (executed): 18,862 NIFs, 100.00% clean, 0 truncated / 0 recovered / 0 failures /
0 realignment WARNs.** Specialty blocks all parse with 0 unknown on real data (BSLODTriShape 23,
BSLagBoneController 163, BSProceduralLightningController 3, BSTreeNode 20).

### Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice): CLEAN (0 findings)

All four checklist items verified clean. `translate_material` (`material_translate.rs:73-170`) is the
single canonical boundary with exactly two production callers (cell-loader spawn + loose-NIF); the
render path reads resolved `m.metalness`/`m.roughness` directly with no per-draw fallback. The old
per-draw `Material::classify_pbr` is fully deleted (grep returns zero hits) — only the pure
`classify_pbr_keyword` + `resolve_pbr` survive. Ordering holds: `resolve_pbr` (`:160`) runs before
`classify_glass_into_material` (`:161`), so forced-glass roughness wins. Skyrim
`BSLightingShaderProperty` emissive routes to `EmissiveSource::Lighting` (`walker.rs:308-311`), with
the `Effect` arm gated behind `!info.has_material_data` so a co-present effect-shader block cannot
clobber a Skyrim lit material's emissive source.

---

## Shader-Type Coverage Matrix

`ShaderTypeData` has 9 Rust variants. All are **parse-complete** and **import-complete**
(`apply_shader_type_data` + `capture_shader_type_fields` are exhaustive — a new variant fails
compilation; `to_core()` carries every field into the ECS `Material`).

| Variant | Numeric type(s) | Parse | Import | Render | Note |
|---------|-----------------|-------|--------|--------|------|
| None | 0,2,3,4,8–10,12–13,15,17–19 | ✓ | ✓ | ✓ | Base PBR; reads 0 trailing bytes (no over-read). Glow (type 2) is here — no GlowShader variant. |
| EnvironmentMap | 1 | ✓ | ✓ | ✓ | env scale; FO4 path also gates SSR bools 130–139 |
| SkinTint | 5 | ✓ | ✓ | ✓ | Color3 (FO4 adds skin_tint_alpha 130–139) |
| HairTint | 6 | ✓ | ✓ | ✓ | Color3 |
| ParallaxOcc | 7 | ✓ | ✓ | ✓ | max_passes + scale |
| MultiLayerParallax | 11 | ✓ | ✓ | DEFERRED (#562) | inner-layer fields ride unused on GpuInstance |
| SparkleSnow | 14 | ✓ | ✓ | ✓ | Vector4 |
| EyeEnvmap | 16 | ✓ | ✓ | DEFERRED (#562) | cubemap + L/R centers ride unused |
| Fo76SkinTint | FO76 type 4 | ✓ | ✓ | ✓ (remapped to kind 5 at import) | Color4; FO76 `BSShaderType155` numbering, isolated dispatch |

The GpuInstance vec4-share between `hair_tint.w` and `multi_layer_envmap_strength` is guarded by a
`debug_assert!` that holds because `ShaderTypeData` is single-tag. The render-DEFERRED rows are
roadmap items (#562), not regressions.

---

## Cell-Load Regression Status

TES5 cells parse through the unified `esm/cell/` walker; compressed CELL groups decompress
(`reader.rs:478-504`, zlib via `ZlibDecoder`). `parse_real_skyrim_esm` walks real `Skyrim.esm`
(590 cells, 18,113 statics, 37 worldspaces) and finds SolitudeWinkingSkeever (981 refs, lighting
populated). Multi-master REFR merge is per-FormID last-write-wins (#1546). The Whiterun BanneredMare
control-bench load path is untouched by Dimension-4 merge logic (single-plugin loads never hit
`merge_cell_references`), so the ROADMAP Bench-of-record (R6a-stale-14: 3216 entities @ 362.8 FPS /
2.76 ms / 1299 draws / fence=0.98) is not at risk from any code in this audit. Per audit instructions,
the Vulkan engine was not run; bench refresh remains gated on R6a-stale-15 (a fresh 300-frame GPU bench).

---

## Dedup Notes

- **SKY-D3-01** matches open issue **#1560** (M41 smoke test soft-WARN, no hard equip count guard) — re-confirmed, not re-filed.
- **SKY-D3-02, SKY-D3-03, SKY-D4-01, SKY-D5-01** — NEW; no matching open/closed issue.
- **SK-D2-01** (prior 2026-06-14 report) — verified RESOLVED in `shader.rs:1336-1340`; recommend closing the tracking item.
- **SK-D4-01** (prior 2026-06-14 report) — verified RESOLVED under #1546 with 7 regression tests.
- No other open issue in `/tmp/audit/issues.json` touches the audited Skyrim paths (#1606 is
  Starfield-LOD BSLSP; #1592 is FO4 flag-bits; #1627 is a GpuMaterial transmission TODO — all out of scope).

---

## Recommendation

Skyrim SE remains the engine's healthiest game target — clean parse, clean cell load, working NPC
equip. The actionable backlog is small: one MEDIUM (SKY-D4-01 deleted-REFR tombstones) and three LOW
NPC-equip/BSA-CLI hardening items. Suggested next step:

```
/audit-publish docs/audits/AUDIT_SKYRIM_2026-06-16.md
```
