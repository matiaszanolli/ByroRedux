# #6923 — INC-2026-11-14-11: Door-fade visuals depend on an `AnimatedAlpha` storage that boot never registers

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-11). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: MEDIUM
- **Location**: `byroredux/src/cell_loader/relocated.rs` (`door_fade_system`, `world.query_mut::<AnimatedAlpha>()`); `byroredux/src/boot/world.rs` (`build_world`)
- **Changed in**: `relocated.rs` (commits `4b71155ca`, `98aa53db9`)

## Description
`query_mut` returns `None` for an unregistered component. `boot/world.rs` registers `CrossSpaceHop` and `DoorFade` but not `AnimatedAlpha`, which only comes into existence lazily when an alpha-animated clip inserts it (`anim_convert::insert_missing_sinks`). Until such content is loaded the XCT-ROUTE-008 door-fade ramp is silently skipped (timing and the pass still happen); `actor_alpha_system` (`SetActorAlpha`) has the same dependency. The ramp test masks it with `world.register::<AnimatedAlpha>()`.

## Evidence
`grep -c AnimatedAlpha byroredux/src/boot/world.rs` → 0; the only `register::<AnimatedAlpha>` is in the test module.

## Impact
Actors pop through load doors instead of fading.

## Related
XCT-ROUTE-008

## Suggested Fix
Register `AnimatedAlpha` in `build_world`; add a boot-registration test for the storages `door_fade_system` / `actor_alpha_system` need.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other lazily-created storages a system `query_mut`s)
- [ ] **TESTS**: A regression test pins this specific fix
