# CHAR-2026-10-27-L-01: The VATS skill fallback comment says an uncarried skill reads neutral 50; the code reads 0 when the player has any `ActorValues`

Labels: low, character, game:fo3, game:fnv, bug
Source: docs/audits/AUDIT_CHARACTER_2026-10-27.md

**Source**: `docs/audits/AUDIT_CHARACTER_2026-10-27.md` finding `CHAR-2026-10-27-L-01` (LOW)
**Location (re-mapped to current tree)**: `byroredux/src/systems/vats.rs` (`vats_scan`, skill fallback ~:322-333; `NEUTRAL_SKILL`)

**Game**: FO3, FNV. Residue of #6422 item 7 (fixed in `1fb7e6f56` for `skill_form == 0` only).

## Description
The comment reads "A skill the player does not carry keeps the neutral value too (`ActorValues` absent altogether)". The code is `world.get::<ActorValues>(player).map(|avs| avs.current(weapon.skill_form)).unwrap_or(NEUTRAL_SKILL)`. `ActorValues::current` returns `0.0` for an absent entry (`crates/core/src/ecs/components/actor_values.rs:144-148`, "Bethesda's absent-AV default"). So a player whose component lacks that skill (e.g. a mod weapon naming a non-roster AVIF) is priced at skill 0, and `to_hit_chance` uses its "unskilled" skill term (0.5x), not the neutral one.

## Evidence
See the quoted lines above; `.unwrap_or(NEUTRAL_SKILL)` only fires when the whole `ActorValues` component is absent.

## Impact
Halved hit chance for a weapon whose skill AVIF the player does not carry. Cannot happen on vanilla (the player carries the whole roster).

## Related
#6422, #6350.

## Suggested Fix
Read `avs.get(skill_form).map(ActorValue::current)` so a missing entry falls to `NEUTRAL_SKILL`, or reword the comment to "only when the player has no `ActorValues` at all".

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix

