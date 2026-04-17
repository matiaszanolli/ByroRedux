---
description: "Per-game audit of Oblivion (TES4) compatibility — NIF v20.0.0.5, BSA v103, ESM stubs"
argument-hint: "--focus <dimensions>"
---

# Oblivion Compatibility Audit

Deep audit of ByroRedux readiness for **The Elder Scrolls IV: Oblivion** content.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game data locations, methodology, deduplication rules, and finding format.

## Game Context

| Aspect            | State                                                              |
|-------------------|--------------------------------------------------------------------|
| NIF format        | v20.0.0.5 (no block sizes, inline strings, u16 flags)              |
| BSA format        | v103 — archive opens, **decompression NOT WORKING** (open blocker) |
| ESM parser        | Stub                                                               |
| Parse rate        | 100.00% on 8032 NIFs (header fixes + N26 coverage sweep)           |
| Cell loading      | Deferred until BSA v103 decompression lands                        |
| Reference data    | `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`           |

### Known Quirks (do NOT re-derive — verify still hold)

- `user_version` only exists for files ≥ 10.0.1.8. Older NetImmerse files have `num_blocks` at the position `user_version` would occupy.
- `BSStreamHeader` conditional is `version == 10.0.1.2 || user_version >= 3` (not `user_version >= 10`).
- `NiTexturingProperty` reads `uint` count directly — **do NOT add a leading `Has Shader Textures: bool`**, nif.xml is wrong here and the Gamebryo 2.3 source is authoritative. Regressing this breaks every Oblivion clutter/book/furniture mesh (`afab3e7`).
- Pre-Gamebryo v3.3.0.13 files (e.g. `meshes/marker_*.nif`) inline type names as sized strings instead of a global type table. Parser returns an empty `NifScene` with a debug log rather than failing.
- Oblivion has **no per-block size table**. A single mis-aligned read poisons every subsequent block. Recovery via N23.10's `block_size`-advance path is unavailable.
- `as_ni_node` walker must unwrap every NiNode subclass (`BsOrderedNode`, `NiBillboardNode`, `NiSwitchNode`, `NiLODNode`, etc.) so scene-graph walks descend correctly.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 6.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/oblivion`.
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Oblivion/Data/` exists; if not, note which dimensions lose real-data validation.

## Phase 2: Launch Dimension Agents (parallel)

