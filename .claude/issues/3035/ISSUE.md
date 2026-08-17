# ECS-2026-08-16-07: audit-ecs SKILL.md states 13 add_to_with_access registrations; live count is 10

**Issue**: #3035
**Severity**: LOW
**Dimension**: 5b — Scheduler Access Declarations
**Labels**: `low,ecs,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_ECS_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ECS_2026-08-16.md` (Dimension 5b — Scheduler Access Declarations).

**Location**: `.claude/commands/audit-ecs/SKILL.md`:150 (Dimension 5b, "M27 Phase 1+2"), against `byroredux/src/boot.rs`

## Description

The skill pins the number of declared parallel-stage registrations at *"**13** such calls as of 2026-08-16"*. The live count is **10**.

## Evidence

```
$ grep -c "scheduler.add_to_with_access(" byroredux/src/boot.rs
10
```

Live call sites (re-verified 2026-08-17): `boot.rs`:675, 703, 717, 925, 1005, 1201, 1245, 1291, 1316, 1335.

By stage: `Stage::Early` × 3 (`player_controller_system`, `weather_system`, `timer_tick_system`), `Stage::Update` × 1 (`make_animation_system()`), `Stage::PostUpdate` × 1 (`make_transform_propagation_system()`), `Stage::Physics` × 1 (`physics_sync_system`), `Stage::Late` × 4 (`camera_follow_system`, `reverb_zone_system`, `log_stats_system`, `metrics_sample_system`) = **10**.

There are no `add_to_with_access` calls anywhere else in the workspace. (A naive `grep -c "add_to_with_access"` returns 13 — it also matches two comments and one assertion-message string, which is worth noting since that is how the figure can look correct.)

## Impact

The skill already warns the number *"has drifted twice (10 → 13)"* and tells the auditor to count fresh, so no audit is misled in practice — but a stale count in a checked-in contract document is the same rot class the project files elsewhere (#2274 for `audit-safety.md`).

**The invariant itself is healthy**: zero plain `add_to(` calls for parallel systems, and `install_runtime_registries` still carries all three `debug_assert_eq!`s (`undeclared_parallel_count`, `known_conflict_count`, `unknown_pair_count` — #1394 + #1602).

## Suggested Fix

Correct the figure to 10 — or better, drop the absolute number entirely and keep only the "count it fresh" instruction plus the invariant. A number that has now drifted three times is not carrying its weight.

## Related

- #2274 (same failure mode in `audit-safety.md`)
- #2974 (TD4-2026-08-16-01 — the same class: an audit skill's own recipe drifting from the code)

## Completeness Checks
- [ ] **DROP-THE-NUMBER**: Prefer removing the absolute count over correcting it a third time
- [ ] **SIBLING**: Other audit SKILL.md files checked for pinned counts that have drifted
- [ ] **INVARIANT-INTACT**: The zero-plain-`add_to` invariant and the three `debug_assert_eq!`s are confirmed still healthy
- [ ] **PATH-GATE**: `.claude/commands/_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3035 --json state` when live state is needed.*
