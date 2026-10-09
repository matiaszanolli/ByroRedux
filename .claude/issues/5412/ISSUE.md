# #5412: CHAR-2026-10-08-D4-01: #5239 sends a player `SetBase` on Health/AP to `set_permanent`, which **replaces** the permanent-modifier layer that constant spells also write to. The only source behind the semantics is an Oblivion CS-wiki note,...

**Labels**: low,character,bug,game:fo3,game:fnv,game:fo4
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5412

**Source**: `docs/audits/AUDIT_CHARACTER_2026-10-08.md` — `CHAR-2026-10-08-D4-01` (HEAD `00f580e09`)

- **Severity**: LOW. On FO3/FNV it changes no value today, because nothing applies a Health/AP ability to the player yet. It is a latent semantic defect plus a sourcing gap.
- **Dimension**: Population Boundary (setav/SDK write contract), with a doc-sourcing part in Coverage & Doctrine
- **Game**: fo3, fnv, fo4
- **Location**:
  - `byroredux/src/commands/actor_value.rs:82-107`: `edit_av`, where `player_pool` leads to `avs.set_permanent(av, value)`.
  - `byroredux/src/extensions/commands.rs:414-455`: `apply_pending_actor_value_writes`, the `ActorValueOperation::SetBase` arm.
  - `crates/core/src/ecs/components/actor_values.rs:152`: `set_permanent` does `permanent_mod = value`.
  - `crates/core/src/character/ruleset.rs:174-186`: the `is_player_derived_pool` doc.
  - `docs/engine/charal.md` §6, around line 414: "the GECK documents that a player `SetActorValue` never modifies base health".
- **Status**: NEW. It was introduced by the #5239 fix (`2464a52d7`); #5239 itself is closed and its stated defect is fixed.
- **Source**:
  - (1) `/mnt/data/src/reference/geck-uesp-wiki/(main)/SetActorValue.wiki` carries `CSWikiPage = SetActorValue`. Its note ("Base health is determined by (Endurance * fPCBaseHealthMult). For example, a level 2 player with 40 Endurance will have 80 base health … 180 total health - 80 from base health, and 100 for the rest") is word-for-word the note in `/mnt/data/src/reference/cs-uesp-wiki/(main)/SetActorValue.wiki`. A 40 Endurance and an END×2 base are Oblivion figures; FO3/FNV END runs 1–10, and their player Health is `fAVDHealth*`-driven.
  - (2) `/mnt/data/src/reference/falloutck-uesp-wiki/(main)/SetValue - ObjectReference.wiki`: "Sets the base value specified Actor Value on the Object Reference to the passed-in value. **Any modifiers are left intact.**" Its own example is `Game.GetPlayer().SetValue(HealthAV, 50)`.
  - (3) `crates/scripting/src/magic.rs` module doc, citing the CK *Actor Value* page: the permanent modifier is "adjusted when Abilities or Enchantments change the value". `apply_modifiers` writes constant spells there with `mod_permanent(±amount)`.
- **Description**:
  - The per-frame `refresh_player_only_bases` owns the player's Health/AP **base**. #5239 therefore redirects a deliberate `SetBase` into the permanent-modifier layer.
  - That layer is not reserved for the `SetBase` value: the magic runtime adds and removes every constant spell's Health/AP amount there. `set_permanent` overwrites the layer outright. A player who carries a +30 Health ability (permanent = 30) and runs `setav Health 100` ends with permanent = 100; the ability's 30 is gone.
  - When that ability is later removed, `remove_spell` applies `mod_permanent(-30)`, leaving 70. The result is a net loss of 30 that no source describes.
  - The FO4 CK names "modifiers left intact" as the defining property of `SetValue`, and this implementation violates it on every game. The only citation for "base untouched, value on top" is the TES4 note; FO3/FNV and FO4 have no source of their own.
- **Evidence**:
  - `actor_values.rs:152`: `self.values.entry(avif_form_id).or_default().permanent_mod = value;`, versus `magic.rs::apply_modifiers`, which does `values.mod_permanent(modifier.actor_value, modifier.amount * direction)`.
  - The out-of-tree probe found that vanilla authors constant spells that modify exactly these pools: **FNV 14, FO3 9**. Examples: `PerkLifeGiver1/2/3` Health +30/+60/+90, `PerkActionBoy*`/`PerkActionGirl*` ActionPoints +15/+25/+30, `MS03PerkSurvival*` Health +5/+10/+15, `PerkMathWrath` ActionPoints +200, `DLC03PuppiesEffect` Health +500, `CalmHeartEffect` Health +50.
  - The FNV and FO3 Player `NPC_` records author 0 spells. No perk-ability runtime exists, and FO3/FNV `SCPT` is not executed (ROADMAP). On vanilla FO3/FNV, then, the overwritten layer is 0 today. The defect becomes live once perk abilities or FO3/FNV scripted `AddSpell` reach the player, or now through an FO4 Papyrus `AddSpell`; the FO4 reach was not measured.
- **Impact**:
  - A console `setav` or an SDK mod's `SetBase` on the player's Health/AP silently discards active ability bonuses.
  - Removing the ability afterwards lowers the pool below the value that was set.
  - FO4 behaviour is the opposite of its documentation.
- **Related**: #5239 (closed; this is its follow-up), #4415 (closed; the magic layer), #5039 (the per-frame refresh).
- **Suggested Fix**:
  - Give the redirect its own layer instead of reusing `permanent_mod`. For example, add a `set_override`/`base_delta` field on `ActorValue` that `current()` adds and that `refresh_player_only_bases` never touches. Constant spells then keep `permanent_mod` to themselves.
  - Alternatively, keep `permanent_mod` but write `value + Σ(active constant amounts)`.
  - Either way, re-source the semantics per game: mark the FO3/FNV behaviour as Oblivion-derived in `charal.md` §6, and for FO4 either follow the CK (base set, refresh suspended for that key) or record the conflict.

## Completeness Checks
- [ ] **SIBLING**: both write sites (console edit_av and SDK SetBase) use the same layer
- [ ] **TESTS**: A regression test pins this specific fix
