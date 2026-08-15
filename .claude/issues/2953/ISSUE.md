# CHAR-D4-06: MAX_REGEN_SUBSTEPS claims to mirror crates/physics::MAX_SUBSTEPS (8 vs 5)

- **Issue**: [#2953](https://github.com/matiaszanolli/ByroRedux/issues/2953)
- **Finding ID**: `CHAR-D4-06`
- **Labels**: `low,legacy-compat,documentation`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2953 --json state`.

---

- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: all
- **Location**: `crates/core/src/character/regen.rs:45-48` (`MAX_REGEN_SUBSTEPS` doc), `:71-73` (`PoolRegenAccumulator` doc)
- **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md:401-405` — the design intent is
  "one global fixed-step clock **mirroring `crates/physics::PhysicsWorld`'s own
  accumulator** — the only other fixed-timestep precedent in the engine", capped at
  "`MAX_REGEN_SUBSTEPS` (8)".
- **Description**: `POOL_REGEN_DT`'s "Matches `crates/physics::PHYSICS_DT`" is true
  (both `1.0/60.0`). The sibling claim on the substep cap is not:
  `crates/physics/src/world.rs:15` defines `MAX_SUBSTEPS = 5`, while
  `MAX_REGEN_SUBSTEPS = 8`, under a doc comment reading "Mirrors
  `crates/physics::MAX_SUBSTEPS`."
- **Evidence**: the clamp bodies are otherwise character-for-character identical
  (`accumulator += dt.max(0.0)`; clamp to `N × DT`; floor-divide; subtract), so the
  only divergence is the constant the comment asserts parity on. Behaviourally the
  two clocks drift during a hitch: regen advances up to 133 ms of simulated pool
  time per frame where physics advances at most 83 ms.
- **Impact**: Documentation-level today; the practical effect is a slightly larger
  post-hitch regen burst than the design intent describes, and a maintainer tuning
  one constant "to keep them in sync" would be reasoning from a false premise.
- **Related**: CHAR-D4-03 (same module's precondition doc).
- **Suggested Fix**: Either set `MAX_REGEN_SUBSTEPS = 5` (matching the claim and the
  physics clock) or keep 8 and replace "Mirrors" with the reason it differs.
  `crates/physics` also carries a second wall-clock guard (`SUBSTEP_TIME_BUDGET`)
  that regen has no analogue for — worth saying so if the mirroring language stays.

## Completeness Checks
- [ ] **SIBLING**: The same drift class is swept across the other capture documents / docstrings, not just the row cited
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
