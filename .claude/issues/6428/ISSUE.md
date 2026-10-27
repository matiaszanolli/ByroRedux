# CHAR-2026-10-26-L-07: `RadiationRads` has no production writer, and the gauge and the affliction model disagree about which layer holds it

- **Severity**: LOW
- **Dimension**: Population Boundary / Progression & Pools
- **Game**: fo3, fnv
- **Location**: `byroredux/src/inventory.rs:384-390`, `:425-444` (`values.current(avif).clamp(0.0, scale)`) and test `vitals_snapshot_appends_the_rad_gauge_after_the_pools` (writes `mod_temporary`); contrast `crates/core/src/character/affliction.rs:84-93` (`pool_value` = the `damage` layer) and `docs/engine/charal.md:220-224`
- **Status**: NEW
- **Source**: `charal.md:221-224` ("the existing `ActorValue::damage` layer … read via `AfflictionTable::pool_value`, not `ActorValues::current`"). The gauge row cites GECK *Actor Value Codes* 54 / *GetRadiationLevel* (composed `GetAV radiationrads`).
- **Description**:
  1. Nothing in production creates or changes the value. `grep RadiationRads` finds only the ObScript name table. `consumables::restoration` models Health/AP/Magicka/Stamina only. No exposure system exists. So the gauge reads 0 outside ObScript, console or SDK edits, and the test that proves it hand-inserts the modifier.
  2. If rads accumulate as `damage` (the affliction design), `current()` is `−rads` and the gauge, clamped at 0, stays empty. If they accumulate as a modifier, `pool_value` reads 0 and no band ever applies.
  3. No test drives both readers from one write, and `affliction_tick_system` is unregistered.
- **Impact**: Latent. The first radiation writer will make one reader wrong with no test noticing.
- **Suggested Fix**: Decide the layer in `charal.md` (the engine's `GetAV radiationrads` is composed, so the affliction `pool_value` for rads probably should read `current()`). Pin it with a test that drives both readers from one write, and record in the slice doc that rad accumulation and RADS-stage penalties are unbuilt.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-07`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

