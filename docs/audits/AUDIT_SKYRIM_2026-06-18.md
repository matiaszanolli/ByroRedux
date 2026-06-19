# Skyrim SE Compatibility Audit — 2026-06-18

**Type**: Per-game compatibility audit (Skyrim Special Edition)
**Branch**: `main` · HEAD at audit time `2aac5351`
**Scope**: 7 dimensions — BSTriShape packed geometry + SSE skinned reconstruction,
BSLightingShaderProperty/BSEffectShaderProperty shader-type dispatch, NPC equip + FaceGen (M41),
multi-master load order + TES5 cell-load regression, BSA v105 (LZ4), specialty blocks + real-data
rendering, NIFAL canonical material translation (Skyrim slice).
**Dedup baseline**: `/tmp/audit/issues.json` (29 open issues) + prior `docs/audits/AUDIT_SKYRIM_2026-06-16.md`.
**Real data**: `Skyrim Special Edition/Data/` present (Skyrim.esm, Dawnguard.esm, Dragonborn.esm,
HearthFires.esm, Meshes0–1 + Textures0–8 BSAs).

---

## Executive Summary

Skyrim SE is the engine's renderer **control bench** — Whiterun BanneredMare loads as a full cell
with 6 named equipped NPCs, and both loose-mesh and cell rendering work. This audit is therefore
**regression coverage** plus the Skyrim-specific geometry/shader/equip risk surface, not readiness
scoping.

HEAD (`2aac5351`) is **unchanged** since the 2026-06-16 Skyrim audit — no Skyrim-relevant code has
landed in the interim (the only commits since are FO4 precombine / DXT1 / particle / scheduler /
renderer-safety work). The heavy real-data corpus (Meshes0 NIF sweep, BSA v105 extraction) is
therefore byte-identical to 2026-06-16's verified results and was **not** re-burned; those results
are cited below. The geometry/shader/material paths were **independently re-read** at the live code
this session, not transcribed.

**The audit is clean.** No CRITICAL or HIGH findings. Both heavy parse dimensions (BSTriShape
packed geometry, shader-type dispatch) re-verified field-for-field against `nif.xml` and confirmed
CLEAN. The remaining surface is one MEDIUM defense-in-depth gap (deleted-REFR tombstones) and four
LOW items, all of which are carry-overs confirmed still-present.

This audit additionally **elevates two existing open issues** that touch the Skyrim multi-master /
localized-text load path and were under-emphasized in prior dimension write-ups: **#1553** (.STRINGS
loader never wired in) and **#1554** (ESL `0x0200` light-master flag undecoded). Both are real,
unfixed, and Skyrim-impacting; they are recorded in *Cell-Load Regression Status* below (not re-filed).

### Finding Tally

