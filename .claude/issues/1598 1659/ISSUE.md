# #1598: FO4-D6-LOW-01: MOVS (movable static) records parsed into index.movables but never queried

Severity: LOW · Dimension: ESM Architecture Records
Location: `crates/plugin/src/esm/cell/mod.rs:896-912` (`movables` map); `byroredux/src/cell_loader/` (no consumer)
Source: `docs/audits/AUDIT_FO4_2026-06-14.md` (FO4-D6-LOW-01)

`parse_movs` decodes EDID/MODL/LNAM/ZNAM/DEST/VMAD into `MovableStaticRecord`;
records land in `EsmCellIndex.movables`, merged across plugins. No production
code reads `index.movables`. MOVS REFRs still render via the MODL-only
catch-all into `index.statics`, so the mesh appears, but loop/activate-sound
IDs, destruction flag, and script flag are inert. Zero vanilla FO4 records
(count 0) — DLC/mod-content only. Suggested: fold into the same
categorised-spawn work item as #1359 (CONT, already fixed).

# #1659: SKY-D3-03: BSDismemberSkinInstance per-partition body-part flags parsed but discarded at import

Severity: LOW · Dimension: NPC Equip + FaceGen
Location: parser `crates/nif/src/blocks/skin.rs:370-401`; import `crates/nif/src/import/mesh/skin.rs:36-44,135-143`

`BsDismemberSkinInstance::parse` reads per-partition `part_flag`/`body_part`
into `partitions: Vec<BodyPartInfo>`, but both extractors
(`extract_skin_ni_tri_shape`/`extract_skin_bs_tri_shape`) only read
`inst.base.*` — `partitions` is dropped before reaching `ImportedSkin`.
Cosmetic-only (Skyrim NPC body/skin clips through armor seams). Suggested
fix: surface `BodyPartInfo` onto `ImportedSkin` symmetrically on both
extraction paths for a future slot-hiding consumer.
