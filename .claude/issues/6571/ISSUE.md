# SCR-2026-10-31-02: `tempering_multiplier` leaves a `SceneAliasCandidate` on the `TemperingSubject` scratch entity forever

Labels: low,scripting,bug,game:skyrim

Source: `docs/audits/AUDIT_SCRIPTING_2026-10-31.md` (SCR-2026-10-31-02). Severity LOW. Dimension: Scene/Package/Cinematic (alias substrate).

**Location**: `crates/scripting/src/tempering.rs:64-72` (lend), `:52-58` (`install` spawns the entity); consumers `byroredux/src/crafting.rs:223,258,329`

## Description
- To give item-tab conditions a subject, `tempering_multiplier` inserts `SceneAliasCandidate { base_form_id: item_base, .. }` on a scratch entity spawned once at boot. It resets `TemperingItem.enchanted` afterwards but never removes the candidate (the sibling `with_item_enchanted` restores its mark).
- `refresh_scene_actor_bindings` enumerates every `SceneAliasCandidate`; its eligibility closure (`quest_alias.rs:~1180`) checks only `Dead`, reuse and reservation, with no liveness or `GlobalTransform` test on the candidate path. After the first improve action a placeless candidate with `reference_form_id = 0` and the last item's base FormID is a standing alias candidate.
- Other scanners (`mines.rs`, `attraction.rs`, `random_conversation.rs`) also read `GlobalTransform`, which the scratch entity lacks, so they skip it.

## Impact
Latent. A Find-Matching alias gated on `GetIsID(<that weapon or armor base>)` could bind the scratch entity, pointing the alias at nothing placeable. No vanilla alias of that shape found.

## Suggested Fix
Remove the candidate (or the whole component) from the scratch entity at the end of `tempering_multiplier`, as the enchanted flag already is, or keep the base form in `TemperingItem` and have the condition subject read it from there.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
