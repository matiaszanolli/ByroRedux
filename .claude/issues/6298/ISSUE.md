# #6298: ESM-2026-10-21-04: CHAL is decoded with the FNV schema on every game; Starfield's 8 challenges become zeroed records

**Labels**: low,bug,esm-plugin,game:starfield

**Source**: docs/audits/AUDIT_ESM_2026-10-21.md · **Severity**: LOW · **Dimension**: Record Schema Dispatch · **Location**: `crates/plugin/src/esm/records/dispatch_misc_stub.rs` (`CHAL` arm); `crates/plugin/src/esm/records/misc/challenge.rs`

- **Record / Sub-record**: `CHAL` / `DATA`, `SNAM`
- **Location**: `crates/plugin/src/esm/records/dispatch_misc_stub.rs:224-226`; `crates/plugin/src/esm/records/misc/challenge.rs:73`
- **Status**: NEW
- **Description**: `CHAL` is dispatched to `parse_chal` without a game gate. Starfield's `CHAL` (`wbDefinitionsSF1.pas:10747-10766`) is `EDID` plus `wbBaseFormComponents`. Starfield.esm ships 8 of them, carrying `DNAM`/`CITC`/`ATAN`/`ATAV`/`CTDA` and no `DATA`/`SNAM`. They enter `EsmIndex::challenges` as default `ChalRecord`s. The sibling `MUSC` decoder (`misc/music.rs`) does gate on `Fallout3NV`.
- **Impact**: inert today. `ChallengeDefs` consumers filter on `threshold > 0` (`crates/scripting/src/challenges.rs:278`). The `esm_dim8_bench` count of "8 challenges" on Starfield is misleading.
- **Suggested Fix**: gate the `parse_chal` arm on `GameKind::Fallout3NV`, as `parse_musc` does, and stub `CHAL` elsewhere.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
