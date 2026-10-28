# FO4-2026-10-27-02: FO4 BPTD `BPND` (101 B) is decoded with the FO3/FNV 84-byte layout; parts are garbage and a vanilla load raises 152 false remap warnings

Labels: low, esm-plugin, legacy-compat, game:fo4, bug
Source: docs/audits/AUDIT_FO4_2026-10-27.md

# FO4-2026-10-27-02: FO4 BPTD `BPND` (101 B) is decoded with the FO3/FNV 84-byte layout; parts are garbage and a vanilla load raises 152 false remap warnings

**Source**: `docs/audits/AUDIT_FO4_2026-10-27.md` finding `FO4-2026-10-27-02` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `crates/plugin/src/esm/records/misc/equipment.rs` - `parse_bptd`, BPND arm (fixed offsets 0-20, `remap_fid` on @12 and @16); `crates/plugin/src/esm/records/dispatch_misc_gameplay_b.rs` BPTD dispatch arm (no game gate). Introduced by aebcc99a1 (Refs #6350), whose layout was corpus-established against FalloutNV.esm and Fallout3.esm only.

## Description
`parse_bptd` reads the BPND payload at the FO3/FNV 84-byte offsets for every game. FO4's BPND is 101 bytes with a different layout, so the decoded `BptdPart` fields are garbage and `generic_debris` / `explosion` are passed through `remap_fid` as bogus FormIDs.

## Evidence
BPND sizes per master: Fallout4.esm (40 BPTD) 405 payloads of **101 B**; FalloutNV.esm 304 x 84 B; Fallout3.esm 207 x 84 B; Skyrim SE (base + 2 DLC) 108 x 84 B. Only FO4 differs.

A decoded FO4 payload: in Fallout4.esm `DefaultBodyPartData` (0x1D) every part's BPND starts `00 00 80 3f | 00x8 | 00 00 80 3f ...`. The FNV layout reads the float 1.0 at @12 as `generic_debris` FormID 0x3F800000 and reads `part_type`, `to_hit_chance`, `health_percent` as 0.

Warning count: on the vanilla 7-master load each DLC's BPTD is parsed with one master; the 0x3F... values reach `FormIdRemap::remap`'s multi-plugin out-of-range arm (`reader.rs` `log::warn!`): DLCRobot 26, DLCCoast 86, DLCNukaWorld 40 = **152 false "FormID 3f800000 has mod_index 63 but plugin has 1 masters" warnings**. Fallout4.esm has no masters, so its 405 values log at debug only.

## Impact
Today: no gameplay effect - FO4's `CharacterRulesProfile` has `vats: None`, so `build_vats_model` returns `None` and `vats_scan` / `resolve_limb_table` never read the table. The 152 warnings bury real remap failures in the log (same class as #5493's FLTR spam, ~20x smaller). The stored `BptdRecord.parts` are wrong data waiting for a future FO4 VATS or dismemberment consumer.

## Related
ESM-2026-10-27-01 (FNV/FO3 byte order inside the same 84-byte struct; fold this fix into it), #6350 (open VATS tracker), #5493.

## Suggested Fix
Decode BPND fields only for the 84-byte dialect (FO3/FNV/Skyrim), gating on game or payload length. On FO4 keep the BPTN/BPNN/BPNT/BPNI name run and leave the payload undecoded until a sourced FO4 layout (xEdit `wbDefinitionsFO4.pas`) is in hand - no guessing. Add a test that an FO4-sized (101 B) BPND neither populates the fields nor calls `remap_fid`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

