# #7119 — ESM-D8-02: `NPC_.PRPS` is 12 B (AV, value, Curve Table) on Starfield/FO76 from form version 152; decoded as 8 B pairs (13,683 NPCs), 23,981 warn lines

Labels: low,esm-plugin,character,bug,game:starfield,game:fo76
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D8-02)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D8-02**

- **Severity**: LOW
- **Dimension**: Real-Data Validation
- **Record / Sub-record**: `NPC_` `PRPS`; Starfield (7,131 records) and FO76 (6,552 records)
- **Location**: `crates/plugin/src/esm/records/actor/npc.rs:1154-1171` (`parse_npc_actor_values`, `sub.data.as_chunks::<8>()`)
- **Status**: NEW
- **Description**: xEdit defines the property struct with `wbFromVersion(152, wbFormIDCk('Curve Table', [CURV, NULL]))` appended to the AV + value pair (`wbDefinitionsSF1.pas:5677-5686` and `wbDefinitionsFO76.pas:7002-7009`). The census agrees: every Starfield and FO76 payload is a multiple of 12. FO4 (`wbDefinitionsFO4.pas:5266`) has no curve and stays 8 bytes, which is why the decoder is right for FO4. Other `PRPS` readers (`RACE` 31 on Starfield, 45 on FO4, and the `ACTI`-family) were not checked for the same stride.
- **Evidence**: Starfield `PRPS`: 7,131 of 7,131 payloads are multiples of 12, and 3,132 of them are not multiples of 8, so every chunk after the first is misaligned for those; a 12-byte payload is read as one good pair plus a 4-byte tail, and larger ones are misread from entry two on. FO76: 6,552 of 6,552 are multiples of 12, 1,498 not multiples of 8. FO4: 3,015 of 3,015 are multiples of 8 and 2,317 of them are not multiples of 12, so FO4 stays correct. With the engine's remap this site produces 23,981 of the 62,576 Starfield warns.
- **Impact**: `NpcRecord::actor_value_props` holds wrong (AV id, value) pairs for 13,683 Starfield and FO76 NPCs. `derive_npc_actor_values` consumes it only on the FO4 arm (`actor_value_derive.rs:564`; FO76/Starfield "no longer inherit FO4's Stored", line 1777), so nothing visible changes today. It would the moment a Starfield/FO76 ruleset reads it. LOW as a latent decode error with no consumer.
- **Suggested fix**: choose the stride from `GameKind` (12 for `Fallout76 | Starfield`) or from the record form version (`>= 152`), keeping the Curve Table FormID.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
