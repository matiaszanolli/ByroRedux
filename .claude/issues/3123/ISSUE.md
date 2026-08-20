# Issue #3123: make_billboard_system reads TotalTime undeclared

- **Finding ID**: `CONC-2026-08-20-03`
- **Severity**: LOW
- **Labels**: `low,sync,bug`
- **Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3123

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3123 --json state`.

---

**Severity**: LOW
**Dimension**: Scheduler Access Declarations (regression guard)
**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-08-20.md` (`CONC-2026-08-20-03`)

**Location**: declaration `byroredux/src/boot.rs:1182-1191`; the undeclared read
`byroredux/src/systems/billboard.rs:49-53`

## Description

This delta gave the SpeedTree gust phase a shared clock so foliage and water cannot drift out of phase —
`billboard.rs:49` now reads `TotalTime` (falling back to the closure-local accumulator for synthetic test
worlds that skip the registration). The `Access` chain gained `WindField`, `SpeedTreeWind` and
`MeshHandle` in the same session but **not** `TotalTime`.

Reported at LOW rather than folded into `CONC-2026-08-20-02` (#3121) because the reasoning for declaring
it is *only* the documentation one, and the codebase has already written that reasoning down. The comment
at `boot.rs:1176-1181` explains that these three PostUpdate exclusives declare access despite the
analyzer not pairing exclusives, precisely because

> the ordering contract above is entirely about who touches `GlobalTransform` when, and a blank
> `sys.accesses` row is exactly the wrong place for that to be invisible

A row that is present but three-quarters complete is a weaker version of the same problem.

## Evidence

```rust
// byroredux/src/systems/billboard.rs:49-53
let wind_time = world
    .try_resource::<TotalTime>()
    .map(|time| time.0)
    .unwrap_or(elapsed);
```

The chain at `boot.rs:1184-1191` is `ActiveCamera`, `WindField`, `Billboard`, `SpeedTreeWind`,
`MeshHandle`, `GlobalTransform` (w). No `TotalTime`.

## Trigger Conditions

None. The system is registered `add_exclusive_with_access`, and exclusive systems run serially after
their stage's parallel batch, so they are never paired by the analyzer and never overlap anything.

## Impact

Documentation / `sys.accesses` completeness only. No live or latent race — the system is exclusive and
`TotalTime` has exactly one writer (the engine's own per-frame tick, outside the scheduler).

## Related

- #2391 (the reason these exclusives declare access at all)
- #3111 (same family, live parallel writer, HIGH)
- #3121 (same family, latent contract defect, MEDIUM)

## Suggested Fix

One line — `.reads_resource::<byroredux_core::ecs::resources::TotalTime>()` on the
`make_billboard_system` chain. Its `submersion_system` sibling twelve lines below already declares
exactly this.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the other two exclusives in the PostUpdate
      `GlobalTransform` ordering chain (#2391), whose declarations exist for the same documentation reason
- [ ] **TESTS**: A regression test pins this specific fix
