# #7103 — ESM-D2-07: FO3/FNV `RACE` `DATA` reads 8 skill pairs (xEdit: 7 + `wbUnused(2)`); phantom `(0, 0)` bonus on every race

Labels: low,esm-plugin,character,bug,game:fnv,game:fo3
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D2-07)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D2-07**

- **Severity**: LOW (no FO3/FNV consumer of `skill_bonuses`; the Oblivion half of the same bug was fixed in the window)
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `RACE` / `DATA` (FO3, FNV; 36 bytes)
- **Location**: `crates/plugin/src/esm/records/actor/race.rs:525-552` (`let bonus_count = if game == GameKind::Oblivion { 7 } else { 8 };` ... `if game == GameKind::Oblivion { r.skip_or_eof(2); }`), stale comment at `:515-516` ("FO3/FNV: 8 pairs"); stale test name `race_oblivion_data_reads_8_skill_pairs_plus_heights` at `records/actor/tests.rs:1247`
- **Status**: NEW (Oblivion's 7+2 fix landed in `278886579`; the FO3/FNV arm was left at 8)
- **Description**: `wbDefinitionsFO3.pas` and `wbDefinitionsFNV.pas` RACE `DATA` are `wbArrayS('Skill Boosts', ..., 7)` + `wbUnused(2)` + 4 floats + u32 flags, byte-identical in shape to TES4. The 8th "pair" is the two unused bytes. The decoder pushes it whenever `skill != 0xFF`.
- **Evidence**: FalloutNV.esm: all 22 36-byte `RACE.DATA`; Fallout3.esm: 22/22; Oblivion.esm: 15/15. In all three, bytes 14..16 are `00 00`, while unused skill slots are `ff 00` (first sample FNV: `ff00 ff00 ff00 ff00 ff00 ff00 ff00 0000 0000803f ...`). So 8th pair = `(skill 0, bonus 0)`, not the `0xFF` sentinel, and is pushed into `skill_bonuses` on every FO3/FNV race. The existing Oblivion test fixture still has 8 pairs (`:1247-1275`) and passes only because the 8th is `(0xFF, 0)`, which now reads as the 2 skipped bytes.
- **Impact**: a spurious `(0, 0)` entry per race; harmless arithmetically (+0) and unconsumed on FO3/FNV (`skill_bonuses` readers are `oblivion_population.rs:171` and the Skyrim branch of `actor_value_derive.rs:506`), but it is a wrong entry in a public Vec.
- **Suggested fix**: `bonus_count = 7` and `skip_or_eof(2)` for the whole TES4-era arm; update the `:515` comment and rename the test (add an FNV fixture with a `0000` tail asserting no phantom entry).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
