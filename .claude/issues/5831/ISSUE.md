# TD3-2026-10-15-07: The m42-eat-sleep smoke is described as seating the settler at the bed in CLAUDE.md, the smoke README and the script's own section header; it now asserts selection + idle

Labels: low,gameplay,ai,game:fnv,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-07`).

- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `CLAUDE.md:104-109`; `docs/smoke-tests/README.md:57`; `docs/smoke-tests/m42-eat-sleep.sh:97-98`
- **Status**: NEW
- **Age**: `5f205f0ca` (2026-10-13)
- **Effort**: trivial
- **Description**: the CLAUDE.md sentence "the walk-then-seat runtime must seat HIM at the bed (the seat leg is scoped to the found entity)" and the README row "window selection, walk-then-seat" predate #5391/#5500; the script now fails if the settler IS `Seated` (`:123-125`) and says the seat runtime is unit-gated (`:14-15`, `:118`). The script's own banner at `:97-98` ("... then the walk-then-seat runtime seats her at the bed") contradicts the comment block 12 lines below it.
- **Evidence**: `sed -n 97,127p docs/smoke-tests/m42-eat-sleep.sh`; `git show 5f205f0ca`.
- **Impact**: someone reading CLAUDE.md believes a live seat leg gates Eat/Sleep; it does not.
- **Related**: #5391 (closed), #5500, TD3-2026-10-15-12.
- **Suggested Fix**: CLAUDE.md: "... must hand him `SleepBehavior`, and — because his In-Cell package targets another cell (#5391) — he must idle, not walk or seat; the seat runtime is unit-gated". Same edit in the README row and the `:97` banner.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