| Severity | Count | IDs |
|----------|-------|-----|
| CRITICAL | 0     | — |
| HIGH     | 0     | — |
| MEDIUM   | 1     | SKY-D4-01 |
| LOW      | 4     | SKY-D3-01 (Existing #1560), SKY-D3-02, SKY-D3-03, SKY-D5-01 |
| **Total**| **5** | (carried + re-confirmed; 0 NEW this cycle) |

**Verified RESOLVED (still fixed):** SK-D2-01 (FO4 `env_map_scale` over-read → `shader.rs:1336-1340`,
gated `bsver < FO4_DLC_UPPER`), SK-D4-01 prior (cross-plugin whole-value REFR stomp → fixed under
#1546, per-FormID merge).

**Existing open issues confirmed real + unfixed (not re-filed):** #1553 (.STRINGS wiring),
#1554 (ESL light-master flag), #1560 (M41 equip count guard).

---

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction: CLEAN (0 findings)

Independently re-verified against `nif.xml` this session. All clean:

- **`VF_*` flag bits** (`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:194-243`): all 11 constants
  (VERTEX=0x001 … FULL_PRECISION=0x400) match `nif.xml` `VertexAttribute`. SSE-recon copies
  (`crates/nif/src/import/mesh/sse_recon.rs:38-44`) match.
- **`vertex_desc` field extraction** (`bs_tri_shape.rs:274-275`): `vertex_attrs = (desc>>44)&0xFFF`,
  `vertex_size = desc&0xF` — correct per nif.xml `BSVertexDesc` bit positions. Same in
  `sse_recon.rs:206`.
- **Half-float decode** (`crates/nif/src/import/mesh/decode.rs:18-46`): IEEE-754 binary16→f32 correct
  across zero/subnormal/normal/Inf/NaN; pinned by `decode_half_float_tests.rs`.
- **Index stride** (`bs_tri_shape.rs:278-284,445`): `num_triangles` u32 on FO4+ / u16 on SSE; indices
  always u16 triples (correct — BSTriShape never uses 32-bit indices).
- **Skinned weights/indices** (`bs_tri_shape.rs:1074-1084`, `sse_recon.rs:332-352`): 4×half weights +
  4×u8 indices, both paths renormalize via shared `renormalize_skin_weights`.
- **SSE Z-up→Y-up**: positions `[x,z,-y]` (`sse_recon.rs:268`), normals `[nx,nz,-ny]` (`:291`),
  tangents `[bx,bz,-by,sign]` (`:380`). The on-disk **bitangent triplet** is routed as the Y-up
  tangent (∂P/∂U) in both the inline decoder (`bs_tri_shape.rs:1009`) and SSE recon
  (`sse_recon.rs:380`); the on-disk "tangent" feeds only the sign. Reconstructed bodies do not read
  magenta/chrome.
- **`alpha_property_consumed`**: set once, unconditionally, in `apply_alpha_flags`
  (`crates/nif/src/import/material/mod.rs:1092`); consulted at exactly two gate sites
  (`crates/nif/src/import/material/walker.rs:496,572`). Skinned geometry inherits the parent
  `NiAlphaProperty` exactly once.

### Dimension 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch: CLEAN (0 findings)

Independently re-verified field-for-field against `nif.xml` this session:

- **Skyrim `parse_shader_type_data`** (`crates/nif/src/blocks/shader.rs:1242-1318`): 1→EnvMap(1f),
  5→SkinTint(3f), 6→HairTint(3f), 7→ParallaxOcc(2f), 11→MultiLayerParallax(6f), 14→SparkleSnow(4f),
  16→EyeEnvmap(7f). All other types (0,2,3,4,8–10,12–13,15,17–20) fall through to `None` reading
  **0 bytes** — no silent over-read.
- **FO76 isolation** (`parse_shader_type_data_fo76`, `shader.rs:1424-1459`): only 4→Fo76SkinTint
  (Color4), 5→HairTint (Color3); reached solely from `parse_fo76_plus` (`shader.rs:1160`). The two
  enums cannot cross-contaminate. (Subtle non-bug worth flagging for future auditors: `BSShaderType155`
  value 12 = "Eye Envmap" carries *no* trailing data because nif.xml gates the eye cubemap on the
  numeric literal `Shader Type == 16`, which `BSShaderType155` never emits — handling correct,
  documented at `shader.rs:1448-1456`.)
- **Skyrim flag bits**: DECAL=0x0400_0000 (bit 26), DYNAMIC_DECAL=0x0800_0000 (bit 27),
  DOUBLE_SIDED=0x10 (slsf2 bit 4) — all match nif.xml; cross-era equivalence enforced by compile-time
  `assert!`s.
- **`BSEffectShaderProperty`** (`shader.rs:1617-1656`): field order matches nif.xml; all named fields
  (`soft_falloff_depth`, `greyscale_texture`, `lighting_influence`, `env_map_min_lod`, falloff
  angle/opacity) captured into `BsEffectShaderData`.
- The Disney/Burley PBR lobe (`MAT_FLAG_PBR_BSDF`) stays unreachable for vanilla Skyrim (no BGSM).

**Verified RESOLVED — SK-D2-01**: `parse_shader_type_data_fo4` reads `env_map_scale` only when
`bsver < FO4_DLC_UPPER` (`shader.rs:1336-1340`), per nif.xml `BSVER <= 139`. Still fixed.

### Dimension 3 — NPC Equip + FaceGen (M41): 3 LOW

The named-NPC equip path is functionally correct and not regressed (`build_npc_equip_state` walks
`OTFT`/`DOFT` default-outfit through `expand_leveled_form_id` and inserts `Inventory`+`EquipmentSlots`).
Three LOW gaps confirmed still present:

#### SKY-D3-01: Skyrim prebaked NPC equip path has no spawn/equip count guard
- **Severity**: LOW
- **Dimension**: NPC Equip + FaceGen
- **Location**: `docs/smoke-tests/m41-equip.sh` · `byroredux/src/cell_loader/references.rs`
- **Status**: Existing: #1560 (re-confirmed unchanged 2026-06-18)
- **Description**: The only verification gate for the named Bannered Mare NPCs landing
  `Inventory`+`EquipmentSlots` is the M41 smoke test, which emits a soft `WARN` (not a hard fail) on
  `Inventory=0`/`EquipmentSlots=0`. Hard floors key on total `entities`/`draws` only — a regression
  zeroing all OTFT/LVLI resolution passes CI silently.
- **Impact**: Equip-pipeline regression on Skyrim+ is not gated; relies on a human reading the log.
- **Suggested Fix**: Promote `Inventory==0`/`EquipmentSlots==0` to hard fails for the Skyrim cell, or
  add an `EquipmentSlots >= 5` floor for WhiterunBanneredMare. Per #1560.

#### SKY-D3-02: Prebaked (Skyrim) equip state ignores TPLT inventory inheritance
- **Severity**: LOW
- **Dimension**: NPC Equip + FaceGen
- **Location**: `byroredux/src/npc_spawn.rs:290-366` (`build_npc_equip_state`, called from
  `spawn_prebaked_npc_entity` at `:1244`) vs `:498` (kf-era `resolve_inherited_inventory` call)
- **Status**: NEW prior cycle, CONFIRMED still present
- **Description**: The kf-era spawn path resolves effective inventory through
  `byroredux_plugin::equip::resolve_inherited_inventory`, which walks the `TPLT` chain when
  `template_flags & TEMPLATE_FLAG_USE_INVENTORY (0x0100)` is set. The Skyrim/prebaked path
  (`build_npc_equip_state`) iterates `npc.default_outfit`/`npc.inventory` directly with no TPLT walk
  (`npc_spawn.rs:306-322`). `template_flags`/`template_form_id` are parsed cross-game
  (`crates/plugin/src/esm/records/actor.rs:557-582`), so leveled/templated Skyrim NPCs with an empty
  own CNTO inherit gear via TPLT and will spawn naked. The 6 named Bannered Mare NPCs author their
  own DOFT/CNTO and are unaffected.
- **Impact**: Render-only naked actors for templated Skyrim NPCs that rely on inherited CNTO
  (narrower than the kf-era case the same helper already covers — most Skyrim NPCs use DOFT, which is
  handled). LOW for the named-NPC target.
- **Suggested Fix**: Seed `build_npc_equip_state`'s inventory from
  `resolve_inherited_inventory(npc, npc.level, index)`, identical to the kf-era path (already
  game-agnostic).

#### SKY-D3-03: BSDismemberSkinInstance per-partition body-part flags parsed but discarded
- **Severity**: LOW
- **Dimension**: NPC Equip + FaceGen
- **Location**: parser `crates/nif/src/blocks/skin.rs:375-401` (`BsDismemberSkinInstance` +
  `BodyPartInfo`); import `crates/nif/src/import/mesh/skin.rs:36-44,135-143`
- **Status**: NEW prior cycle, CONFIRMED still present (documented limitation)
- **Description**: `BsDismemberSkinInstance::parse` reads per-partition `part_flag: u16` and
  `body_part: u16`, but `extract_skin_ni_tri_shape`/`extract_skin_bs_tri_shape` read only `inst.base.*`
  (bone_refs, skeleton_root_ref, data_ref). The `partitions` vector with its dismemberment flags is
  never surfaced into `ImportedSkin`, so Skyrim NPC armor renders over the full FaceGen body with no
  slot-based suppression (acknowledged in-code at `skin.rs:28-29`).
- **Impact**: Cosmetic — armored Skyrim NPCs show body/skin clipping through equipped armor at seams.
  No correctness/UB issue.
- **Suggested Fix**: Surface `BodyPartInfo` partition flags onto `ImportedSkin` so a future
  slot-hiding/dismember consumer can hide FaceGen body sub-shapes whose `body_part_type` overlaps an
  equipped armor's biped slot. Track as Phase B.2.

### Dimension 4 — Multi-Master Load Order + TES5 Cell-Load Regression: 1 MEDIUM

The #1546 per-REFR merge (`merge_cell_references`, `crates/plugin/src/esm/cell/mod.rs:942-961`) is in
place; per-FormID last-write-wins handles *changed* and *added* REFRs. `parse_real_skyrim_esm` finds
SolitudeWinkingSkeever; TES5 compressed-record decompression is green; the interior-render record set
dispatches. One MEDIUM gap confirmed still present:

#### SKY-D4-01: Deleted-REFR tombstones (0x20 flag) not captured — DLC-deleted base REFRs over-render
- **Severity**: MEDIUM
- **Dimension**: Multi-Master Load Order + TES5 Cell-Load
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:780-1004` (`parse_refr_group` /
  `PlacedRef` construction; `header.flags` never inspected) · struct `crates/plugin/src/esm/cell/mod.rs:336-436`
  (no `deleted`/flags field) · merge `cell/mod.rs:942-961` (no delete branch)
- **Status**: NEW prior cycle, CONFIRMED still present (self-documented gap)
- **Description**: The #1546 per-REFR merge keeps base REFRs the DLC didn't re-emit and overlays the
  ones it did. But a Bethesda override CELL can also *delete* a base REFR by re-emitting it with the
  record-level Deleted flag (0x20). `parse_refr_group` reads `header` then sub-records and builds
  `PlacedRef` from `header.form_id`/`base_form_id`/placement only — `header.flags & 0x20` is never
  consulted, and `PlacedRef` carries no flags field. `merge_cell_references` therefore has no way to
  distinguish a delete from an edit; a deleted REFR survives the merge as its base copy. The omission
  is acknowledged in `merge_cell_references`'s own doc comment (`cell/mod.rs:935-938`).
- **Impact**: Base-game REFRs a DLC deletes render twice / in the wrong place under
  `--master Skyrim.esm --esm Dawnguard.esm --cell <overridden>`. Bounded — over-render of individual
  objects, never an empty/near-empty cell. Vanilla single-plugin loads (control bench) unaffected.
- **Suggested Fix**: Capture `header.flags & 0x20 != 0` into a `deleted: bool` on `PlacedRef`; in
  `merge_cell_references`, when an override ref carries the Deleted flag, remove the base entry and
  skip the tombstone rather than overlaying it. Low effort; the FormID key already exists.

**Verified RESOLVED — SK-D4-01 (prior 2026-06-14 HIGH, distinct from SKY-D4-01 above)**: the
whole-value `HashMap::extend` stomping the base REFR list on cross-plugin override is fixed under
#1546 (per-FormID merge), 7 dedicated regression tests.

### Dimension 5 — BSA v105 (LZ4): 1 LOW

v105 header / 24-byte folder records / u64 offsets correct; codec dispatch uses
`lz4_flex::frame::FrameDecoder`. Full Meshes0 + Textures0–8 extraction sweep verified clean
2026-06-16 (~52k files, ~19.4 GB, 0 errors; corpus unchanged this cycle). One LOW operator-UX gap
confirmed still present:

#### SKY-D5-01: Numeric-sibling auto-load skips `Skyrim - Textures0.bsa` siblings (digit suffix gates it off)
- **Severity**: LOW
- **Dimension**: BSA v105 (LZ4)
- **Location**: `byroredux/src/asset_provider.rs` (`open_with_numeric_siblings`,
  `if stem.chars().last().is_some_and(|c| c.is_ascii_digit()) { return; }`)
- **Status**: NEW prior cycle, CONFIRMED still present
- **Description**: `open_with_numeric_siblings` auto-loads `<stem>2.bsa`..`<stem>9.bsa` only when the
  named archive's stem does not already end in a digit. Skyrim's base textures ship as
  `Skyrim - Textures0.bsa`..`Textures8.bsa` — the stem ends in `0`, so passing
  `--textures-bsa "Skyrim - Textures0.bsa"` loads only archive 0; Textures1–8 are silently not loaded.
  Documented intentional behavior (the FNV `Foo.bsa`/`Foo2.bsa` unnumbered-base case is the target).
- **Impact**: If a user passes only `Textures0.bsa`, most texture entries resolve to the
  missing-texture checkerboard → "chrome/posterized" surfaces. Operator UX, not data corruption.
- **Suggested Fix**: Optionally sweep the remaining `<stem>0..9.bsa` siblings when a digit-suffixed
  base is passed (dedup on resolved path). Or keep the docs and treat as WONTFIX.

### Dimension 6 — Specialty Blocks + Real-Data Rendering: CLEAN (0 findings)

The #838 BSLODTriShape regression guard is intact (`crates/nif/src/blocks/mod.rs` routes
`BSLODTriShape` → `NiLodTriShape::parse`, distinct from `BSMeshLODTriShape` → `BsTriShape`). The
#837 `BsLagBoneController` + `BsProceduralLightningController` parsers exist and are dispatched. The
import walker unwraps `BSFadeNode`/`BSBlastNode`/`BSMultiBoundNode`/`BSTreeNode`. **Meshes0 sweep
(2026-06-16, corpus unchanged): 18,862 NIFs, 100.00% clean, 0 truncated / 0 recovered / 0 failures /
0 realignment WARNs.** Specialty blocks parse with 0 unknown on real data.

### Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice): CLEAN (0 findings)

`translate_material` (`byroredux/src/material_translate.rs`) is the single canonical boundary; the
render path reads resolved `m.metalness`/`m.roughness` directly with no per-draw fallback (the old
`Material::classify_pbr` is fully deleted). Ordering holds: `resolve_pbr` runs before
`classify_glass_into_material`, so forced-glass roughness wins. Skyrim `BSLightingShaderProperty`
emissive routes to `EmissiveSource::Lighting`, with the `Effect` arm gated behind
`!info.has_material_data` so a co-present effect-shader block cannot clobber a Skyrim lit material's
emissive source.

---

## Shader-Type Coverage Matrix

`ShaderTypeData` has 9 Rust variants. All are **parse-complete** and **import-complete**
(`apply_shader_type_data` + `capture_shader_type_fields` are exhaustive — a new variant fails
compilation; `to_core()` carries every field into the ECS `Material`).

| Variant | Numeric type(s) | Parse | Import | Render | Note |
|---------|-----------------|-------|--------|--------|------|
| None | 0,2,3,4,8–10,12–13,15,17–20 | ✓ | ✓ | ✓ | Base PBR; reads 0 trailing bytes (no over-read). Glow (type 2) is here — no GlowShader variant. |
| EnvironmentMap | 1 | ✓ | ✓ | ✓ | env scale; FO4 path also gates SSR bools 130–139 |
| SkinTint | 5 | ✓ | ✓ | ✓ | Color3 (FO4 adds skin_tint_alpha 130–139) |
| HairTint | 6 | ✓ | ✓ | ✓ | Color3 |
| ParallaxOcc | 7 | ✓ | ✓ | ✓ | max_passes + scale |
| MultiLayerParallax | 11 | ✓ | ✓ | DEFERRED (#562) | inner-layer fields ride unused on GpuInstance |
| SparkleSnow | 14 | ✓ | ✓ | ✓ | Vector4 |
| EyeEnvmap | 16 | ✓ | ✓ | DEFERRED (#562) | cubemap + L/R centers ride unused |
| Fo76SkinTint | FO76 type 4 | ✓ | ✓ | ✓ (remapped to kind 5 at import) | Color4; FO76 `BSShaderType155` numbering, isolated dispatch |

The render-DEFERRED rows are roadmap items (#562), not regressions.

---

## Cell-Load Regression Status

TES5 cells parse through the unified `esm/cell/` walker; compressed CELL groups decompress
(zlib via `ZlibDecoder`). `parse_real_skyrim_esm` walks real `Skyrim.esm` and finds
SolitudeWinkingSkeever. Multi-master REFR merge is per-FormID last-write-wins (#1546). The Whiterun
BanneredMare control-bench load path is untouched by Dimension-4 merge logic (single-plugin loads
never hit `merge_cell_references`), so the ROADMAP Bench-of-record (R6a-stale-14) is not at risk
from any code in this audit. Per audit instructions and the unchanged HEAD, the Vulkan engine was
not run.

**Known load-order gaps (existing open issues — confirmed real + unfixed this session, not re-filed):**

- **#1553 — .STRINGS loader written + tested but never wired in (MEDIUM).** The companion-file
  loader (`crates/plugin/src/esm/strings_table.rs`, `StringTableSet::load`) and the thread-local
  resolution mechanism (`crates/plugin/src/esm/records/common.rs:104-200`, `StringsTableGuard` /
  `resolve_lstring`) both exist and are unit-tested, but **no production path calls
  `StringTableSet::load` or installs a `StringsTableGuard`** during load. `StringTableSet::load` has
  exactly one repo hit — a doc comment. The load-order entry point
  (`byroredux/src/cell_loader/load_order.rs`) never loads a table, so `resolve_lstring` always finds
  an empty `CURRENT_STRINGS_TABLE` and `read_lstring_or_zstring` falls through to
  `format!("<lstring 0x{:08X}>", id)`. CELL FULL names route through this helper. **Impact**: all
  localized FULL/DESC/INFO text on Skyrim SE renders as `<lstring 0x…>` placeholders. The `localized`
  flag IS detected — only the table load is missing.

- **#1554 — ESL `0x0200` light-master flag undecoded; `0xFE` prefix treated as flat mod-index
  (MEDIUM).** The live ESM reader (`crates/plugin/src/esm/reader.rs:611-656`, `read_file_header`)
  reads only `flags & 0x80` (localized) — `0x0200` is never tested. `FormIdRemap::remap`
  (`reader.rs:256-297`) computes `mod_index = (raw >> 24) as u8` and indexes the master table flatly;
  a `0xFE_III_FFF` ESL FormID yields `mod_index = 254`, falls into the out-of-range pass-through
  branch, and the nested `III` sub-index / `FFF` local-id are never decoded. The ESL decoder that
  exists (`crates/plugin/src/legacy/mod.rs:74-96`) is in the dead legacy bridge (per CLAUDE.md the
  live path is `crates/plugin/src/esm/`). **Impact**: any ESL-flagged plugin (vanilla Skyrim SE
  ships Creation Club ESLs; common in modded load orders) has its `0xFE`-prefixed FormIDs
  mis-resolved — cross-plugin REFR/base lookups into ESL content fail.

These two are the most consequential Skyrim load-order items currently open; the SKILL's
multi-master dimension exercises exactly the path they break.

---

## Dedup Notes

- **SKY-D3-01** matches open issue **#1560** — re-confirmed, not re-filed.
- **SKY-D3-02, SKY-D3-03, SKY-D4-01, SKY-D5-01** — carry-overs from 2026-06-16; no matching
  open/closed issue. Each re-verified against live code this session.
- **#1553, #1554** — existing open issues; confirmed real + unfixed in the live `esm/` path
  (Cell-Load Regression Status). Not re-filed.
- **SK-D2-01** / **SK-D4-01** (prior reports) — verified RESOLVED and still fixed
  (`shader.rs:1336-1340`; #1546 per-FormID merge).
- No other open issue in `/tmp/audit/issues.json` touches the audited Skyrim paths (#1606 is
  Starfield-LOD BSLSP; #1592 is FO4 flag-bits; #1627 is a GpuMaterial transmission TODO — all out of
  scope).

---

## Recommendation

Skyrim SE remains the engine's healthiest game target — clean parse, clean cell load, working NPC
equip. HEAD is unchanged since 2026-06-16 and the audit re-confirms that clean state. The actionable
backlog is small and unchanged: one MEDIUM (SKY-D4-01 deleted-REFR tombstones) and three LOW
NPC-equip/BSA-CLI hardening items, plus two already-tracked load-order gaps (#1553 .STRINGS wiring,
#1554 ESL light-master) that would unblock localized text and ESL content for Skyrim multi-master
loads. Suggested next step:

```
/audit-publish docs/audits/AUDIT_SKYRIM_2026-06-18.md
```
