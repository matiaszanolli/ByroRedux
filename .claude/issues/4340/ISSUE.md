# #4340 — TD1-004: `apply_effect` grew 519 → 680 LOC in four days; the per-family helper extraction #3854 prescribed never landed

**Labels**: low, scripting, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4340

- **Severity**: LOW · **Dimension**: 1
- **Location**: `crates/scripting/src/fragment/effects.rs:626-1305` · **Status**: Regression of #3854 (only the file split landed, `f5127c1cc`) · **Age**: `11bed5111`, `d0dac91b1`, `f61ea0447`, `5162829a3`, `37ed10277` (09-12..09-14) · **Effort**: medium · **Kind**: tech-debt
- **Finding**: A 29-arm `match effect`; cognitive complexity 71/25, nesting 6. New MQ101 arms `SetLocked` (72 lines) and `SetLockLevel` (39) arrived after the split. It carries the nested-lock contract doc (#3493/#3949).
- **Suggested Fix**: Make each arm a one-line delegate to `apply_<family>_effect`: globals, inventory, placement/enable, lock ledger, scene, player control & chargen, vehicle-cinematic, AI/combat. Helpers must stay in `crates/scripting/src/fragment/effects.rs` or in flat sibling files added to `SOURCES`, because `every_production_file_is_in_the_sources_concat` does not recurse. Keep every acquired lock named in `apply_effect`'s doc block (#3949 scan).

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (and every lock `apply_effect` acquires stays named in its doc block, #3949)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
