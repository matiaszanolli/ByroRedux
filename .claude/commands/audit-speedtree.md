---
description: "Audit the SpeedTree (.spt) TLV parser + placeholder-billboard fallback shipped in Session 33 Phase 1"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# SpeedTree Subsystem Audit

Audit the `byroredux-spt` crate (Session 33 Phase 1) for TLV walker correctness, tag coverage against the FNV/FO3/Oblivion `.spt` corpus, the ≥95% acceptance gate, and the placeholder-billboard fallback that keeps cell loads alive when a tree fails to decode.

**Architecture**: Single-pass — small enough to run dimensions inline rather than spawning Tasks.

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Scope

**Crate**: `crates/spt/src/` (Session 33 Phase 1 — TLV walker + placeholder fallback).

**Cross-cuts**:
- `byroredux/src/cell_loader/refr.rs` — extension switch routes `.spt` to the SpeedTree importer instead of NIF
- `byroredux/src/scene/nif_loader.rs` — `--tree` direct-visualisation CLI entry (`--tree trees\\joshua01.spt`)
- `crates/plugin/src/esm/records/tree.rs` — TREE record parser (was previously falling into the generic record path and losing texture/billboard data)

**Phase 1 acceptance** (ground truth — verify before reporting):
- Single-file dissector + tag dictionary recovered against the corpus
- TLV walker ≥95% on FNV/FO3/Oblivion `.spt` corpus (`>5 GB joshua01.spt` etc.)
- Importer placeholder fallback — un-decoded trees render as a billboard card (better than parse panic)
- `.spt` references in cell records route to the SpeedTree importer, not NIF
- `--tree` smoke test passes

**Future phases (NOT yet shipped — do not flag as missing unless scope includes them)**: full geometry recovery (real branch/leaf mesh, not billboard), wind-bone animation from `BSTreeNode`, distance-LOD swap, baked-shadow lookup.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 5.
- `--depth shallow|deep`: `shallow` = walker contract check; `deep` = run corpus against the walker + diff against the baseline. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: TLV Format | Tag Coverage | Corpus Acceptance | Placeholder Fallback | Routing & CLI

## Phase 1: Setup

1. `mkdir -p /tmp/audit/speedtree`
2. `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels --search "speedtree OR .spt OR TREE" > /tmp/audit/speedtree/issues.json`
3. Confirm corpus path: `find /mnt/data/SteamLibrary/steamapps/common -iname '*.spt' 2>/dev/null | head` — and the in-BSA paths (`trees\\*.spt` in FNV `Fallout - Meshes.bsa`, FO3 `Fallout - Meshes.bsa`, Oblivion `Oblivion - Meshes.bsa`)

## Phase 2: Dimensions

### Dimension 1: TLV Format Correctness
**Entry points**: `crates/spt/src/` (top-level walker + tag dictionary), `crates/spt/tests/`
**Checklist**:
- Header magic + version bytes recognised across FNV/FO3/Oblivion variants (TLV format isn't versioned by a global field — verify each entry point claims its own header)
- Tag-length-value walker correctly skips unknown tags using their length (no byte-misalignment cascade past the first unknown tag)
- Length field byte-width is consistent (LE u32 per current dissector; flag if any variant ships u16)
- Walker stops cleanly at EOF — no off-by-one read past file end on the last tag
- Negative / zero / pathological lengths bail with `Err`, not panic — `.spt` is artist-shipped data, must not crash the cell loader
- Endian: LE everywhere (no big-endian fallback path); compile-error gate if a future big-endian host is added

### Dimension 2: Tag Coverage
**Entry points**: tag dictionary module under `crates/spt/src/`, the analyzer outputs
**Checklist**:
- ~40 known tags map to typed data structures (texture path, billboard descriptor, branch geometry, leaf cluster, wind params, LOD distances)
- Any tag that appears in the corpus at ≥1% frequency MUST have either a parser or an explicit skip-with-rationale comment
- Texture-path tags resolve through the same `resolve_texture` / sibling-BSA auto-load path that NIF uses — verify no parallel "spt resolver" duplicates the logic
- Billboard tag captures: texture, world-space width/height, mip bias — these flow into the placeholder importer
- "Last tag wins" vs "first tag wins" semantics for duplicate tags — confirm and document

### Dimension 3: Corpus Acceptance (≥95% gate)
**Entry points**: `crates/spt/tests/parse_real_spt.rs` (or equivalent), corpus location resolved via env-var (mirror the NIF `BYROREDUX_*_DATA` pattern)
**Checklist**:
- Acceptance harness runs over FNV + FO3 + Oblivion `.spt` corpus and reports walker-clean rate
- Threshold: ≥95% (Phase 1 gate). Under-95% = audit finding even if every per-file failure is graceful
- Walker-clean ≠ semantically-correct — failures should be bucketed (truncation, unknown tag exceeding length, header mismatch, etc.) with corpus-wide histogram
- Regression-guard sample: 3-5 specific `.spt` files pinned by SHA, in-tree, should parse byte-stable across runs
- Memory: walker should NOT load the whole file before iterating (mmap or chunked-read pattern) — verify on the largest `joshua01.spt` corpus entry

### Dimension 4: Placeholder Fallback
**Entry points**: `crates/spt/src/import.rs` (or equivalent), `byroredux/src/scene/nif_loader.rs`, billboard mesh creation in `crates/renderer/src/mesh.rs`
**Checklist**:
- When the walker fails OR no billboard tag was captured, importer returns a placeholder card (Quad mesh + magenta-checker placeholder texture OR the texture from the billboard tag if available)
- Placeholder return is non-null — must never `Err` out of the cell loader (graceful degradation is the Phase 1 contract)
- Placeholder mesh sized to a sane default (~1 m × 2 m for trees) — drift here means juniper bushes render as 100-m skyboxes
- Billboard normal faces camera (Y-up world, billboard quad in XY plane with normal +Z) — verify Z-up→Y-up coord conversion is applied (the trap NIF importer fell into for years)
- Two-sided rendering enabled on the placeholder (foliage shouldn't disappear when camera looks from behind)
- Material slot: placeholder goes through the same `MaterialTable::intern` path as NIF meshes — verify dedup applies (a forest of 1000 juniper bushes should produce 1 material, not 1000)

### Dimension 5: Routing & CLI
**Entry points**: `byroredux/src/cell_loader/refr.rs` (extension dispatch), `byroredux/src/scene/nif_loader.rs` (`--tree` flag), `crates/plugin/src/esm/records/tree.rs` (TREE parser)
**Checklist**:
- Cell-loader `.spt` route fires when the REFR's base record is TREE and the model path ends in `.spt`; mixed `.nif` + `.spt` in the same cell coexist
- `.spt` references in BSA archives resolve through the same lookup chain as `.nif` (sibling-BSA auto-load, AE pipeline-path strip applied if relevant)
- `--tree path/to/x.spt` CLI entry instantiates the same code path as the cell-loader route (no parallel "direct viz" stub that drifts from the in-engine path)
- TREE record parser (Session 33 dedicated dispatch) captures texture / billboard / shadow data without dropping fields — pre-fix every `.spt`-referencing TREE silently lost its authoring
- Cell unload despawns the SpeedTree entities cleanly; no leaked BLAS entries for placeholder billboards
- Failed `.spt` import does NOT block the rest of the cell from loading (graceful degradation)

## Phase 3: Output

Write findings to **`docs/audits/AUDIT_SPEEDTREE_<TODAY>.md`** following the base finding format. Suggest `/audit-publish` on completion.
