# 3260: CONC-D3-2026-08-24-01: live 3-edge lock cycle aborts character-mode BYRO_LOCK_ORDER_CHECK runs

**Severity**: HIGH · **Report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-24.md` (CONC-D3-2026-08-24-01)

## Description

#2675 enumerated a complete three-edge lock cycle already present in the live schedule and fixed only the detector (depth-1 containment check → `find_path` reachability). All three edges are still present:

| Edge | Producer | Stage / mode |
|---|---|---|
| `Transform → GlobalTransform` | `make_transform_propagation_system` (`systems.rs:78-84`) | PostUpdate parallel |
| `GlobalTransform → CharacterController` | `camera_follow_system` (`character.rs:533,539`) | Late parallel |
| `CharacterController → Transform` | `character_controller_system` via `player_controller_system` (`character.rs:193,205`) | Early parallel |

Composing edges 2+3 gives `GlobalTransform ⇝ Transform`, the reverse of the canonical chain's own head. The detector is now correct (strengthened per #2675) and the graph is now cyclic.

**Why CI still passes**: `.github/workflows/ci.yml`'s `lock-order-check` job never drives `camera_follow_system`/`character_controller_system`; `vulkan-validation` passes no `--cell` so `PlayerMode` never becomes `Character`.

## Location

`byroredux/src/systems/character.rs:533-541` and `:193-212`; `crates/core/src/ecs/systems.rs:78-84`; detector at `crates/core/src/ecs/lock_tracker.rs:383-390`

## Trigger Conditions

A debug build with `BYRO_LOCK_ORDER_CHECK=1` running a character-mode cell (`PlayerMode::Character` with a live `PlayerEntity`). Deterministic — no timing window needed.

## Impact

(a) With the flag set on any character-mode debug session, the process aborts on frame ~1. (b) Without it, a genuine ordering violation sits on the renderer-feeding pose path waiting on a stage merge to become a silent hang.

## Related

#2675, #2388, #2135, #2547, #2387.

## Suggested Fix

Break edge 2: in `camera_follow_system`, copy the two `gq.get(...)` results into locals and drop `gq` before acquiring `CharacterController`. Add a sequential-drive test under `global_order::set_enabled_for_tests(true)` with `PlayerMode::Character`.

## Completeness Checks
- [ ] **LOCK_ORDER**: Edge 2 broken, chain no longer cyclic
- [ ] **TESTS**: Sequential-drive test under `global_order::set_enabled_for_tests(true)`
