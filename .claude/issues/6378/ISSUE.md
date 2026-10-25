# TD3-2026-10-25-07: `game-loop.md` live-schedule table omits the 10-22..25 systems

Labels: low, ecs, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6378
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW (same family as closed #5110/#5318; no guard exists)
- **Dimension**: 3
- **Effort**: small
- **Location**: `docs/engine/game-loop.md:136-176`. Not listed:
  - `vats_mode_system` (Early, exclusive + declared, `boot/schedule/early.rs:109-111`, `818eba7f4`)
  - `flee_package_system`, `find_package_system` (PostUpdate, `post_update.rs:124-127`)
  - `use_weapon_package_system` (Update, `update.rs:722`)
  - the crime systems (`jail_bed_system` `update.rs:309`, `evidence_deposit_system` `:322`, `make_guard_rush_system` `:395`)
- The PostUpdate AI row still lists only "sandbox/wander/travel/follow/escort/guard/patrol + eat_sleep + graph locomotion + console walk + walk animation".
- **Fix**: Add rows or extend the AI row. Optional guard: a test that parses `scheduler.add*` stage/system names in `boot/schedule/*.rs` and asserts each appears in `game-loop.md` (the `PARALLEL_SYSTEMS` machinery at `boot/schedule/mod.rs:760` already enumerates parallel systems).

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD3-2026-10-25-07`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
