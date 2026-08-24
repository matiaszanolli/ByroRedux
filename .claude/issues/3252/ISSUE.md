# 3252: ECS-2026-08-24-04: make_animation_system() writes AnimatedTextureFlip undeclared

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_ECS_2026-08-24.md` (ECS-2026-08-24-04)

## Description

`apply_texture_flip_channels` takes a write guard on `AnimatedTextureFlip`'s storage from inside `animation_system`, but `AnimatedTextureFlip` — the 11th animated-channel sink, added this session for the `#2221` flipbook work — is absent from the system's `Access` declaration. Same shape as the now-fixed `ECS-2026-08-20-04`/`#3121` (`Children` on this same system), reintroduced by code that landed after that fix.

## Location

`byroredux/src/boot.rs:993-1027` (declaration) vs `byroredux/src/systems/animation.rs:405-416` (`apply_texture_flip_channels`), reached from `animation.rs:759` and `animation.rs:961`

## Impact

No live conflict today — `Stage::Update`'s parallel batch is a singleton, so the boot guard's counters legitimately stay at 0. The gap: a future second Update-stage parallel system touching `AnimatedTextureFlip` would silently race, since the declaration currently understates the system's true footprint.

## Related

Adjacent to closed `#3121`.

## Suggested Fix

Add `.writes::<byroredux_core::ecs::AnimatedTextureFlip>()` to `make_animation_system()` in `boot.rs`. Extend `scheduler_access_tests.rs`'s animation-declaration test to assert all eleven sinks.

## Completeness Checks
- [ ] **LOCK_ORDER**: Declaration now matches actual write surface
- [ ] **TESTS**: Extend `scheduler_access_tests.rs` to cover all 11 animated-channel sinks
