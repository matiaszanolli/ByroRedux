# AUD-2026-09-21-D5-02: (latent) Combat sounds are keyed to take-state transitions, dropping most impacts, the killing blow, and misattributing the swing

**Issue**: #4743
**Filed**: 2026-09-22 (audit-publish, AUDIT_AUDIO_2026-09-22.md)

**Severity**: LOW (latent until #4700 and #4742 land)
**Dimension**: 5 — Engine Consumers
**Location**: `byroredux/src/systems/combat_anim.rs:153-241` (decision ladder), `:18-21`, `:202-204`, `:224-239`, `:123-148`

## Description
The per-actor decision ladder runs Dead → active-take → HitEvent → aggressor, in that order, with sound only from the Dead and HitEvent install branches. (a) An active take (step 2) runs before a fresh HitEvent (step 3), so most hits during stagger/attack takes play no impact — the "dedup" rationale in the code comment is wrong, since each `HitEvent` is visible to exactly one read. (b) `Dead` is inserted the same frame as the lethal `HitEvent`, so the killing blow never reaches the impact branch. (c) The swing one-shot fires on the player's own `attacks_started` delta regardless of weapon, while the enemy's own attack take carries `sound: None` — so the Draugr's real attacks stay silent.

## Evidence
Line references above; real-data clip durations from `docs/engine/p2-combat-anim-sound-fixture.md`; `clips.hit_secs`/`attack_secs` from `asset_provider/animation.rs:297-316`.

## Impact
Once reachable (after #4700/#4742), most player hits on a Draugr would still be silent, the kill silent apart from the death voice, and the enemy's own attacks soundless.

## Related
#4742 (this report, same function, should land first); #4700 and #4708 (`AUDIT_GAMEPLAY_2026-09-21.md`, same ladder's death-replay defect); #4564 (fixture doc vs pinned attack clip).

## Suggested Fix
Decouple sound from the take ladder: emit the impact for every `HitEvent` on a marked target regardless of take state or same-frame death; fire the swing at the aggressor's own attack-take start and position; drop the "dedup" rationale.
