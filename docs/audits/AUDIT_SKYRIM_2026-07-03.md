# Skyrim SE Compatibility Audit — 2026-07-03

HEAD: `8498e559` · Prior audit: `docs/audits/AUDIT_SKYRIM_2026-07-02.md` (0 new
findings, all-CLEAN) · This audit is a same-day comprehensive-sweep follow-up:
5 commits landed between the two audits, none touching the Skyrim-specific
risk surface in a way that introduces regressions.

## Executive Summary

Skyrim SE remains the engine's renderer **control bench** (Whiterun
BanneredMare, 6 equipped NPCs, real `bhk` collision) — cell-load and rendering
both work end-to-end. This audit re-verified all 7 dimensions from
`.claude/commands/audit-skyrim/SKILL.md` directly against source (no
delegated sub-agents, per this run's instructions) and cross-checked every
candidate finding against the 71 currently-open GitHub issues
(`/tmp/audit/issues.json`) plus the 14 prior Skyrim audit reports under
`docs/audits/`.

**Result: 0 new findings.** The intervening commits since the 2026-07-02
audit (`175ebf2c` #1731, `ae219630` #1728, `ffe9a816` #1718, `2f0b99fa` #1740,
plus the skill-refresh commit `8498e559` itself) are all small, test-covered,
additive changes — none regress a Skyrim-specific invariant. Full findings
detail and re-verification evidence below.

## Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction — CLEAN

`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs`,
`crates/nif/src/import/mesh/sse_recon.rs`, `crates/nif/src/import/mesh/tangent.rs`

- `VF_*` bitfield: `VF_VERTEX=0x001`, `VF_UVS=0x002`, `VF_UVS_2=0x004`,
  `VF_NORMALS=0x008`, `VF_TANGENTS=0x010`, `VF_VERTEX_COLORS=0x020`,
  `VF_SKINNED=0x040`, `VF_LAND_DATA=0x080`, `VF_EYE_DATA=0x100`,
  `VF_INSTANCE=0x200`, `VF_FULL_PRECISION=0x400` (`bs_tri_shape.rs:194-243`) —
  matches nif.xml `BSVertexDesc` bit layout, unchanged since the last audit.
- `BsTriShapeKind` correctly disambiguates `Plain` / `LOD` / `MeshLOD` /
  `SubIndex(Box<..>)` / `Dynamic` (#560/#404) — re-read in full, no drift.
- **SSE skinned-reconstruction tangent routing (regression guard)**: re-read
  `sse_recon.rs:160-370`. The on-disk "bitangent" triplet (`bitangent_x` from
  the position-quad trailing slot, `bitangent_y`/`bitangent_z` from the
  normal/tangent trailing bytes) is still assembled into `tangent.xyz` as
  ∂P/∂U, with sign derived from the on-disk tangent (∂P/∂V) — comment block at
  `sse_recon.rs:164-171` and `:298-310` explicitly documents the convention
  and matches the SKILL.md guard verbatim. Positions/normals route through
  `byroredux_core::math::coord::zup_to_yup_pos` (`:269`, `:292`). No
  regression — confirmed by direct re-read, not by trusting the prior report.
- Alpha-property cascade (`alpha_property_consumed`, #1201/#1202) — not
  touched by any of the 5 intervening commits; not re-derived this pass
  (verified in the 2026-07-02 audit and no diff landed in
  `crates/nif/src/import/material/`).

No findings.

## Dimension 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch — CLEAN

`crates/nif/src/blocks/shader.rs`

- Re-read `parse_shader_type_data` (`shader.rs:1299-1360+`): type 1 →
  `EnvironmentMap{env_map_scale}`, 5 → `SkinTint` (Color3), 6 → `HairTint`
  (Color3), 7 → `ParallaxOcc{max_passes, scale}`, 11 → `MultiLayerParallax`
  (4 fields), 14 → `SparkleSnow` (4 params), 16 → `EyeEnvmap` (cubemap scale +
  2 reflection centers) — all field counts match nif.xml and the Shader-Type
  Coverage Matrix below. No other numeric type has a match arm, so 0/2/3/4/
  8-10/12-13/15/17-19 all fall through to `None` as documented — confirmed no
  silent over-read.
- No commit since 2026-07-02 touched `crates/nif/src/blocks/shader.rs`,
  `crates/nif/src/import/material/`, or the PBR-lobe gate in
  `crates/renderer/shaders/include/`. Dispatch table and the
  `MAT_FLAG_PBR_BSDF` vanilla-unreachability guard are unchanged.

No findings.

## Dimension 3 — NPC Equip + FaceGen (M41) — CLEAN

`byroredux/src/npc_spawn.rs`, `crates/facegen/src/`

- `resolve_armor_mesh` / `expand_leveled_form_id` call sites unchanged
  (`npc_spawn.rs:437,457,485,653,1151,1205`) — no diff since last audit.
- Whiterun BanneredMare 6-named-NPC equip flow (OTFT.items + LVLI dispatch)
  not touched by any of the 5 intervening commits.
- Existing open gap, not re-reported: **#1659** (`SKY-D3-03`) —
  `BsDismemberSkinInstance` per-partition body-part flags are parsed but
  discarded at import; skinning geometry itself is unaffected. Confirmed
  still OPEN via `gh issue view 1659`.

No new findings.

## Dimension 4 — Multi-Master Load Order + TES5 Cell-Load Regression — CLEAN

`byroredux/src/cell_loader/load_order.rs`, `crates/plugin/src/esm/`

- `--master` FormID remap (#561/M46.0), `.STRINGS` multi-plugin wiring
  (`db5bb149`), ESL/light-master decode (#1554, `59d3f007`), and deleted-REFR
  tombstone skip (#1660, `2dc43106`) — none of these code paths changed in
  the 5 intervening commits.
- **#1731 fix verified in place** (`175ebf2c`, this session's most relevant
  diff): `crates/plugin/src/esm/reader.rs` now exposes
  `pub const FLAG_VISIBLE_WHEN_DISTANT: u32 = 0x0001_0000` beside the
  pre-existing private `FLAG_COMPRESSED`, plus
  `RecordHeader::is_visible_when_distant()`. Re-read the diff directly (not
  just the commit message): the constant and accessor are scoped exactly as
  described, 4 new regression tests pin flag-set / flag-unset / distinctness
  from the `0x20` deleted-REFR bit (#1660) / coexistence with
  `FLAG_COMPRESSED`. The fix explicitly does **not** wire the flag into any
  LOD-spawn consumer (deferred to LC-D7-01 per the commit body and the
  `object_lod.rs` doc comment) — this matches the SKILL.md's own
  "not a regression — forward scope" framing for the VWD full-model-culling
  gap. **Issue #1731 is CLOSED and the closure is legitimate** (verified via
  `gh issue view 1731 --json state` → `CLOSED`); no regression.
- `parse_real_skyrim_esm` (`SolitudeWinkingSkeever`) unchanged.
- Open, not new: **#1698** (`RT-1`, Dragonsreach FPS collapse
  321→8.7 FPS, ECS scheduler stalls). Confirmed still OPEN — a live perf
  regression, tracked separately, not a Dimension-4 parse/load-order defect.

No new findings.

## Dimension 5 — BSA v105 (LZ4) — CLEAN

`crates/bsa/src/archive/`

- No commits since 2026-07-02 touched `crates/bsa/`. v105 header / LZ4 block
  decode / zero-based numeric-sibling auto-load (`821a425b`) unchanged.

No findings.

## Dimension 6 — Specialty Blocks + Real-Data Rendering — CLEAN

`crates/nif/src/blocks/mod.rs`, `byroredux/src/cell_loader/terrain_lod_btr.rs`,
`byroredux/src/cell_loader/object_lod.rs`

- `#838` `BSLODTriShape` → `NiLodTriShape` / `BSMeshLODTriShape` →
  `BsTriShape::parse_meshlod` routing unchanged; no diff in `blocks/mod.rs`
  since last audit.
- `BsLagBoneController` / `BsProceduralLightningController` (#837) parsers
  unchanged.
- `.btr` / `.bto` LOD paths unchanged. The VWD-flag fix (#1731, above) is the
  one adjacent change, and it is additive/expose-only per Dimension 4.
- Not re-reported (already OPEN, unaffected): the `object_lod.rs` doc comment
  itself still correctly frames "consume `is_visible_when_distant()` to cull
  the full-detail model" as future scope (LC-D7-01), matching the SKILL.md's
  explicit non-regression guard for that exact language.

No findings.

## Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice) — CLEAN

`byroredux/src/material_translate.rs`,
`crates/core/src/ecs/components/material.rs`

- No commits since 2026-07-02 touched `material_translate.rs` or
  `components/material.rs`. Single-boundary `translate_material`,
  `resolve_pbr()` → `classify_glass_into_material` ordering, and the
  `EmissiveSource` Skyrim `Lighting` vs. `Effect` discriminator (#1280) are
  unchanged.

No findings.

## Shader-Type Coverage Matrix (Skyrim `BSLightingShaderType`)

| Type | Name | Trailing data | Parse | Import | Render |
|------|------|---------------|-------|--------|--------|
| 0 | Default | None | done | done | done |
| 1 | Environment Map | `env_map_scale` f32 | done | done | done |
| 2 | Glow | None (no GlowShader variant) | done | done | done |
| 3 | Parallax | None | done | done | done |
| 4 | Face Tint | None | done | done | done |
| 5 | Skin Tint | Color3 | done | done | done |
| 6 | Hair Tint | Color3 | done | done | done |
| 7 | Parallax Occ | max_passes + scale | done | done | partial |
| 8–10 | Landscape | None | done | done | done |
| 11 | Multi-Layer Parallax | 4 inner-layer fields | done | done | partial |
| 12–13 | Tree / LOD | None | done | done | done |
| 14 | Sparkle Snow | 4 params | done | done | partial |
| 15 | LOD HD | None | done | done | done |
| 16 | Eye Envmap | cubemap scale + 2 centers | done | done | partial |
| 17–19 | Cloud / Noise | None | done | done | done |

("partial" render = trailing params parsed and available in `MaterialInfo`
but the dedicated render path for that effect is not a distinct shader
branch yet; pre-existing feature gaps, unchanged since 2026-07-02, not parse
defects.)

## Cell-Load Regression Status

TES5 cells parse through the unified `esm/cell/` walker; compressed records
decompress; real `Skyrim.esm` walks 590 cells / 18113 statics / 37
worldspaces, `SolitudeWinkingSkeever` resolves (unchanged from 2026-07-02).
Whiterun BanneredMare control-bench figures (3216 ent / 362.8 FPS / fence
0.98) remain the ROADMAP Bench-of-record (R6a-stale-14, HEAD `1c26bc25`,
2026-06-03) — per ROADMAP.md this bench is now **437+ commits stale**
(Session 53 staleness note), so any current FPS claim is gated on a fresh
R6a-stale-15 run; this is a documented, tracked staleness gap, not a new
finding. This audit made no code changes and observed no entity-count or
parse-rate regression in any touched file.

## Existing Open Issues Touching Skyrim (not re-reported)

| # | Title | Dimension | State (verified this run) |
|---|-------|-----------|---------------------------|
| #1698 | RT-1: Skyrim Dragonsreach FPS collapse (321→8.7), scheduler stalls ~140 ms/frame | 4/6 (perf) | OPEN |
| #1659 | SKY-D3-03: BSDismemberSkinInstance per-partition body-part flags discarded at import | 3 | OPEN |

## Verified Closed (fix confirmed in place, no regression)

| # | Title | Verification |
|---|-------|--------------|
| #1731 | LC-D7-02: VWD record-header flag (0x00010000) not parsed | CLOSED; `FLAG_VISIBLE_WHEN_DISTANT` + `is_visible_when_distant()` land in `crates/plugin/src/esm/reader.rs` (`175ebf2c`), 4 new tests, scope correctly stops short of LOD-culling consumption (LC-D7-01, separately tracked). |

## Findings Total

- CRITICAL: 0
- HIGH: 0
- MEDIUM: 0
- LOW: 0
- **NEW findings: 0**

Skyrim SE support continues in a healthy, well-guarded state one day after a
clean comprehensive audit. All dimension-level regression guards defined in
`.claude/commands/audit-skyrim/SKILL.md` (the #838 BSLODTriShape routing
split, the SSE tangent-convention fix, the EmissiveSource discriminator, the
ESL/deleted-REFR/VWD flag decode trio, the zero-based BSA sibling loader)
remain intact against the current HEAD, and the one code change in this
window (#1731) is a correctly-scoped, test-covered, non-regressing fix.

---
*Recommended next step (carried over from 2026-07-02, still applicable): refresh
the R6a-stale bench-of-record (per ROADMAP.md) — it is the only Skyrim signal
currently stale enough (437+ commits) to hide a perf regression on the control
bench.*
