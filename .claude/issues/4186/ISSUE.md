# CONC-D4-02: WindField's only same-stage reader runs in Early's parallel phase — the accepted one-frame lag is neither stated nor pinned

Labels: low,concurrency,doc-rot,documentation

**Description**: #3111 kept `weather_system` in `Stage::Early` but moved it to the exclusive phase (which runs after the stage's parallel batch). `player_controller_system` — the reader the fix targeted — is in that parallel batch, so it reads the *previous* frame's `WindField` value. Correct and deliberate, but the registration comment says only "the controller sees one stable snapshot," never which frame's. The directly analogous accepted cross-stage lag (#3653, `ParticleEmitter::rate`) is both spelled out in a comment and pinned by a dedicated test; this one is neither.

**Evidence**:
`Scheduler::run` runs `data.parallel` then `data.exclusive` per stage; `player_controller_system` is `add_to_with_access` (parallel), `weather_system` is `add_exclusive_with_access`, both `Stage::Early`. `analyze_pair` never compares a parallel entry against an exclusive one, so no counter can see this ordering.

**Impact**: Practically nil in magnitude (wave-scroll offset / wind-wave scale change over seconds-to-minutes timescales). The structural exposure: any new Early-parallel WindField consumer silently inherits the same lag, and a maintainer reading the comment would reasonably believe the controller sees this frame's wind.

**Related**: #3111, #3653 (the documented-and-pinned precedent), #3123.

**Suggested Fix**: One sentence in the `early.rs` registration comment stating the Early parallel batch reads the previous frame's WindField and why that's acceptable; optionally extend the existing pin to assert the phase relationship on the built schedule.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*
