# TD3-2026-10-25-05: crime and VATS shipped 10-23..25 but several docs still say "no crime system" / "guards do not read it"; VATS has no matrix/ROADMAP row

Labels: low, gameplay, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6376
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW (HISTORY Session 104 covers crime; VATS landed after the last close)
- **Dimension**: 3
- **Effort**: small
- **Location**:
  - `docs/engine/scripting.md:809` ("the engine has no crime system, so scripts' writes are all of it"; `byroredux/src/crime.rs:107-119` now writes `FactionCrimeGold`)
  - `ROADMAP.md:276` ("guards do not read it"; `99051a4eb` guards rush a wanted player, `73924db73` guard dialogue reads the bounty, `1a67c9f30` witnesses call guards)
  - `docs/feature-matrix.md:238` ("Not yet: ... crime hostility")
  - `docs/feature-matrix.md:206` ("crime/detection ... stay declined"; `e161c45f3` made `PayFine`, `PayFineThief`, `GoToJail` run, though `PlayerPayCrimeGold` stays declined)
  - VATS (`818eba7f4`, `7eb4de261`, `3dc16208e`, `077ff45d6`, `ff97cdd93`, `aebcc99a1`, 2026-10-25): `vats_mode_system`, `vats.*` console commands and `docs/smoke-tests/vats-targeting.sh` exist. `debug-cli.md:532-536` and `docs/smoke-tests/README.md:61` are updated. `docs/feature-matrix.md` and `ROADMAP.md` have no VATS or Action Points runtime row, and HISTORY has no Session 105.
- **Fix**: Add a matrix row for VATS and AP runtime ("plumbing; FNV first gate, #6350 open"). Correct the three crime sentences. The VATS part can wait for `/session-close`, but the three crime sentences contradict code that landed in Session 104.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD3-2026-10-25-05`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
