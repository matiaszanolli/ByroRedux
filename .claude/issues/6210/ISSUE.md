# #6210: ESM-2026-10-21-01: Skyrim+/FO4/FO76/Starfield PSDT duration is minutes, decoded as hours, so every timed package is active around the clock

**Labels**: high,bug,esm-plugin,gameplay,ai,game:skyrim,game:fo4,game:fo76,game:starfield

**Source**: docs/audits/AUDIT_ESM_2026-10-21.md · **Severity**: HIGH · **Dimension**: Sub-Record Byte Accounting · **Location**: `crates/plugin/src/esm/records/misc/pack.rs` (PSDT arm of `parse_pack`; `PackSchedule::active_at`)

- **Record / Sub-record**: `PACK` / `PSDT`
- **Location**: `crates/plugin/src/esm/records/misc/pack.rs:970-1004` (PSDT arm); `pack.rs:479-495` (`PackSchedule::active_at`); pinned wrong by `parse_pack_reads_skyrim_plus_psdt_schedule_from_offset_8` (`pack.rs:~1580`)
- **Status**: NEW. Related to #2012 (closed): that fix moved the Skyrim+ duration to offset 8 but kept the hours unit.
- **Description**: xEdit defines the Skyrim+ field as `wbInteger('Duration (Hours)', itU32, wbDiv(60, 4))` (`wbDefinitionsTES5.pas:8548`, `FO4.pas:10569`, `SF1.pas:15600`). The stored integer is minutes; xEdit shows it divided by 60. The FO3/FNV/Oblivion field is plain `itU32` hours (`FNV.pas:7134`, `TES4.pas:2926`). `parse_pack` reads offset 8 on `uses_prebaked_facegen()` games and stores the raw value in `PackSchedule::duration_hours`. The Skyrim+ `Minute` byte at offset 4 is also dropped, so a start like 08:30 becomes 08:00.
- **Evidence**:
  - Skyrim.esm census: `WhiterunBraithSleep21x12` stores `hour=21, duration=720`; `SolitudeBackupFishmongerWork6x14` stores `6, 840`; `DragonBridgeClintonSleep22x10` stores `22, 600`.
  - Duration histogram over timed PSDTs: 720 ×262, 60 ×225, 120 ×218, 240 ×193, 360 ×141, 480 ×135. Every value is a multiple of 60 except 36 records.
  - In `active_at`, `end = start + duration_hours` (for example 21 + 720). With `end > 24`, the wrap arm is `h >= start || h < end - 24`, and `end - 24 ≥ 36` makes it true for every hour.
  - Skyrim.esm: 1,536 of 1,545 timed PSDTs have a raw duration ≥ 24. Fallout4.esm: 251 of 252.
- **Impact**: `select_active_package` (`byroredux/src/npc_spawn/ai_package.rs:772-800`) is not game-gated. It returns the first package in the stack whose conditions pass. On Skyrim, FO4 and Starfield every scheduled package (sleep, work, eat) is "in schedule" all the time, so an NPC is pinned to the first scheduled package in its stack around the clock. Time-of-day package switching, the core of ambient AI, does not happen on any Creation-era title. FO3/FNV/Oblivion are unaffected.
- **Related**: #2012, #6079 (Day-of-Week on the same sub-record), #3352 (zero-length windows).
- **Suggested Fix**: on the 12-byte arm, divide the raw value by 60 and keep the start minute. Store the window in minutes, or as `f32` hours, so 90-minute windows survive. Correct the #2012 test to `duration=720 → 12 h`, and add a Skyrim.esm real-data pin such as `WhiterunBraithSleep21x12` → 21:00 for 12 h.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
