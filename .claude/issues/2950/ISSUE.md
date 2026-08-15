# CHAR-D4-03: pool_regen_tick_system has a second undocumented prerequisite; boot.rs's comment will mislead whoever wires it

- **Issue**: [#2950](https://github.com/matiaszanolli/ByroRedux/issues/2950)
- **Finding ID**: `CHAR-D4-03`
- **Labels**: `medium,legacy-compat,bug`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2950 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: oblivion (the only game with a `PoolRegenConfig` builder)
- **Location**: `crates/core/src/character/regen.rs:111-131` (`pool_regen_tick_system` doc + preamble), `byroredux/src/boot.rs:855-879` (registration comment)
- **Status**: NEW
- **Source**: n/a — a wiring/precondition defect, not a numeric one.
- **Description**: The tick needs **two** resources: `PoolRegenConfig` *and*
  `PoolRegenAccumulator`. Its docstring names only the first ("A no-op if
  `PoolRegenConfig` hasn't been inserted … or if less than one 60 Hz tick has
  elapsed"), and `boot.rs`'s comment states the system is "registered now so the
  tick is already live the moment that wiring lands", where "that wiring" is
  described purely as the `PoolRegenConfig` insertion via `build_character_ruleset`.
  A workspace-wide grep finds **no insertion site for either resource** —
  `PoolRegenAccumulator` appears only in `regen.rs` (definition + tests) and in
  `boot.rs`'s `Access` declaration. `World::try_resource_mut` does not
  default-insert, so a wiring commit that inserts only the config leaves the system
  silently dead forever.
- **Evidence**:
  ```rust
  let Some(config) = world.try_resource::<PoolRegenConfig>() else { return; };
  let Some(mut accumulator) = world.try_resource_mut::<PoolRegenAccumulator>() else { return; };  // undocumented second gate
  ```
  Both of `regen.rs`'s live-path tests construct the accumulator by hand
  (`world.insert_resource(PoolRegenAccumulator::default())`), so the test suite
  cannot catch the omission either; `tick_system_is_a_noop_without_config` asserts
  the no-op is *tolerated*, never that it stops being one.
- **Impact**: When Oblivion's `CharacterRuleset` wiring lands, Fatigue and Magicka
  regeneration are dead with no error, no warning, and no failing test — a
  gameplay system that looks wired (registered in `boot.rs`, declared in
  `sys.accesses`) but never runs. Diagnosing it means noticing that a system
  registered specifically to be "already live" is returning at its second line.
- **Related**: #2153 (the same system's lock-stack declaration, OPEN, routed to
  `/audit-concurrency`); *CHAR-D1-01* (the same system evaluates a `PlayerOnly`
  derived formula for every actor).
- **Suggested Fix**: Insert `PoolRegenAccumulator::default()` unconditionally at
  boot (it is `Default` + `Copy` and harmless when no config exists), or fold the
  accumulator into `PoolRegenConfig` so a single insertion arms the whole tick.
  Either way, correct the docstring and the `boot.rs` comment to name both
  preconditions.

## Completeness Checks
- [ ] **SIBLING**: The same pattern is checked in the other per-game ruleset builders (`fallout.rs` / `tes.rs` / `skyrim.rs`), not just the one cited
- [ ] **SOURCE**: Any changed constant cites the capture document line it comes from (`docs/engine/charal-*-ruleset.md`) — never a guessed value
- [ ] **CHARAL-BOUNDARY**: The per-game seam stays *data in the tables*; no consumer gains a branch on game identity
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
