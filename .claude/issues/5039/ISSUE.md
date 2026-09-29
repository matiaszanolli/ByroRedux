# CHAR-2026-09-29-D1-01: #4674 materialises the player's `PlayerOnly` derived stats (Health/AP) into `ActorValues` base at stamping — any later Endurance/Agility/level change, and constant spells applied at the same attach, leave them stale

**Labels**: medium,bug,character,game:fo4,game:fo3,game:fnv

**Source report**: `docs/audits/AUDIT_CHARACTER_2026-09-29.md`

- **Severity**: MEDIUM
- **Dimension**: Ruleset Seam
- **Game**: FO4 (capture explicitly requires dynamic rescale); FO3 / FNV (same mechanism)
- **Source**: charal-fo4-ruleset.md:77-78 ("health rescales **dynamically** with any Endurance / level change — there is no permanent/temporary split for player HP"). charal.md §6 (decision: "derived stats … are **computed on demand** … **not** materialised into `ActorValues` at spawn … so an attribute change can't leave a stale derived value behind").
- **Location**:
  - `byroredux/src/inventory.rs:340-436` (`build_player_character_template`; the write is `values.set_base(key, value)` at `:400`)
  - `byroredux/src/inventory.rs:698-735` (`attach_to_player`; `apply_modifiers` runs at `:730`, *after* evaluation)
  - `docs/engine/charal.md:377-392` (§6)
  - `docs/engine/charal-fo4-ruleset.md:96-106` (caveat 2, "Player application LANDED")
- **Status**: NEW. #4674's fix is correct for spawn values. Its own Impact line ("A temporary END change cannot rescale HP, which the capture requires") was not addressed.
- **Description**: The skill's single-sink rule says no site besides the ruleset writes derived stats straight into `ActorValues`, and charal.md §6 records that as a design decision. The #4674 fix does it anyway. It evaluates the player's `PlayerOnly` Absolute rows once, against the seed, and writes the results into the **base** layer, so they become ordinary carried values. What follows:
  - Every consumer takes the carried fast path: `GetActorValue`, `vitals_snapshot`, combat damage and drowning.
  - No formula is ever re-evaluated.
  - `attach_to_player` applies the record's constant-spell modifiers *after* the evaluation. An Endurance/Agility ability on the Player record therefore shifts END/AGI but never reaches HP/AP.

  Production writers that can change the inputs today:
  - `magic::add_spell` / `remove_spell` (a scripted `AddSpell` of an attribute ability)
  - The SDK `ActorValueOperation::{SetBase, ModifyPermanent, ModifyTemporary}` in `byroredux/src/extensions/commands.rs`
  - Console `setav` / `modav`

  The stale base is what the save serialises. The first level-up or XP writer, when it lands, inherits the same problem.
- **Evidence**:
  ```rust
  // inventory.rs:398-401 — evaluated once, written as base
  if let Some(value) = ruleset.derived_value(key, &values, level) {
      values.set_base(key, value);
  }
  // inventory.rs:728-731 — modifiers land afterwards
  if let Some(mut values) = character.values {
      byroredux_scripting::magic::apply_modifiers(&mut values, &character.spell_modifiers, 1.0);
  ```
  charal.md:381 still says derived stats are "**not** materialised into `ActorValues` at spawn".

  The vanilla spawn value is right. The probe shows the FO3/FNV Player and its race carry no SPLO, and the FO4 race spell 0x1CCDA3 touches none of END/AGI/Health/AP. The defect is the first attribute change after spawn.
- **Impact**:
  - FO4 player HP does not follow END/level. The capture calls that the defining property of the stat.
  - FO3/FNV HP and AP do not follow END/AGI/level.
  - The HUD, combat damage pool, drowning and CTDA reads all see the frozen value.
  - The design doc and the code now disagree on a stated decision, and the FO4 caveat reports the application as "LANDED".
- **Related**: #4674 (closed; its Impact line named this), CHAR-2026-09-29-D4-01 (same composition gap on the NPC side), #4415 (open; modifier writer), #2947 (level/XP persistence)
- **Suggested Fix**: Pick one of two:
  - Keep the player's `PlayerOnly` stats derived (`base` = formula output recomputed on read, or a re-evaluation system keyed on END/AGI/level change events), with modifiers layered on top.
  - Or amend §6 and the FO4 caveat to document stamping as a deliberate interim, and re-evaluate on every attribute writer, including `attach_to_player`'s own modifier application (evaluate *after* `apply_modifiers`, using `current`).

  Either way, add a test that `modav Endurance` on the player changes FO4 Health.

**Validated at HEAD 9fcfdc3fc**: `build_player_character_template` in `byroredux/src/inventory.rs` still does `if let Some(value) = ruleset.derived_value(key, &values, level) { values.set_base(key, value); }`, and `attach_to_player` calls `magic::apply_modifiers` afterwards; `docs/engine/charal.md` §6 still says derived stats are "**not** materialised into `ActorValues`".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
