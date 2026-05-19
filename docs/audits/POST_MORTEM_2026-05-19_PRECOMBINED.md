# Post-mortem — FO4 PreCombined Mesh gap missed by 2026-05-18 audit

**Date**: 2026-05-19
**Surface**: Diamond City Dugout Inn (FO4) rendered as "props floating in a void" —
floors, walls, ceilings invisible. Trail commit: [#1188 — load.rs](../../byroredux/src/cell_loader/load.rs).

## What the user saw

Loading `DmndDugoutInn01` showed the bar's clutter (couch, lamps, posters, NPCs)
correctly placed but the architecture — every wall, every floor, every ceiling
panel, the back-bar shelving — was simply absent. Per-REFR rendering was working;
the cell was just missing ~100 architectural placements.

## Root cause

FO4+ ships a **PreCombined Mesh system**: the Creation Kit bakes per-cell-tile
architecture (XCRI / XPRI sub-records on CELL) into single `_oc.nif` files under
`meshes\precombined\<cell_fid:08x>_<hash:08x>_oc.nif`. The original architecture
REFRs stay in the ESM but are flagged "absorbed" via XPRI so the runtime knows
to skip them; the precombined NIF is rendered instead.

Three things were wrong in our stack:

1. The CELL parser never read XCRI / XPRI — those bytes flowed past without
   surfacing the precombined-hash list or the absorbed-REFR set.
2. There was no precombined-NIF spawn pass.
3. The walker had a generic skip on `BSMultiBoundNode` subtrees containing
   `BSPackedCombinedGeomDataExtra` (SK-D4-04 / #564) which would also have
   dropped these subtrees even after the spawn pass landed, except…
4. …vanilla `_oc.nif` files ship BSTriShape blocks with `num_vertices = 0`. The
   actual vertex / triangle bytes live in `Fallout4 - Geometry.csg`, a 5 GB
   binary blob next to the BA2s keyed by `BSPackedGeomObject.{filename_hash,
   data_offset}`. We have no CSG reader.

## What the 2026-05-18 audit (DIM4, FO4 ESM Architecture Records) missed

The audit was scoped to SCOL / MOVS / PKIN / TXST. The PreCombined Mesh family
of sub-records (XCRI / XPRI / XCWT) wasn't named anywhere in the FO4 audit
prompt or its findings, despite being a **first-class FO4 cell-render
requirement** that every vanilla settlement / Diamond City interior depends on.

Why it slipped:

- The skill's audit checklist had a hardcoded list of architecture record types
  carried forward from the original drafting. XCRI / XPRI weren't on the list
  and nothing forced the auditor to enumerate "what CELL sub-records does FO4
  add over Skyrim?" empirically against the data.
- The renderer-side counterpart (precombined NIF format, CSG companion file)
  is one degree of separation away: the audit's dim-1 (NIF BSVER 130) checklist
  enumerates BSTriShape variants and the half-float vertex format, but the
  precombined-shared variant — whose geometry is *externalized* into CSG — has
  no entry in our NIF spec docs and isn't called out as a known FO4-only
  carrier.

## Forward action

Three concrete updates to the audit infrastructure:

1. **audit-fo4 DIM4 — add XCRI / XPRI / XCWT** to the architecture-records
   checklist with a "must surface in CELL parser, must propagate to absorbed-
   refs set on CellData" success criterion. This catches the parser-side gap
   directly.
2. **audit-fo4 DIM1 — add BSPackedCombinedSharedGeomDataExtra companion-file
   check**: any BSTriShape walking path must verify whether `num_vertices = 0`
   on the wire indicates a CSG-backed shape. Today that's a silent "produces
   zero meshes" failure mode.
3. **New milestone — `M-FO4-PRECOMBINED`** for the actual CSG / PSG reader.
   The fallback we shipped today (conditional absorption: when precombined
   spawn fails, render the original REFRs) matches Bethesda's
   `bUseCombinedObjects=0` behaviour and is good enough for correctness, but
   pays the cost of N individual placements vs. one baked combine per cell
   tile. M-FO4-PRECOMBINED reclaims that.

## Today's fix (shipped under #1188)

- `crates/plugin/src/esm/cell/walkers.rs` — XCRI + XPRI parsers; populate
  `CellData.precombined_mesh_hashes` and `CellData.absorbed_refs`.
- `crates/plugin/src/esm/cell/mod.rs` — add the two fields and `PlacedRef.form_id`
  (the REFR's own placement-level identity, distinct from `base_form_id`,
  needed to match against the XPRI list).
- `byroredux/src/cell_loader/precombined.rs` (new) — `spawn_precombined_meshes`:
  walks the hash list, extracts each `_oc.nif` via `tex_provider.extract_mesh`,
  parses through the standard NIF pipeline, spawns if any mesh survives import.
  Today: zero entities on vanilla FO4 because of the CSG dependency.
- `byroredux/src/cell_loader/load.rs` — call `spawn_precombined_meshes` FIRST,
  then conditionally honor `cell.absorbed_refs` only when the spawn count is
  non-zero. With zero precombined geometry, the REFR loader falls through to
  full per-REFR rendering and the cell looks correct.
- `crates/nif/src/import/walk/mod.rs` — doc-only: explain why the
  `has_packed_combined_geom_extra` skip stays in place for the Shared variant
  (BSTriShape children carry no inline data, so walking them is a no-op
  regardless of whether the parent skip fires).

Dugout Inn now renders with 833 entities, 157 lights, 530 textures, 667 draws
at 267 wall-FPS, 3.74 ms/frame.
