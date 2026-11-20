# #7078 — Player spellcasting Phase 0: investigation notes

PRD: `docs/prd/player-spellcasting.md` (ACTIVE 2026-11-19). No engine code in this phase. Tool added: `crates/plugin/examples/spell_cast_census.rs`
(`cargo run --release -p byroredux-plugin --example spell_cast_census -- <Oblivion.esm> [EditorID substrings]`; `SPELL_CENSUS_DUMP=1` adds one
machine-readable line per Spell-type record).

## Sourced results (wiki, xEdit, ESM; the executable decode is separate)

| Item | Result | Source |
|---|---|---|
| Effect cost equation | `B × M × D × A`, B = base cost × `fMagicDurMagBaseCostMult` (0.1), M = magnitude^`fMagicCostScale` (1.28), D = duration, A = area × `fMagicAreaBaseCostMult` (0.15), each floored at 1; times `fMagicRangeTargetCostMult` (1.5) for a Target effect. Constants are game settings. | CS wiki *fMagicDurMagBaseCostMult*, *fMagicCostScale*, *fMagicAreaBaseCostMult*, *fMagicRangeTargetCostMult*; UESP *Spell Making* |
| Skill factor | `fMagicCasterSkillCostBase + fMagicCasterSkillCostMult × (1 − skill/100)`; `Oblivion.esm` authors 0.2 and 1.2 (hex `3e4ccccd`, `3f99999a`), which is `1.4 − 0.012 × skill`. Per effect, summed; a spell with the cost overridden in the editor multiplies the override by the modifier with the skill of the dominant effect. Luck modifies the skill first. | CS wiki *fMagicCasterSkillCostBase*; `Oblivion.esm` GMST |
| Auto-Calculate | "pre-calculated, based on the numeric value given to the effects" (CS wiki *Spell*); xEdit TES4 `SPIT` flag bit 0 `Manual Spell Cost`. The stored `SPIT` `Cost` of an auto-calculated spell is stale: of 315 UESP-listed auto-calc spells matched to `Oblivion.esm`, UESP's in-game Magicka equals the equation (floor or round) for 312 and the stored `Cost` for 174. Flare: stored 9, equation 11.15, UESP 11. Convalescence: stored 28, equation 48.03, UESP 48. | census (`spell_cast_census`) against UESP `OB Spell` tables |
| Mastery | CS wiki: "Spell Level: sets the skill rank necessary to cast the spell. Normally spell rank is determined by" `fMagicSpellLevel{Apprentice,Journeyman,Expert,Master}Min`. `Oblivion.esm` authors 25.5, 62.5, 149.5, 399.5 (`41cc0000`, `427a0000`, `43158000`, `43c7c000`), i.e. base cost 26 / 63 / 150 / 400. UESP's per-level tables agree with the cost thresholds for 321 of 330 matched spells and with the authored `SPIT` `Level` for 286 of 330. | CS wiki *Spell*, *fMagicSpellLevelXXMin*; `Oblivion.esm` GMST; census |
| Primary school | "The school is always the same as that of the single most expensive effect." (CS wiki *Spell*, for every spell, not only custom ones). A script effect carries its school in `SCIT` (CS wiki *Effect Item*: "School: the magic school that the effect belongs to"; xEdit TES4 `SCIT`). | CS wiki |
| Powers | Lesser Power: "Non hard-coded magicka cost ... Not affected by Skill Mastery and Spell Efficiency." Greater Power: cost 0, once a day, ignores Spell Level. Spell: affected by both. | CS wiki *Spell*; UESP *Magic Overview* |
| Range / area | Range Self, Touch, Target; Area in feet, converted with `fMagicUnitsPerFoot` (22) (CS wiki *fMagicUnitsPerFoot*); an area effect needs line of sight from its centre unless the `Area Effect Ignores LOS` flag is set (CS wiki *Spell*). Spell projectiles travel at most `fMagicProjectileMaxDistance` (10000) at `fMagicProjectileBaseSpeed` (1000). | CS wiki |
| Armor effectiveness | `fMagicArmorPenaltyMin` 0.05 and `fMagicArmorPenaltyMax` 0.3 bound the armor penalty on spell effectiveness; the interpolation between them by armor skill is not sourced. `fMagicFatigueDrainBase`/`Mult` (1 / 0) make fatigue have no effect on spell effectiveness by default. | CS wiki; `Oblivion.esm` GMST |
| Fatigue per cast | one point | UESP *Magic Overview* |
| Default cast key | `C` on PC (the engine binds `C` to Block); selecting a spell is by the spell menu or a hotkey | UESP *Oblivion:Controls* |

