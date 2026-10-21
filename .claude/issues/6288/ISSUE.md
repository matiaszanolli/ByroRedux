# #6288: GAME-D4-2026-10-21-04: Death neither ends an actor's magic effects nor runs their `ScriptEffectFinish`, contrary to the module doc

**Labels**: low,gameplay,combat,scripting,bug,game:fo3,game:fnv,game:oblivion

**Source**: docs/audits/AUDIT_GAMEPLAY_2026-10-21.md · **Severity**: LOW · **Dimension**: Combat & Death Pipeline (Dim 4, `reconcile_dead_actor` must grow with each per-actor runtime component) · **Location**: `crates/scripting/src/magic_effects.rs:16` (doc), `:292-383` (no `Dead` skip); `byroredux/src/combat.rs:1019-1128` (`reconcile_dead_actor` has no dispel)

**Audit status**: NEW (from `d1b298921`)

- **Description**: The module doc says a script effect "runs … `ScriptEffectFinish` once when it ends — the duration elapsed, or `Dispel` / death". Nothing dispels on death. The only `dispel` callers are `RemoveSpell` (`magic.rs:255`) and ObScript `Dispel` (`obscript_effects.rs:617`). `magic_effect_system` keeps running `ScriptEffectUpdate` blocks and per-second value changes on corpses until each duration lapses, and an `indefinite` effect never lapses. The list is saved and parked on corpses (#6085), so it outlives saves and evictions too.
- **Impact**: Effect scripts keep executing with a corpse as their calling reference, and an effect's Finish-time cleanup is deferred to its natural end or never happens.
- **Suggested Fix**: In `reconcile_dead_actor` (live producers only, not the corpse-restore sweep), mark the corpse's effects `ending` the way `dispel(world, actor, None)` does, so Finish runs once on the next pass. Correct the doc if a different rule is intended.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every per-actor runtime component added by M47.3 is torn down (or deliberately kept) by `reconcile_dead_actor`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
