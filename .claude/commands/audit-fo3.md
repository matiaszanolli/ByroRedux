---
description: "Per-game audit of Fallout 3 compatibility — NIF v20.2.0.7, BSA v104, ESM via FNV-shared parser"
argument-hint: "--focus <dimensions>"
---

# Fallout 3 Compatibility Audit

Deep audit of ByroRedux readiness for **Fallout 3** content.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game data locations, methodology, deduplication rules, and finding format.

## Game Context

| Aspect            | State                                                                |
|-------------------|----------------------------------------------------------------------|
| NIF format        | v20.2.0.7 (BSVER 34)                                                 |
| BSA format        | v104 ✓                                                               |
| ESM parser        | Shares parser with FNV (same record set)                             |
| Parse rate        | 100.00% (10989 / 10989)                                              |
| Validation demo   | Megaton Player House (1609 entities, 199 textures, 42 FPS)           |
| Reference data    | `/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/`       |

### Known Specifics

- NIF shader chain: `BSShaderPPLightingProperty` with refraction / parallax / bump map / normal map (normal map is a dedicated slot in FO3+; Oblivion still reads normals from the bump slot).
- FO3 shader flags use the legacy single-u32 layout, not the FO4 u32-pair.
- Block types: `BSSegmentedTriShape` (biped body parts), legacy particle stack, `BSShaderNoLightingProperty`.
- Havok blocks (30 types) skip via `block_size` — v20.2.0.7 has the size table so recovery works.
- XCLL interior lighting identical to FNV — uses the same `CellLightingRes` path.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 6.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/fo3`.
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Fallout 3 goty/Data/` exists; if not, note which dimensions lose real-data validation.

## Phase 2: Launch Dimension Agents (parallel)

### Dimension 1: NIF v20.2.0.7 Parser Correctness (FO3 subset)
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/blocks/properties.rs`, `crates/nif/src/blocks/*.rs`
**Checklist**: `BSShaderPPLightingProperty` field completeness (refraction strength, refraction period, parallax passes, parallax scale, bump map tiling). Normal-map slot location in `BSShaderTextureSet` (slot 1 in FO3, differs from Oblivion which uses the bump slot via NiTexturingProperty). `BSShaderNoLightingProperty` (UI/sky). `BSSegmentedTriShape` vertex index handling. Legacy particle controllers still reachable in FO3-era meshes. Stream position audits — any block types that pass on FNV but might be FO3-only.
**Output**: `/tmp/audit/fo3/dim_1.md`

### Dimension 2: BSA v104 Archive (Meshes + Textures)
**Subagent**: `general-purpose`
**Entry points**: `crates/bsa/src/archive.rs`
**Checklist**: Verify `Fallout - Meshes.bsa` lists + extracts cleanly. Texture archives (`Fallout - Textures.bsa`) DDS extraction produces valid BC1/BC3/BC5 headers. Sound archives — not needed for rendering but should open without error. Folder hash collisions across Fallout 3's ~150 subdirectories. Compare metrics vs FNV (same format, should behave identically).
**Output**: `/tmp/audit/fo3/dim_2.md`

### Dimension 3: ESM Record Coverage (Fallout3.esm)
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/records/`, `crates/plugin/src/esm/cell.rs`
**Checklist**: Parse `Fallout3.esm` through the shared FNV parser. Are there FO3-unique record types absent from FalloutNV.esm (e.g., pre-FNV formats for NPC_, DIAL, INFO)? Compare category counts: items, containers, NPCs, factions, globals, settings vs the FNV baseline of 13,684 structured records. CELL record XCLL/RCLR layout identity vs FNV. Water records (WATR) for rivers / ponds — parsed or skipped? NAVM differences?
**Output**: `/tmp/audit/fo3/dim_3.md`

### Dimension 4: Rendering Path for FO3 Shaders
**Subagent**: `renderer-specialist`
**Entry points**: `crates/nif/src/import/material.rs`, `byroredux/src/render.rs`, `crates/renderer/shaders/triangle.frag`
**Checklist**: `BSShaderPPLightingProperty` flag bits mapped correctly (decal, alpha-test, two-sided, glow, window — per-game flag bit positions are NOT identical across FO3/FNV/Skyrim). Normal map handle extracted from `BSShaderTextureSet[1]`, not from the bump slot (Oblivion trap). Parallax depth / parallax map slot routed to the parallax branch in the fragment shader (if enabled). Cubemap / environment map slot. Refraction strength → glass tint path. Decal z-bias uses the NIF-flagged decal detection (not heuristic).
**Output**: `/tmp/audit/fo3/dim_4.md`

### Dimension 5: Real-Data Validation
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, `crates/nif/tests/parse_real_nifs.rs`
**Checklist**: Current parse rate on `Fallout - Meshes.bsa` (expect 100% / 10989). `BYROREDUX_FO3_DATA=... cargo test -p byroredux-nif --test parse_real_nifs -- --ignored fo3` passes. Pick Megaton Player House interior (already validated — should match 1609 entity baseline). Load one creature mesh (e.g. deathclaw), verify skinning data extraction. Pick a UI/menu element (NoLighting shader) and verify it routes through the non-Phong path. Pick one FaceGen head mesh — parses but may not render fully.
**Output**: `/tmp/audit/fo3/dim_5.md`

### Dimension 6: Blockers & Game-Specific Quirks
**Subagent**: `general-purpose`
**Entry points**: `ROADMAP.md`, `byroredux/src/cell_loader.rs`, `byroredux/src/npc_spawn.rs`
**Checklist**: Can Fallout 3 cells be loaded end-to-end via the same `--esm Fallout3.esm --cell <id>` CLI as FNV? Any hardcoded FNV-only paths in the cell loader? Weather (WTHR) / CLMT records — FO3 has them; are they pulled from the FNV-shared parser correctly? Exterior worldspace loading — FO3's Wasteland is a different WRLD form ID; any FNV-hardcoded assumptions (e.g. specific worldspace names, origin coords)? FO3-specific CLMT sun position curves.
**M41.0 long-tail regression guards (shared with FNV — Session 29)**:
- B-spline pose-fallback (#772, 3c32a5e): gated on `FLT_MAX` sentinel. B-splines are reachable on FO3 just as on FNV (`feedback_bspline_not_skyrim_only.md`).
- AnimationClipRegistry dedup (#790, da99d15): case-insensitive interning by lowercased path; without it, one keyframe set leaks per cell load.
- NPC hand-mesh load (#793 / M41-HANDS, da8d7e2): `lefthand.nif` + `righthand.nif` loaded alongside `upperbody.nif` on kf-era NPCs. Megaton dwellers depend on this — bodies with no hands = #793 regression.
- Megaton parse-side baseline (Session 12, #455+): 929 REFRs (down from 1609 post-NIF-expand). Cell-loader stale-comment cleanup landed in #822 FNV-D3-DOC (ca6be24). Any audit citing the 1609 number is referencing pre-expand stats — confirm against current `cell_loader.rs` comments before reporting.
**Output**: `/tmp/audit/fo3/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/fo3/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_FO3_<TODAY>.md` with structure:
   - **Executive Summary** — Compatibility level, delta vs FNV baseline (what's the same, what diverges).
   - **Dimension Findings** — Grouped by severity per dimension.
   - **FNV-Shared Surface** — Explicit list of record types / block types / shader paths that FO3 inherits from FNV coverage. Any FO3-only gaps in those paths.
   - **Validation Status** — Interior cell load status, exterior cell load status, creature / NPC status.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_FO3_<TODAY>.md`
