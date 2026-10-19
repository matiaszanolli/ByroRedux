# NIFAL-SD-02: `bs_sub_index` is listed as raw-parked with no consumer, but equip-time partition hiding reads it

Labels: low,nifal,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6129

**Severity**: LOW · **Dimension**: Nodes · **Tier violated**: `parked-not-leak` (ledger inaccuracy; not a leak) · **Games**: Skyrim SE, FO4 (`BSSubIndexTriShape`); FO3/FNV/Oblivion (`triangle_body_parts`)

## Location
`docs/engine/nifal.md` (`bs_sub_index` passthrough row); `crates/nif/src/import/types.rs` (`ImportedMesh::hide_skin_partitions`, `is_dismemberment_cap`); consumers `byroredux/src/npc_spawn/resumable/mod.rs`, `prebaked.rs`, `scene/nif_loader.rs`, `npc_spawn/seam_blend.rs`

## Description
The passthrough table says `bs_sub_index` is "raw-parked - dismemberment / locational-damage system" with no consumer, and the audit skill's Dim 4 first step expects zero consumers. In fact `ImportedMesh::hide_skin_partitions` reads `bs_sub_index` segments (and `skin.triangle_body_parts`) to remove triangles covered by worn armour, driven from the NPC `pre_spawn` hooks. `is_dismemberment_cap` is used to drop FO3/FNV gore caps at spawn and to exclude them as seam neighbours. None of `triangle_body_parts`, `hide_skin_partitions` or the cap drop appears in `nifal.md` (grep for `dismember`/`partition`/`triangle_body_parts` finds only the passthrough row).

## Evidence
This is a raw-tier to raw-tier transform before `translate_material` and feeds no canonical component, so it is not a leak. The skill's first-step grep only searches `byroredux/`, `renderer/`, `core/` and so stays empty even though the nif crate itself consumes the field.

## Impact
A future auditor reads "zero consumers" and may flag the legitimate consumer, or delete the field as dead. The dropped gore caps are a deliberate "until dismemberment exists" translation drop that the ledger does not record.

## Related
`nifal.md` section 2 "Passthroughs"; closed #2600, #1206.

## Suggested Fix
Change the `bs_sub_index` row to "consumed by `hide_skin_partitions` (equip-time skin hiding); the dismemberment system itself remains parked", and add a ledger row for `triangle_body_parts` / gore-cap drop.

Source: docs/audits/AUDIT_NIFAL_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
