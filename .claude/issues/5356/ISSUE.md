# #5356: PHYS-D2-2026-10-05-04: `clamp_explosive_velocities` still documents pre-#5246 semantics, and its third-offence arm counts a "detach" on every later burst, including for bodies with no articulation

**Labels**: low,physics,doc-rot,test-gap,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5356

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-05.md` — `PHYS-D2-2026-10-05-04` (HEAD `a2c24b16e`)

- **Severity**: LOW
- **Dimension**: Step & Sync / Queries & Diagnostics
- **Location**: `crates/physics/src/world.rs:949-955` (fn doc), `:1032-1048` (`*offences >= 3` arm)
- **Status**: NEW
- **Description**: the function doc still says "A body clamped on consecutive substeps is parked; a clean substep
  returns it to watch-list absence". That describes the set-cleared-on-clean-substeps design that #5246 removed in
  favour of a lifetime count, and the field doc and constant doc now say the opposite. Separately:
  - The `>= 3` arm calls `remove_multibody_articulations`, adds one to `explosive_detaches_total` and logs "detached
    … articulation" on the third burst *and every burst after it*.
  - It does this for any dynamic body, including clutter or a link whose articulation was already removed, where
    the removal is a no-op.
  - The guard `the_third_burst_detaches_the_articulation` drives a plain dynamic body with no joint and asserts
    `explosive_detaches_total() == 1`. It therefore pins a detach that detached nothing.
- **Impact**: a gate or operator reading `phys.stats` "explosive detaches" counts bursts past the third, not rigs
  detached. A reader trusting the fn doc misreads the escalation ladder.
- **Suggested Fix**: rewrite the doc to state the lifetime ladder. Count a detach only when
  `multibody_joints.rigid_body_link(handle).is_some()` before the removal, or only on the transition to offence 3.
  Make the test assert on a real articulation.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
