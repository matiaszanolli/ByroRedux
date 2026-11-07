# CHAR-2026-11-07-M-02: Skyrim jail sentences have no 7-day cap, and the 100-gold-per-day divisor is not in the Skyrim capture

**Issue**: #6757
**Labels**: bug, medium, gameplay, game:skyrim, character
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding M-02, Dim 3). Game: Skyrim.

## Description
`CrimeModel::SKYRIM` is `OBLIVION` with three fields flipped, so `sentence_days` returns `bounty/100` with no ceiling. Since #6569/#6573 and `36d7bfbd6`, `Faction.SendPlayerToJail` is a live Skyrim path that builds a `JailSentence` from the summed faction gold, and `serve_sentence` advances the game clock by `days x 24` hours. The `SKYRIM` doc comment ("the guards and jail settings below are Oblivion's and unused here") is no longer true for `prison_days_divisor`.

## Location
`crates/core/src/character/crime.rs` (`CrimeModel::sentence_days` at `:174-175`; `SKYRIM` inherits `prison_days_divisor: 100` via `..Self::OBLIVION`, `:121`). Callers `crates/scripting/src/jail.rs:397` (`send_player_to_jail`) and `:293`; consumer `serve_sentence` (`jail.rs:340`, `sleep.0 += sentence.days * 24.0`).

## Evidence
`jail.rs:397`: `days: model.sentence_days(sentence_gold)`; `crime.rs`: `bounty.max(0) as f32 / self.prison_days_divisor.max(1) as f32`. No `max_sentence_days` field exists.

Source: UESP *Skyrim:Crime* (`/mnt/data/src/reference/uesp-wiki/Skyrim/Crime.wiki:103`): "The maximum sentence is seven days ... You will serve the maximum sentence for any bounty 700 or higher." UESP *Oblivion:Crime* `:72` gives "one day for every 100 gold" with no cap, so the cap is Skyrim-specific. Skyrim.esm authors no `iCrimeDaysInPrisonMod` (byte scan).

## Impact
A 5000-gold Skyrim bounty serves 50 days instead of 7; schedules, timers, weather and quest clocks advance 43 extra days per arrest.

## Related
#6569, #6573. The divisor is also absent from the Skyrim capture (see the capture-lag finding in the same audit).

## Suggested Fix
Add `max_sentence_days: Option<f32>` to `CrimeModel` (`Some(7.0)` on `SKYRIM`, `None` on `OBLIVION`), clamp in `sentence_days`, pin 700 and 5000 gold, and add the divisor and cap to the Skyrim capture's Crime section citing UESP line 103.

## Completeness Checks
- [ ] **SIBLING**: the other `sentence_days` caller (`jail.rs:293`) and the Oblivion path keep their uncapped behaviour
- [ ] **TESTS**: pins at 700 and 5000 gold on Skyrim, and an Oblivion no-cap pin
