# SAVE-D1-2026-08-20-02: the completeness guard's SCAN_ROOTS covers one subdirectory of crates/core/src — 63 impl Component/Resource sites sit outside both guards, and LodCoverageStats landed unclassified in this delta

**Issue**: #3166 — https://github.com/matiaszanolli/ByroRedux/issues/3166
**Finding ID**: `SAVE-D1-2026-08-20-02`
**Severity**: MEDIUM
**Dimension**: 1 — Snapshot Completeness & Determinism
**Audit**: `/audit-save` — 2026-08-20 comprehensive suite, HEAD `bb0b92f2`
**Labels**: medium, tech-debt, bug

---

**Audit**: `/audit-save` — `docs/audits/AUDIT_SAVE_2026-08-20.md` (HEAD `bb0b92f2`)
**Finding ID**: `SAVE-D1-2026-08-20-02`
**Severity**: MEDIUM
**Dimension**: 1 — Snapshot Completeness & Determinism
**Data-Loss Class**: silent-drop (latent)

## Location

- `byroredux/src/save_io/registry_completeness_tests.rs:299-304` — `SCAN_ROOTS`

Unscanned examples:
- `crates/core/src/ecs/resources/mod.rs:938` — `LodCoverageStats` (**new this delta**)
- `crates/core/src/character/affliction.rs:143` — `AfflictionStatus`
- `crates/core/src/character/regen.rs` — `PoolRegenAccumulator`
- `crates/core/src/character/components.rs` — `FactionReputation`
- `crates/core/src/animation/controller.rs` — `AnimationController`
- `crates/audio/src/lib.rs` — `AudioEmitter`, `AudioListener`, `OneShotSound`

## Description

`SCAN_ROOTS` is:

```rust
const SCAN_ROOTS: &[&str] = &[
    "../crates/core/src/ecs/components",
    "../crates/scripting/src",
    "../crates/physics/src",
    "../byroredux/src",
];
```

The first entry is a **subdirectory** of `crates/core/src`, so four sibling directories that also
define ECS state are invisible: `crates/core/src/ecs/resources/`, `crates/core/src/character/`,
`crates/core/src/animation/`, `crates/core/src/string/` — plus `crates/audio/` and
`crates/plugin/` entirely. **Four directories missed inside `crates/core/src` alone.**

## Method

The guard was **re-implemented in Python against the live tree** (`impl_target_type` + the XOR
assertion). Within `SCAN_ROOTS` it finds **213 distinct types with 0 unclassified and 0
double-classified** — the guard is genuinely GREEN and this is not a guard failure. The finding is
what it cannot see.

The same walk over the whole workspace finds **63 further `impl Component` / `impl Resource`
sites outside those roots**, of which only four are registered (`Transform`, `AnimationPlayer`,
`AnimationStack`, `ItemInstancePool`) and only four more are covered by the sibling
`REDERIVED_NOT_SAVED` list (`CharacterLevel`, `Background`, `Perks`, `FactionRanks`). The
remaining ~55 are classified by neither guard — not *mis*classified, simply **never considered**.

## Evidence: this delta produced a live instance

`8e7582ed` / EX-15 added `impl Resource for LodCoverageStats` at
`crates/core/src/ecs/resources/mod.rs:938`. **It is neither registered nor allowlisted, and the
guard stayed green because the file is not scanned.** It happens to be pure telemetry and
correctly not save-worthy — but that outcome was *luck, not enforcement*, and it is precisely the
scenario the guard exists to make impossible.

Spot-checked for live exposure across the unscanned set:
- `affliction_tick_system` has no scheduler registration (forward-latent).
- `FactionReputation` has zero production insert or mutate sites.
- `pool_regen_tick_system` **is** scheduled (`byroredux/src/boot.rs:936`), but its
  `PoolRegenAccumulator` holds only fractional carry.

**No live silent-drop exists through this gap today** — which is exactly why it should be closed
now rather than after one does.

## Impact

The guard is presented in its own doc comment, and consumed by this audit's Dimension 1, as *the*
completeness ledger. Its actual coverage is **~78% of the workspace's ECS state definitions**, and
the shortfall is concentrated in the two worst places for it to be:

- `crates/core/src/character/` — CHARAL, actively under construction, and the home of exactly the
  accumulating-progress types #2947 already had to build a bespoke runtime tripwire for.
- `crates/core/src/ecs/resources/` — accreting new resources, one this cycle.

## Related

- `SAVE-D1-18` — the prior, closed instance of this class (which added `../byroredux/src`). **Not
  a regression**: that specific root is still present and still working; these are different roots.
- **#2947** / `validate_progression_state` — a bespoke runtime tripwire built for one unscanned
  type, which a wider scan would have generalised.
- `SAVE-D2-2026-08-20-02` — the sibling guard's own discovery holes.

## Suggested Fix

Widen `SCAN_ROOTS` to:

```rust
const SCAN_ROOTS: &[&str] = &[
    "../crates/core/src",
    "../crates/scripting/src",
    "../crates/physics/src",
    "../crates/audio/src",
    "../byroredux/src",
];
```

and absorb the resulting ~55 new types into `NOT_SAVED_BY_DESIGN` with real reasons in one pass —
the same exercise that surfaced seven genuine gaps (#2378–#2382) when the allowlist was first
built. `collect_rs_files` already panics on an unreadable root, so a future directory move stays
loud.

## Completeness Checks
- [ ] **SIBLING**: `crates/plugin/` is considered too (currently unscanned entirely) — either added or explicitly documented as out of scope
- [ ] **SIBLING**: each of the ~55 newly-visible types gets a *real* reason, not a blanket one — the reason text is the only thing that ages (see `SAVE-D1-2026-08-20-01`)
- [ ] **TESTS**: the widened guard is green and `LodCoverageStats` specifically is classified rather than invisible
