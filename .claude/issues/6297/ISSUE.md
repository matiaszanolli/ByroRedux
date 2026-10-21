# #6297: ESM-2026-10-21-03: FO76 RACE ANAM skeleton run is authored but still excluded

**Labels**: low,bug,esm-plugin,game:fo76

**Source**: docs/audits/AUDIT_ESM_2026-10-21.md · **Severity**: LOW · **Dimension**: Record Schema Dispatch · **Location**: `crates/plugin/src/esm/records/actor/race.rs` (`ANAM` arm)

- **Record / Sub-record**: `RACE` / `ANAM` (skeleton section)
- **Location**: `crates/plugin/src/esm/records/actor/race.rs:612-624`
- **Status**: NEW. The skill and #6122 recorded FO76 as unmeasured; it is now measured.
- **Description**: The `ANAM` arm is gated to `Skyrim | Fallout4 | Starfield` with the comment "FO76 is left out: its RACE has not been measured". On SeventySix.esm, 157 RACE records carry 307 `ANAM` path strings (44–56 bytes, for example `actors/deathclaw…`, `Actors\Molerat\C…`), plus the `NAM1`/`MNAM`/`FNAM` markers the arm's section logic keys on.
- **Impact**: every FO76 non-human race falls back to the default human rig, which is the #6070/#6122 symptom. This is low priority while FO76 is not a compat target.
- **Suggested Fix**: add `Fallout76` to the gate, cite the FO76 xEdit RACE skeleton struct, and add a fixture shaped like a SeventySix record.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
