# ESM-D6-01: BPTD `BPNN` / `BPNT` / `BPNI` are plain strings but are read through the lstring helper

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6796
Labels: low, esm-plugin, bug, game:fo4, game:fo76
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (latent; becomes MEDIUM when VATS-FO4-007 / an FO4 targeting scan consumes `vats_target_node`)
- **Dimension**: 6 (Localized Strings; the inverse hazard: a non-lstring field decoded as lstring)
- **Record/Sub-record**: BPTD `BPNN` "Part Node", `BPNT` "VATS Target", `BPNI` "IK Data - Start Node"
- **Location**: crates/plugin/src/esm/records/misc/equipment.rs:280-282
- **Status**: NEW (BPTD issues #6448/#6465 are about BPND, not these)
- **Description**: `read_lstring_or_zstring` treats any exactly-4-byte payload of a localized plugin as a `u32` string id. `BPTN` is `wbLString` (FO4.pas:11564 area, FO76.pas:12623 area, tes5.pas:9257 area), but `BPNN`, `BPNT` (and Skyrim's `BPNI`) are `wbString(..., 0, cpNormal, True)`, not lstrings. A node name of exactly 3 characters plus NUL (`COM`, `gun`, `Eye`, `Arm`) is 4 bytes and so becomes a table lookup.
- **Evidence**: the probe finds 17 `node` and 14 `vats_target_node` placeholders in Fallout4.esm, 2 in DLCCoast, 32 + 26 in SeventySix.esm, e.g. `BptdPart { name: "Barrel", node: "<lstring 0x006E7567>", vats_target_node: "<lstring 0x006E7567>" }`. `0x006E7567` is the little-endian bytes of `"gun\0"`, `0x004D4F43` is `"COM\0"`, `0x00657945` is `"Eye\0"`, `0x006D7241` is `"Arm\0"`. Skyrim.esm shows 0 (its node names are long), so this only bites FO4/FO76 (and mods).
- **Impact**: body-part node / VATS target bone names are replaced by `<lstring 0x…>` for the shortest node names on every FO4/FO76 load. `byroredux/src/systems/vats.rs:528` does `bone_position(world, entity, &part.vats_target_node).unwrap_or(center)`, so the FNV/FO3 scan is unaffected today, but any FO4/FO76 targeting reads the placeholder and silently falls back to the actor centre.
- **Suggested fix**: use `read_zstring` for `BPNN`/`BPNT`/`BPNI` (keep `BPTN` on the lstring path). Add a localized-plugin unit test with a 3-char node name.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
