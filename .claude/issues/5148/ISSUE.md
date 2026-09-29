# AUD-2026-09-29-D5-02: the P2 gate's oneshots_requested > 0 check passes on the footstep from combat.approach, so it cannot detect a silent combat-sound path

**Labels**: low,bug,audio,test-gap,game:skyrim

**Source**: `docs/audits/AUDIT_AUDIO_2026-09-29.md`
**Severity**: LOW
**Dimension**: Engine Consumers (smoke-gate coverage)
**Location**:
- `docs/smoke-tests/p2-melee-core.sh`: the `oneshots_requested > 0` gate.
- `crates/audio/src/lib.rs`: `oneshots_requested` — one total counter across all producers.
- `byroredux/src/commands/view.rs`: `combat.status`.
- `byroredux/src/boot/world.rs` with `byroredux/src/asset_provider/texture.rs`: the Skyrim default footstep loads whenever `--sounds-bsa` is present.

## Description
`2f8538334` added `--sounds-bsa "Skyrim - Sounds.bsa"` to `skyrim_se.env` and a gate requiring `oneshots_requested > 0` after the kill. That counter counts every `play_oneshot` call from every producer: footsteps, splashes, ripples and combat.
- With the new fixture argument, `FootstepConfig.default_sound` is decoded at boot.
- `combat.approach` places the capsule on a ring 96–144 BU from the target, well past the 52.5 BU stride, so `footstep_system` requests at least one one-shot before the first swing regardless of combat sound.
- The gate prints "PASS -- audio dispatch requested N one-shots" even if `play_oneshot_cached` never calls `play_oneshot` (wrong path, cached miss, or a regression of #4742's early-return class).

## Evidence
See Location. The `FootstepEmitter` on the camera is seeded at boot, and `single_large_jump_fires_one_footstep_only` pins one footstep per repositioning.

## Impact
Test infrastructure only. The single end-to-end audio gate proves some sound was requested, not that the combat sound family it was added for works.

## Related
#4745 (open; its filed scope — fixtures lacking `--sounds-bsa` — is fixed, this is the follow-up gate weakness), #4742, AUD-2026-09-29-D5-01 (the footstep producer).

## Suggested Fix
Either:
- read `oneshots_requested` just before the lethal swing and require a delta of at least 2 after it (impact + death voice); or
- expose per-`FeedbackSound` request counts on `combat.status` and gate on those.

Validated at HEAD 9fcfdc3fc: `p2-melee-core.sh` gates on `(( oneshots_requested > 0 ))`; `AudioWorld::oneshots_requested` is a single total counter.

## Completeness Checks
- [ ] **SIBLING**: other smoke gates that assert on the shared `oneshots_requested` total
- [ ] **TESTS**: the tightened gate is shown to fail when combat one-shots are suppressed
