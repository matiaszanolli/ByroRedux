# TD7-2026-10-25-05: sub-record length gates added since 10-21 are unnamed, and PACK repeats `12` six times

Labels: low, esm-plugin, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6393
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 7 · **Effort**: trivial · **Age**: 2026-10-24 (PKW3 10f8cbb91, CRVA 4cfcee4b9, CTDA 73924db73); ACBS #6120
- **Location**: `plugin/src/esm/records/actor/faction.rs:329,336` `CRVA` `>= 14` / `>= 20`; `misc/pack.rs:1266` `PKW3` `>= 20` (its own doc says "24 bytes"); `pack.rs:1232,1259` `PLD2`/`PTD2` `>= 12` plus the same `12` re-checked inside `parse_location`/`parse_target`/`parse_data_target` (`pack.rs:832,855,882`) and the PLDT/PTDT arms; `actor/npc.rs:866-867` ACBS `>= 20` (FO4/76) / `>= 16` (Starfield); `condition.rs:398` `data.len() == 24` (Oblivion CTDA) beside the already-named `CTDA_RUN_ON_MIN_LEN = 28` (`:166`).
- **What**: The tree already names these (`CHAL_DATA_LEN`, `GRAS_DATA_LEN`, `CTDA_RUN_ON_MIN_LEN`, `weather.rs SKYRIM_*_SIZE`, `PackUseWeapon` flags). The new gates are the exceptions, and in PACK the double gate (arm guard + helper guard) means two edits per layout change. The PKW3 `>= 20` vs the documented 24-byte body is deliberate tolerance but undocumented in code.
- **Fix**: `const CRVA_MIN_LEN: usize = 14; CRVA_TAIL_LEN = 20; PACK_LOCATION_LEN = 12 (shared by PLDT/PLD2/PTDT/PTD2/PDTO); PKW3_MIN_LEN = 20; ACBS_FO4_LEN = 20; ACBS_SF_LEN = 16; CTDA_OBLIVION_LEN = 24`, and drop the arm-level duplicates where the helper already checks.


#### Dimension 8 — Dead Code (7)

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD7-2026-10-25-05`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