### Dimension 1: NIF v20.0.0.5 Parser Correctness
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/header.rs`, `crates/nif/src/stream.rs`, `crates/nif/src/blocks/*.rs`, `docs/legacy/nif.xml`
**Checklist**: Header `user_version` threshold (10.0.1.8, not 10.0.1.0). BSStreamHeader dual condition (`version == 10.0.1.2 || user_version >= 3`). NiTexturingProperty reads u32 count raw (no bool gate — regression guard). Inline-string block type handling for v3.3.0.13. No-block-size path does NOT rely on `block_size` for recovery. u16 vs u32 flag width per block. Oblivion-only block types dispatched: NiKeyframeController, NiSequenceStreamHelper, NiBillboardNode + 12 NiNode subclasses, NiLight hierarchy, NiUVController, NiCamera, NiTextureEffect, legacy particle stack (13 types), 11 BSShader*Property aliases.
**Output**: `/tmp/audit/oblivion/dim_1.md`

### Dimension 2: BSA v103 Archive
**Subagent**: `general-purpose`
**Entry points**: `crates/bsa/src/archive.rs`
**Checklist**: BSA v103 header recognition (version byte). Hash function produces correct folder/file hashes. Decompression path — **this is the known blocker**. Identify the exact failure mode: does zlib fail on truncated streams? Is the compression flag being read correctly? Is the hash-to-offset table walked correctly? Is there a v103-specific field layout difference vs v104 that the reader is missing? Check whether `meshes/*.nif` extraction succeeds even for uncompressed entries. Report the smallest reproducer.
**Output**: `/tmp/audit/oblivion/dim_2.md`

### Dimension 3: ESM Record Coverage
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/`, `crates/plugin/src/legacy/tes4.rs`
**Checklist**: TES4 header format differences (e.g., HEDR version 1.0 vs 0.94, group structure). What record types are Oblivion-unique (SPEL, ENCH, MGEF, BOOK, etc. — subset vs FNV)? Dialog/Quest record format: DIAL/INFO differ between Oblivion and later titles. CELL record format for Oblivion (XCLL, RCLR, etc.). Does `parse_esm_cells()` walker handle Oblivion's group layout? What would minimum "render a cell" require from the parser?
**Output**: `/tmp/audit/oblivion/dim_3.md`

### Dimension 4: Rendering Path for Oblivion Shaders
**Subagent**: `renderer-specialist`
**Entry points**: `crates/nif/src/import/material.rs`, `crates/nif/src/import/walk.rs`, `crates/renderer/shaders/triangle.frag`
**Checklist**: NiTexturingProperty → `MaterialInfo` pipeline (base slot 0, dark slot 1, normal from bump slot per `#131`, detail, glow, gloss). NiMaterialProperty color mapping (raw monitor-space per 0e8efc6). NiAlphaProperty blend factor extraction (ensure all Gamebryo AlphaFunction enum values route correctly). NiStencilProperty / NiZBufferProperty / NiVertexColorProperty / NiSpecularProperty / NiWireframeProperty / NiDitherProperty / NiShadeProperty — do we honor them or drop them silently? Vertex color interaction with material color.
**Output**: `/tmp/audit/oblivion/dim_4.md`

### Dimension 5: Real-Data Validation
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, `crates/nif/tests/parse_real_nifs.rs`
**Checklist**: Current parse rate on `Oblivion - Meshes.bsa` (expect 100% / 7963+). Run `BYROREDUX_OBLIVION_DATA=... cargo run -p byroredux-nif --example nif_stats` and report block type histogram. Cross-check against the N26 coverage sweep — are any new block types appearing since then? Identify the remaining no-block-size files (if any) and whether they render meaningfully or are debug placeholders. Pick 3 representative interior meshes (e.g. Anvil Heinrich Oaken Halls chandelier, a book, a creature head) and trace them through `import_nif_scene` → verify expected mesh count + material chain.
**Output**: `/tmp/audit/oblivion/dim_5.md`

### Dimension 6: Blockers & Game-Specific Quirks
**Subagent**: `general-purpose`
**Entry points**: `ROADMAP.md` (Known Issues), `docs/audits/`
**Checklist**: Is BSA v103 decompression still the primary blocker? Does the `--bsa` CLI path work for Oblivion archives (open + list, even if extract fails)? Are there Oblivion-specific record types the cell loader would need beyond the FNV-aligned set? Are there animation blocks that parse but cannot play because scene-graph name resolution is missing? Does the pre-Gamebryo v3.3.0.13 fallback log as `warn` or `debug` (spam risk on full archive sweeps)? Any 100%-parse-rate NIFs that would still render wrong visually (e.g., legacy particle emitters that parse but don't route to the renderer)?
**Output**: `/tmp/audit/oblivion/dim_6.md`

## Phase 3: Merge

1. Read all `/tmp/audit/oblivion/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_OBLIVION_<TODAY>.md` with structure:
   - **Executive Summary** — Current compatibility level (NIF parse, archive extract, ESM parse, render end-to-end), top blockers in priority order.
   - **Dimension Findings** — Grouped by severity per dimension.
   - **Blocker Chain** — Sequential list of what must land to reach "interior cell renders" (e.g., BSA v103 decompression → minimal TES4 ESM → ...).
   - **Regression Guard List** — Issues previously fixed that this audit verified are still correct (NiTexturingProperty u32 count, BSStreamHeader conditional, `user_version` threshold).
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_<TODAY>.md`
