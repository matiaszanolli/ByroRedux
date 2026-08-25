# CONC-D3-2026-08-24-01: live 3-edge lock cycle (Transform->GlobalTransform->CharacterController->Transform) aborts character-mode BYRO_LOCK_ORDER_CHECK runs, unreachable by CI

## Description
#2675 enumerated a complete three-edge lock cycle already present in the live schedule and fixed only the detector (depth-1 containment check → `find_path` reachability). All three edges are still present, byte-for-byte:

| Edge | Producer | Stage / mode | Held-across evidence |
|---|---|---|---|
| `Transform → GlobalTransform` | `make_transform_propagation_system` (`systems.rs:78-84`) | PostUpdate parallel | `tq` (Transform) bound before `gq` (GlobalTransform); `tq.storage_mut().drain_dirty_into(...)` at `:93` proves `tq` outlives `gq` |
| `GlobalTransform → CharacterController` | `camera_follow_system` (`character.rs:533,539`) | Late parallel | `gq` bound at `:533`, `cq` at `:539`, then `gq.get(cam_entity)` at `:548` — `gq` provably still live |
| `CharacterController → Transform` | `character_controller_system` via `player_controller_system` (`character.rs:193,205`) | Early parallel | `cq` bound at `:193`, nested `Transform` query at `:204-212` inside `cq`'s block |

Composing edges 2+3 gives `GlobalTransform ⇝ Transform`, the exact reverse of the canonical chain's own head (`docs/engine/ecs.md:597`). The detector is now *correct* (strengthened per #2675) and the graph is now *cyclic* — so it fires on real content instead of staying silent.

**Why CI still passes**: `.github/workflows/ci.yml` sets `BYRO_LOCK_ORDER_CHECK=1` in exactly two jobs — `lock-order-check` (`cargo test --workspace`) never drives `camera_follow_system`/`character_controller_system` (no test call site exists); `vulkan-validation` (`--bench-frames 5`) passes no `--cell`, so `PlayerMode` never becomes `Character` and both systems early-return before touching storage. The strengthened detector has never been run against the cycle it was strengthened to catch.

## Location
`byroredux/src/systems/character.rs:533-541` and `:193-212`; `crates/core/src/ecs/systems.rs:78-84`; detector at `crates/core/src/ecs/lock_tracker.rs:383-390`

## Trigger Conditions
A debug build with `BYRO_LOCK_ORDER_CHECK=1` running a character-mode cell (`PlayerMode::Character` with a live `PlayerEntity`). Deterministic — no timing window needed. An actual hang additionally requires two of the three producers to be co-scheduled in one stage.

## Impact
(a) With the flag set on any character-mode debug session, the process aborts on frame ~1 — the detector is unusable for the mode most gameplay work happens in. (b) Without it, a genuine ordering violation sits on the renderer-feeding pose path waiting on a stage merge or re-stage to become a silent hang with no panic and no log.

## Related
#2675 (detector fix, landed), #2388, #2135, #2547, #2387. `docs/engine/ecs.md:594-640`.

## Suggested Fix
Break edge 2: in `camera_follow_system`, copy the two `gq.get(...)` results into locals and drop `gq` before acquiring `CharacterController`. Add a test driving `make_transform_propagation_system` → `character_controller_system` → `camera_follow_system` sequentially on one `World` with `PlayerMode::Character` under `global_order::set_enabled_for_tests(true)`, asserting no panic.

## Completeness Checks
- [ ] **LOCK_ORDER**: Edge 2 broken, chain no longer cyclic
- [ ] **TESTS**: Sequential-drive test under `global_order::set_enabled_for_tests(true)` with `PlayerMode::Character`

_Source: AUDIT_CONCURRENCY_2026-08-24.md, finding CONC-D3-2026-08-24-01._