## Per-game scope evidence (§7.1)

- **Fallout 3 and New Vegas.** xEdit's `SPIT` types are `Actor Effect`, Disease, Power, Lesser Power, Ability, Poison and Addiction (no `Spell` type), and `Cost` and `Level` are marked `(Unused)`; the GECK calls the record an *Actor Effect* with a `PC Start Effect` flag (GECK wiki *Actor Effect*).
- **Fallout 4 and Starfield.** The wiki pages give no player-casting rule. xEdit's `SPIT` carries `Spell` and the Skyrim-lineage flags (`PC Start Spell`, `Instant Cast`, `No Dual Cast Modification`), so the record shape does not show an absence. The scope here rests on Q1 (Oblivion only), not on a claim that the games have no spellcasting.
- **Skyrim.** UESP *Skyrim:Magic Overview*: a spell is assigned to the left or right hand, concentration spells are held, dual-casting exists per school perk, and "Unlike in Oblivion, magic must be equipped to a specific hand". At HEAD no emitter in `PendingSkillAdvances` (`byroredux/src/crafting.rs` and combat) names a magic school.

## Input survey (PSC-CST-001, Q2)

`ActionBindings::default` (`byroredux/src/interaction.rs:182`) binds W S A D, Space, Left Shift, E (Activate), R (Attack), C (Block), Tab, F5, F9,
V (VATS), Left Ctrl (Sneak), G (Ready weapon); mouse left Attack, right Block. Hard-coded outside the table: Escape (game menu), F
(walk / fly toggle, `app_events.rs`), V (view toggle, shared with VATS), F3 (UI), Q (fly-cam descend). Unbound: T, Y, U, I, O, P, H, J, K, L,
B, N, M, X, Z, the digit row, F1, F2, F4, F6 to F12, the middle mouse button and the wheel. The original's PC default for Cast is `C`
(UESP *Oblivion:Controls*), which the engine uses for Block.

## Executable decode (`Oblivion.exe` 1.2.0.416; a delegated read-only decode, spot-checked)

Full results and addresses are in `docs/engine/charal-oblivion-ruleset.md` ("Spell casting"). The spot-checks done here: the three spell skill-use sites
(`0x6994cc`, `0x69b9ea`, `0x69bc51`) disassemble as `type == 0 or 5` → push 0.0 (mult) → push 1 (usage) → `0x415360` (primary-school AV) → `call [reg + 0x39c]`;
and the game settings `fFatigueCastBase` 1.0, `fHandReachMult` 0.6, `fSpellCastingDetectionMod` 20, `fMagicSpellLevel*Min`, `fMagicCasterSkillCost*` were read from
`Oblivion.esm`. Not independently re-derived: the cost function's `pow` argument order (assumed `st1^st0`), the Touch reach callees (medium), the area-award loop
(medium), and the noise link (not established). Filed as #7082.

## Census (`spell_cast_census`, `Oblivion.esm`)

1,137 `SPEL`, 145 `MGEF`. Effects of Spell and Lesser Power records: 1,061 (range Self 278, Touch 261, Target 522; area > 0: 195; with `SCIT`: 47).
SPIT flag bits set (Manual 552, ImmuneSilence1 261, PCStart 11, ImmuneSilence2 261, AreaIgnoresLOS 50, ScriptAlways 67, NoAbsorbReflect 101, TouchExplodes 13), all types.
468 non-manual Spell records: stored cost equals the floored equation for 237. UESP match: 312 of 315 (equation) against 174 (stored). Levels: UESP tables match the
cost thresholds for 321 of 330 and the stored `Level` for 286. 23 Spell records carry effects of more than one school.
