# CHAR-2026-11-07-L-04: The Skyrim DamageResist multiplier row (and Oblivion's armor-rating rows) have no reader, and the prose about where Skyrim armor lives is stale

**Issue**: #6763
**Labels**: documentation, low, game:skyrim, game:oblivion, character, doc-rot
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding L-04, Dim 2 + Dim 5). Game: Skyrim, Oblivion.

## Description
The Skyrim `DamageResist` derived row is `PlayerOnly` + `Multiplier`, so `GetActorValue DamageResist` returns the carried entry, never `1 + 0.004·LightArmor`; nothing else calls `derived_value` for it. The Skyrim capture's claim that the row and the armor model "can disagree" through a `GetActorValue` read is wrong. Oblivion's `LightArmorRating`/`HeavyArmorRating` rows are equally unread (Oblivion has no runtime ruleset). Separately, `charal.md` still says `byroredux/src/armor.rs` reads the CHARAL constants and never mentions the `ArmorModel` profile row (#6567).

## Location
`docs/engine/charal-skyrim-ruleset.md:306-309` (added by #6568); `docs/engine/charal.md:345-346`; rows at `crates/core/src/character/skyrim.rs:133-144` and `crates/core/src/character/tes.rs:138-161`.

## Evidence
`crates/core/src/character/ruleset.rs:161-166`: `derives_for_any_actor = ... formula.scope == DerivedScope::ActorGeneral && formula.kind == DerivedOutput::Absolute; if !derives_for_any_actor { return avs.current(avif); }`. `refresh_player_only_bases` also skips `Multiplier`. `charal.md:345`: "the Skyrim armor model itself is `byroredux/src/armor.rs`, which reads this constant and `ARMOR_SKILL_RATING_COEFF_NPC` 0.015".

## Impact
Misleads the next reader about where Skyrim armor lives (only in `ArmorModel`); the rows are tested coefficient carriers with no runtime effect.

## Related
#6567, #6568, #2933.

## Suggested Fix
Correct the sentences: the row has no reader (a `Multiplier` row is never composed by `actor_value`); the Skyrim armor model is the `ArmorModel` profile row (`character/armor.rs`), which `byroredux/src/armor.rs` reads and which derives its skill multipliers from the two constants. Or retire the Skyrim row and its `DamageResist` pin tests.

## Completeness Checks
- [ ] **SIBLING**: grep the other capture docs, `charal.md`, `feature-matrix.md`, `ROADMAP.md` and code comments for the same stale claim
- [ ] Cite symbols, not line ranges, in the corrected prose
