---
description: "Deep audit of NIF parser — block correctness, version handling, stream position, coverage"
argument-hint: "--focus <dimensions> --game <fnv|skyrim|oblivion|fo4> --corpus <path>"
---

# NIF Parser Audit

Deep audit of the NIF binary format parser for correctness across all game versions. Tests against real game data when available.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game data locations, methodology, deduplication, context rules, and finding format.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 5.
- `--game <name>`: Focus on specific game variant: `fnv`, `fo3`, `skyrim`, `oblivion`, `fo4`, `fo76`, `starfield`. Default: all detected.
- `--corpus <path>`: Path to a directory of extracted NIF files for bulk testing.

## Extra Per-Finding Fields

- **Dimension**: Block Parsing | Version Handling | Stream Position | Import Pipeline | Coverage
- **Game Affected**: Which NifVariant(s) are affected

## Phase 1: Setup

1. Parse `$ARGUMENTS`
2. `mkdir -p /tmp/audit/nif`
3. Fetch dedup baseline
4. Check which game data directories exist (from `_audit-common.md` game data locations)

## Phase 2: Launch Dimension Agents

### Dimension 1: Block Parsing Correctness
**Entry points**: `crates/nif/src/blocks/*.rs` (all block parsers)
**Checklist**: Every field read matches nif.xml spec (compare struct fields vs nif.xml `<add>` elements), NiObjectNETData.parse() called correctly by all blocks, NiAVObjectData.parse() vs parse_no_properties() used correctly, BSShaderPropertyData.parse_fo3() used by FO3-era shaders only, block_size adjustment warnings (compile list from real NIF files if corpus available), boolean type correctness (read_bool vs read_byte_bool per nif.xml type annotation).
**Output**: `/tmp/audit/nif/dim_1.md`

### Dimension 2: Version Handling
**Entry points**: `crates/nif/src/version.rs`, all `stream.variant()` and `stream.version()` calls in block parsers
**Checklist**: NifVariant::detect() covers all known user_version/user_version_2 combinations, feature flags match nif.xml version conditions (has_properties_list, has_shader_alpha_refs, etc.), bsver() return values are correct, version comparisons use correct operators (>= vs >, < vs <=), Oblivion v20.0.0.5 handling (no block sizes, u16 flags, inline strings).
**Output**: `/tmp/audit/nif/dim_2.md`

### Dimension 3: Stream Position Integrity
**Entry points**: `crates/nif/src/lib.rs` (parse_nif block loop), all block parsers
**Checklist**: Every parsed block consumes exactly block_size bytes (when known), no unconditional reads that may exceed block boundaries, skip logic for unknown blocks works correctly, SVD decomposition doesn't read extra bytes, NiTexturingProperty consistent 1-byte shortfall (known issue — diagnose root cause).
**If corpus available**: Parse all NIFs in corpus and report stream position mismatches by block type with frequency counts.
**Output**: `/tmp/audit/nif/dim_3.md`

### Dimension 4: Import Pipeline Correctness
**Entry points**: `crates/nif/src/import.rs` (import_nif, import_nif_scene, walk_node_hierarchical, walk_node_flat, extract_mesh, extract_bs_tri_shape, find_texture_path, find_alpha_property, find_two_sided, find_decal)
**Checklist**: All NiAVObject fields accessed via `.av.*` (no stale field access), shader property lookup covers all shader types for each game variant, texture path resolution works for NiTexturingProperty (Oblivion), BSShaderPPLightingProperty (FO3/FNV), BSLightingShaderProperty (Skyrim), BSEffectShaderProperty (Skyrim+), coordinate conversion (Z-up to Y-up) applied consistently, decal flag detection covers all shader flag bit positions per game.
**Output**: `/tmp/audit/nif/dim_4.md`

### Dimension 5: Coverage Gaps
**Entry points**: `crates/nif/src/blocks/mod.rs` (parse_block dispatch), `docs/legacy/nif.xml`
**Checklist**: List all block type names that appear in real game NIFs (from corpus or BSA listing) but are not in the parse_block dispatch table, count NiUnknown fallbacks per game, identify which missing block types cause cascading failures (blocks without block_size in Oblivion format), estimate coverage percentage per game.
**Output**: `/tmp/audit/nif/dim_5.md`

## Phase 3: Merge

1. Read all `/tmp/audit/nif/dim_*.md` files
2. Combine into `docs/audits/AUDIT_NIF_<TODAY>.md` with structure:
   - **Executive Summary** — Coverage per game, total mismatches, critical gaps
   - **Block Type Coverage Matrix** — Table of block types × games (parsed/skipped/unknown)
   - **Findings** — Grouped by severity
   - **Prioritized Fix Order** — Blocks needed for rendering first, then animation, then collision
3. Remove cross-dimension duplicates

Suggest: `/audit-publish docs/audits/AUDIT_NIF_<TODAY>.md`
