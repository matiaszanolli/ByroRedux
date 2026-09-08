# Issue #4064 — ECS-2026-09-08-D5-01: the declaration-completeness test covers only the two systems where declarations are not load-bearing

Filed: 2026-09-08 from `docs/audits/AUDIT_ECS_2026-09-08.md` via `/audit-publish`
Repo state at filing: `bb8ced68`
Labels: low, ecs, concurrency, test-gap, bug

---

**Severity**: LOW · **Dimension**: 5b (scheduler access declarations — test gap)
**Location**: `byroredux/src/boot.rs` — `assert_declares_everything_it_acquires` and its two `#[test]` callers in `mod scripting_system_access_declaration_tests`
**Source**: `docs/audits/AUDIT_ECS_2026-09-08.md`

## Description

`assert_declares_everything_it_acquires` (#3951/#3473) compares a system's declared `Access` against the storages and resources its body actually acquires. It is invoked on exactly two systems — `papyrus_provider_system` and `legacy_obscript_load_order_system` — both registered with `add_exclusive_with_access`. The test's own assertion message concedes the point: *"These declarations do not gate scheduling today."*

Meanwhile the nine systems registered with `add_to_with_access` **do** gate scheduling. `install_runtime_registries`'s three release assertions (`undeclared_parallel_count`, `known_conflict_count`, `unknown_pair_count`) are the construction-time deadlock proof for every same-stage parallel batch, and that proof is only as sound as the declarations it analyses. An under-declared parallel system yields `known_conflict_count() == 0` while a real write/write overlap sits in the batch.

Two secondary fragilities in the same helper:

1. `declaration()` locates the block with `setup.find(system)` — the *first* textual occurrence — so a `use` import or a prose mention of the system name earlier in `boot.rs` silently redirects the extraction to a wrong slice that still contains type names and passes vacuously. Both current subjects happen to be first-mentioned at their registration site, so it works today by luck. (Verified by accident during the audit: running the same extraction against `metrics_sample_system`, whose name appears first in the `use` list at the top of the file, produced a false "missing declaration" result.)
2. `acquired()` scans only the system's own body, so a dispatcher like `player_controller_system` — whose body acquires exactly one resource and delegates everything else to `fly_camera_system` / `character_controller_system` — would trip the helper's own `types.len() > 3` floor rather than produce a real comparison.

## Evidence

Applying the test's own extraction to all nine `add_to_with_access` registrations finds **no missing declaration today**:

| system | direct acquisitions | missing |
|---|---|---|
| `player_controller_system` | 1 | — |
| `timer_tick_system` | 2 | — |
| `make_animation_system` | 0 | — |
| `make_transform_propagation_system` | 4 | — |
| `physics_sync_system` | 1 | — |
| `camera_follow_system` | 7 | — |
| `reverb_zone_system` | 2 | — |
| `log_stats_system` | 5 | — |
| `metrics_sample_system` | 8 | — |

So there is no live defect, only an unguarded invariant. The three factory/dispatcher systems return 0–1 direct acquisitions, which is why a naive extension needs the delegation followed or the entry point moved.

## Impact

No runtime effect today. The nine parallel declarations are currently correct and visibly hand-maintained — `make_transform_propagation_system` even carries a "WRITE (was read)" note explaining that it takes a write guard purely to drain the dirty set.

The gap is that nothing keeps them that way, on exactly the set where drift is *unsound* rather than merely undiagnostic.

## Related

- #3951 / #3473 introduced the helper for the two exclusive systems.
- #1394 / #1602 / #2690 are the boot assertions this test would be protecting.

## Suggested Fix

Run `assert_declares_everything_it_acquires` over the nine `add_to_with_access` registrations too:

- anchor `declaration()` on the `add_to_with_access(` / `add_exclusive_with_access(` block *containing* the name, rather than on the name's first occurrence anywhere in the file;
- either follow one level of delegation, or assert against the dispatch target's body, for the three factory/dispatcher entries (`player_controller_system`, `make_animation_system`, `physics_sync_system`).

## Completeness Checks
- [ ] **LOCK_ORDER**: any declaration corrected by the widened test must still match the canonical acquisition order in `docs/engine/ecs.md`
- [ ] **TESTS**: the widened test fails when a declaration is deleted from any one of the nine parallel registrations (fault-inject each before landing)
