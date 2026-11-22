# CHARAL — Oblivion character ruleset (data capture)

Living capture of **Oblivion** gameplay-system formulas that sit downstream
of the already-complete core ruleset (`AttributeSet::TES_CLASSIC` — shared
with Morrowind, hence the name rather than a per-game one —
`SkillSet::OBLIVION`, `LevelingModel::OBLIVION`, `oblivion_attribute_bonus`,
`oblivion_health_formula` — all shipped, see `crates/core/src/character/
tes.rs` and `charal.md` §5; the ruleset is wired since #7010
(`RulesetBuilder::Oblivion`), over the 72 actor values the loader supplies because
`Oblivion.esm` authors no `AVIF` records). This doc
exists for the same reason `charal-skyrim-ruleset.md` does even though
Skyrim's core is also done: real, sourced, downstream gameplay math that
isn't a core derived stat but is still worth capturing precisely. LOCKED
(sourced) / PENDING conventions per [[feedback_no_guessing]]. Parent:
[charal.md](charal.md).

## Vampirism — same 24h-tier mechanism shape as Skyrim, cross-game confirmed

Source: UESP *Oblivion:Vampirism*, 2026-07-04, fetched immediately after
`charal-skyrim-ruleset.md`'s Vampirism section for direct comparison. Uses a
"Vampirism Level" percentage (25/50/75/100%) instead of Skyrim's "Stage 1-4"
naming, but the **mechanism is identical in shape**: tier advances every 24
hours since last feeding, feeding always resets to the lowest tier, capped at
the top tier (no further escalation past 72+ hours).

```
Tier ∈ {1,2,3,4} by hours-since-feeding: 0–24h / 24–48h / 48–72h / 72h+
VampirismLevel% = 25 × Tier                              (25/50/75/100 — clean affine)
AttributeAndSkillBonus = +5 × Tier                        (+5/+10/+15/+20 — clean affine)
WeaknessToFire%  = 10 + 10 × Tier                         (20/30/40/50 — clean affine)
ResistNormalWeapons% = 5 × Tier                           (5/10/15/20 — clean affine)
SunlightDamage (HP/sec) = 0 / 1 / 4 / 8                   (NOT affine — a table)
ResistDisease% = ResistParalysis% = 100                   (flat, every tier — not tier-scaled)
```

**Bonus, not penalty** — a structural difference from Skyrim worth keeping
straight: Oblivion vampirism *raises* Strength/Willpower/Speed and
Acrobatics/Athletics/Destruction/Hand-to-Hand/Illusion/Mysticism/Sneak by a
flat `+5×Tier`, whereas Skyrim's "Weakness to Sunlight" only ever *lowers*
Health/Magicka/Stamina by `−15×Stage`. Both games tax the player with rising
sunlight damage, but Oblivion additionally *rewards* higher tiers with
permanent attribute/skill bonuses (implemented as abilities, so they raise
the *base* skill value, not a temporary modifier) — Skyrim has no equivalent
upside.

**Cross-game confirmation of the affliction-family fit.** Both Oblivion and
Skyrim Vampirism share the exact shape flagged as a stronger affliction-family
candidate than Disease in `charal-skyrim-ruleset.md`: a monotonic
"hours-since-last-feeding" pool crossing fixed 24h-multiple thresholds, reset
to zero by a trigger event (feeding), with mostly-affine per-tier penalties/
bonuses and one flat resistance stat that doesn't scale with tier. This is now
a **two-game-confirmed pattern**, not a Skyrim one-off — real weight behind
treating "time-since-trigger-event" as a legitimate second `AfflictionTable`
pool shape (alongside Fallout's damage-accumulation pools), if Vampirism is
ever built. Still not built — no feed/reset trigger or day-night-cycle
consumer system exists in either game's runtime yet.

Also notable: becoming a vampire in Oblivion is itself a disease
(*Porphyric Hemophilia*) with its own 72-hour incubation + sleep-triggered
progression (3 sleeps to become a full vampire) — an *entry* mechanism on top
of the *stage* mechanism above, orthogonal to it. Not decomposed further here
(no formula beyond "3 sleeps, ~24h apart minimum").

## Disease — cross-game confirms Skyrim: NOT the affliction-family shape here either

Source: UESP *Oblivion:Disease*, 2026-07-04, fetched right after Vampirism
for the same side-by-side comparison. Confirms `charal-skyrim-ruleset.md`'s
finding rather than adding a new one: Oblivion disease is **also** a discrete
binary status system, not a pool/threshold mechanism. ~30 diseases, each a
**fixed** `Drain <Attribute> Npts` set (e.g. Ataxia: Drain Strength 5,
Drain Agility 5) with no accumulation, no severity tiers, no time-based
escalation (unlike Skyrim's Survival-Mode 3-rung ladder — vanilla Oblivion
disease doesn't even have that). Two diseases (*Astral Vapors*, *Porphyric
Hemophilia*) are explicitly noted as bypassing Resist Disease entirely — the
latter by deliberate design (patch 1.1.511, so Vicente Valtieri's scripted
vampirism-gift quest always works) — a per-disease exception flag, not a
formula. No numeric infection-chance formula given here either (same PENDING
gap as Skyrim). **Two-game-confirmed now**: Disease is a fixed-effect status
table in both TES games covered so far, Vampirism is the one with real
pool-like math — the divide holds cross-game, not just within Skyrim.

## Enchanting — the Enchant SKILL has no functional role at all, unlike Skyrim's quadratic multiplier

Source: UESP *Oblivion:Enchanting*, 2026-07-04, fetched right after Skyrim's
Enchanting page (`charal-skyrim-ruleset.md`) for direct comparison. The
contrast is sharp and well-supported: **the word "skill" never appears in
connection with enchantment magnitude, cost, or recharging anywhere on this
page.** The full formula is:
```
EffectMagnitude = BaseCost × ConstantEffectEnchantmentFactor × SoulLevel + fMagicCEEnchantMagOffset
ConstantEffectEnchantmentFactor = (Power − 5) / SoulGemNumber / BaseCost
EnchantmentCost = EffectMagnitude × BarterFactor

SoulGemNumber: Petty=1, Lesser=2, Common=3, Greater=4, Grand=5
fMagicCEEnchantMagOffset = 5.0 (default GMST)
```
Every input is either a spell-effect constant (`BaseCost`, `Power`,
`BarterFactor` — properties of the *effect*, from the Spell Effects page) or
the **soul gem's** size (`SoulLevel`/`SoulGemNumber`) — **never the player's
Enchant skill**. Recharging (via gold payment, Varla Stones, or Soul
Trapping) is likewise entirely item/gold-based, no skill term anywhere.

**This is the sharpest cross-game divergence found in the whole Enchanting/
Alchemy/Armor skill-multiplier investigation this session.** Skyrim's
Enchanting has a central, quadratic skill-derived `SkillMultiplier =
1+(Skill/100)×(Skill/100−0.14)/3.4` term (`charal-skyrim-ruleset.md`);
Oblivion's Enchant skill — despite being a full skill in `SkillSet::OBLIVION`
governed by an attribute, with its own trainers, skill books, and presumably
level-up progression like any other Oblivion skill — **contributes nothing
measurable to the system it's named after**, at least per what's documented
on this page. This is a genuine "Oblivion vs. Skyrim system redesign"
finding, not a documentation gap on either page's part (both pages are
otherwise formula-dense and precise) — Bethesda restructured what a skill
called "Enchanting" actually *does* between the two games. Worth remembering
before assuming any Oblivion skill has a Skyrim-style smooth multiplier just
because the same-named skill does in Skyrim.

**Refinement from *Oblivion:Soul Trap*, 2026-07-04**: confirms directly,
in the source's own words, that "your Mysticism skill does **not** determine
the strength of souls that you can trap" — the captured soul's strength is
purely a property of the creature/NPC killed (a fixed table: Petty 150 →
Grand/Black 1600), matched against soul gem capacity, no skill term
anywhere. But magic skills in Oblivion aren't *entirely* inert the way
Enchanting first looked: "increased Mysticism skill will allow you access to
Soul Trap spells of longer duration, and will allow you to cast the spells
for less Magicka cost." So the real shape is: **magic skills gate spell-tier
*access* and reduce **casting** Magicka cost** (a universal per-school
mechanic, not specific to Soul Trap) — **but never scale the magnitude or
success of the effect itself**. This is a more precise statement of the
Enchanting finding above, not a contradiction: Enchant/Mysticism genuinely
don't scale *outcomes* (enchantment magnitude, soul-capture strength), they
gate *access* and *casting cost* instead — the opposite emphasis from
Skyrim, where Enchanting's `SkillMultiplier` scales the outcome directly.
Also surfaces a new **PENDING** item mirroring the still-open Skyrim thread:
Oblivion has its own generic "spell Magicka cost decreases with skill"
mechanic, formula not sourced here — presumably on an Oblivion Magic
Overview / Spellmaking page, not fetched this session.

## Magic school spell cost — LOCKED, closes the PENDING thread above, clean affine (unlike Skyrim's quadratic)

Source: UESP *Oblivion:Illusion*, 2026-07-04 — "Skill Benefits" section:
```
Cost = BaseCost × (1.4 − 0.012 × Skill)      (Skill capped at 100 — no further reduction past it)
```
At Skill 0: ×1.4 (140% of base — casting is *more* expensive than
`BaseCost` at zero skill); at Skill 100: ×0.2 (20% of base, the floor).

**Cross-confirmed on a second school**: UESP *Oblivion:Destruction*,
2026-07-04, gives the **identical** equation verbatim (`Cost = BaseCost *
(1.4 - 0.012 * Min(Skill, 100))`) under the same "Skill Benefits" heading.
Same bias, same coefficient, same cap — not just the same template wording.
This closes the generality caveat from the Illusion finding: it's now a
**two-school-confirmed single game-wide mechanic**, not a coincidence of
shared wiki-template phrasing. Treat it as the formula for all 6 Oblivion
magic schools (Destruction, Restoration, Alteration, Conjuration, Mysticism,
Illusion), each reading its own governing skill.

**Clean affine — a genuine contrast with Skyrim's Enchanting `SkillMultiplier`
(quadratic, `1+(Skill/100)×(Skill/100−0.14)/3.4`)**, closing the loop opened
by the Soul Trap page: Oblivion's magic-school cost-reduction mechanic is a
straight line in skill, not a curve. Two data points isn't enough to call a
universal "Oblivion is affine, Skyrim is quadratic" rule, but it's a real,
sourced contrast worth keeping distinct rather than assuming either
game's shape carries over to the other.

The tier gate and the experience are in the next section: the tier comes from the base cost (not a per-skill table) and
the experience from the `SKIL` use values (Alteration 4, Conjuration 6, Destruction 1.2, Illusion 3, Mysticism 3,
Restoration 0.6), not a flat 3 per cast (corrected 2026-11-19, #7078). The cost is not a CHARAL derived stat (it is a
spell-casting economy, not a character actor value), but its functions live beside `skill_use.rs` in
`crates/core/src/character` (PSC-CST-008) so the cast code holds no constant.

## Spell casting (#7078, provenance relabelled 2026-11-20)

The rules the [player-spellcasting PRD](../prd/player-spellcasting.md) builds. Sources: UESP *Oblivion:Magic Overview* and
*Spell Making*; the CS wiki (*Spell*, *Effect Item*, the `fMagic*` setting pages); xEdit `wbDefinitionsTES4.pas`; `Oblivion.esm`
(`crates/plugin/examples/spell_cast_census.rs`, 1,137 `SPEL` and 145 `MGEF` records); and a decode of `Oblivion.exe` 1.2.0.416
(addresses are VAs, image base `0x400000`; raw notes `.claude/issues/7078/INVESTIGATION.md`).

**Provenance (relabelled 2026-11-20).** The owner ruled out decoding the executables for rules, so this section is held to documented sources. Most of what
the 2026-11-19 decode found is documented, and each item below names its source; the `0x...` addresses in the text are the decode's evidence, **not** a source.

| Rule | Source now | Status |
|---|---|---|
| Effect cost equation (`0.1 × base × magnitude^1.28 × duration × area×0.15 × 1.5 for Target`, each factor's minimum of 1, floor per effect, minimum 1) and the sum | CS wiki *Category:Spell Cost* (formula and rounding) with the `fMagic*` setting pages | **Documented**; matches UESP's listed costs for 312 of 315 spells |
| Skill factor `fMagicCasterSkillCostBase + fMagicCasterSkillCostMult × (1 − skill/100)` and the Luck term (`iActorLuckSkillBase` −20, `fActorLuckSkillMult` 0.4) | CS wiki *fMagicCasterSkillCostBase*, *Effective Skill Level*; UESP *Magic Overview* ("every 5 points of Luck above 50 is 2 points of skill") | **Documented** |
| Manual Spell Cost: `floor(stored × skill factor)`, no minimum | CS wiki *Category:Spell Cost* | **Documented; corrected 2026-11-20** (the decode's "not rounded" contradicted it; the code floors now) |
| Mastery from the unscaled (base) cost against `fMagicSpellLevel*Min`; the stored `Level` for a Manual spell; the player's tier from the unfortified skill (25 / 50 / 75 / 100) | CS wiki *Category:Spell Cost*, *Spell*, *ISkillXXMin*; UESP *Magic Overview* ("your unfortified skill level") | **Documented** |
| Primary school = the most expensive unscaled effect | CS wiki *Spell*, *Category:Spell Cost* | **Documented**; the `SCIT` school of a script effect winning over the `MGEF`'s is **executable-derived** (no source) |
| Fatigue per cast `fFatigueCastBase + fFatigueCastMult × cost` (1 + 0) | CS wiki *Fatigue Game Settings*; UESP *Magic Overview* ("one point of Fatigue") | **Documented** |
| Who is fed: a Self cast always, a Touch or Target cast only on a target that can be affected; Powers, Greater Powers and scrolls feed nothing | UESP *Magic Overview*, "Skill levels and training" | **Documented**; the *count* (once at the release for Self and Touch, once at impact for Target) and the use value (the `SKIL` second use value) are **executable-derived** where UESP's "4.0 per cast" amounts are the only documented figures |
| Spell projectile speed and range | CS wiki *Magic Effects*, *fMagicProjectileBaseSpeed*, *fMagicProjectileMaxDistance* | **Documented** |
| Area radius `fMagicUnitsPerFoot` 22 × area | CS wiki *fMagicUnitsPerFoot* | **Documented**; not built (area ignored) |
| `MGEF` flag bit 22 skips an effect in the cost sum and the school choice | none | **Executable-derived**, kept, declared (#7082) |
| Touch reach | [Original-game trial below](#touch-distance-trial-7082) | **Observed**: Cold Touch follows `iActivatePickLength` (stock ray length 150 BU). It hit a scale-one guard at 183 BU reference distance and missed at 184 BU (#7082) |
| Magicka refusal only when the cost is *greater than* current Magicka | none | **Executable-derived**, kept (the equal case allowed) |
| Refusals for silence, underwater and bound effects; the PC-start spell level | none | **Executable-derived**; not built (PSC-NG-008) |
| Stationary casting and sound detection | Original-game trial below | **Observed**: no detection score change for the tested casts (PSC-NG-007) |

### The cost

`GetMagickaCost` is `0x41d320(caster)`. A Power or Lesser Power (type 2 or 3) gets no caster, so no skill scaling.

- **Per effect** (`0x413810` → `0x548b50`): `0.1 × MGEF base cost × D × A × M × T` with `D` the duration (1 when 0), `A = max(1, area ×
  fMagicAreaBaseCostMult 0.15)`, `M = magnitude^fMagicCostScale` (1 when the magnitude is 0; 1.28 in `Oblivion.esm`, 1.25 the executable
  default) and `T = fMagicRangeTargetCostMult 1.5` for a Target effect. MGEF flag `0x100` zeroes the magnitude, `0x80` the duration, `0x200` the
  area; a Self effect has no area. Effects whose MGEF has flag bit 22 are skipped in the sum.
- **Auto-calculated spell** (`Manual Spell Cost` clear): `0x4151c0` sums `⌊effect cost × modifier⌋` (at least 1 each), the modifier taken from
  that effect's own school skill (`0x413890`). The stored `SPIT` `Cost` is **stale and unused**: of 315 UESP-listed auto-calculated spells matched to
  `Oblivion.esm`, UESP's in-game Magicka equals the equation for 312 and the stored cost for 174 (Flare: stored 9, equation 11.15, UESP 11).
- **Manual Spell Cost spell** (bit 0): `floor(stored Cost × modifier)`, no minimum (CS wiki *Category:Spell Cost*; an earlier reading of the decode left it
  unrounded), with the primary school's skill (`0x41d38a`). Mirili's Expert Command (`SE37CommandCreature4Expert`) stores 368 against an equation 491, and
  costs 294 at skill 50.
- **Modifier** (`0x548c00`): `fMagicCasterSkillCostBase + fMagicCasterSkillCostMult × (1 − S ÷ 100)`, `S = clamp(skill + fActorLuckSkillMult × Luck +
  iActorLuckSkillBase, 0, 100)`. `Oblivion.esm` authors Base 0.2 and Mult 1.2 (the executable defaults 0.1 and 1.2), so the modifier is `1.4 − 0.012 × S`;
  the Luck constants are the executable's 0.4 and −20 (Luck adds `0.4 × (Luck − 50)`). Luck applies to the real cast: the can-cast check `0x5f4570` and the
  spender `0x5fc8fa` call the same function. (Resolved by the decode, which the CS wiki states for the base: "luck will modify the skill somewhat".)

### The gates

`CanCast` is `0x5f44a0`. For a Spell (type 0): Magicka — refused when the cost is **greater than** current Magicka (equal allowed), code 1; mastery
(player only, NPCs are not gated) — the player's tier from the **base** skill (so Fortify does not help) against `iSkillApprenticeMin`, `JourneymanMin`,
`ExpertMin`, `MasterMin` = 25 / 50 / 75 / 100 (executable defaults, not overridden), must be at least the spell's level, code 3. Other refusals: silenced 2,
ranged underwater 6, several bound effects 5, a Power already used 4. The player sees the message through `0x5f3e00`.

**The spell's level** (`0x41d3c0` → `0x548c50`): for an auto-calculated spell, the tier of its caster-less base cost against
`fMagicSpellLevelApprenticeMin`, `JourneymanMin`, `ExpertMin`, `MasterMin` (`Oblivion.esm`: 25.5, 62.5, 149.5, 399.5, so costs 26 / 63 / 150 / 400), for a
Manual spell the stored `SPIT` `Level`. UESP's per-level tables agree with the cost thresholds for 321 of 330 matched spells and with the authored `Level` for 286.
The CS wiki (*Spell*): "Spell Level: sets the skill rank necessary to cast the spell. Normally spell rank is determined by" those settings.

### What a cast spends

`0x5fc890` (the spender): Fatigue = `fFatigueCastBase + fFatigueCastMult × the Magicka cost` (`0x548da0`; executable defaults 15 and 0, `Oblivion.esm` base 1.0,
so **1**), applied as damage to Fatigue clamped at 0, and Magicka as damage `−cost`. `StartCast` `0x5f3e00` picks animation groups.

The stock PC `Oblivion - Meshes.bsa` supplies the following animation times in seconds. These are the `NiTextKeyExtraData` markers in
`meshes\characters\_1stperson` and `meshes\characters\_male`, with `start` at 0 in all 24 clips. The [Construction Set animation reference](https://cs.uesp.net/wiki/Animation_Tab)
identifies `Hit` as the frame at which the spell takes effect.

| Group | `Hit` | First-person `end` | Third-person `end` |
|---|---:|---:|---:|
| `castself` | 0.366667 | 1.300000 | 1.400000 |
| `casttouch` | 0.266667 | 0.866667 | 0.966667 |
| `casttarget` | 0.400000 | 0.900000 | 0.900000 |

The `onehand` and `alt` variants have the same `Hit` times and use the first-person `end` values in both views.

#### Live cast repeat timing (#7082)

In the original Steam PC build 1.2.0416, the test player stood in first person, empty-handed and without blocking, in `TestingHall`. The cast key was pulsed
every 75 ms for seven seconds. A 60 fps HUD capture measured the time from one Magicka deduction to the next. A Target trial's original-game debug HUD
reported 100 fps. Ranges include input and capture quantization.

| Range | Spell | Observed time between deductions |
|---|---|---:|
| Self | Heal Minor Wounds (`00000136`), two trials | 2.02–2.12 s |
| Touch | Cold Touch (`000A97EF`), aimed into empty space | 1.42–1.43 s |
| Target | Flare (`000A97DF`) and Fire Ball (`000A97E7`) | 1.33–1.37 s |

These measurements describe an unblocked cast in this setup. ByroRedux continues to use the provisional 0.45 s interval until player cast animation is wired.

### The experience (the skill use a cast gives)

The earlier census of `0x668c30`'s call sites (19, `.claude/issues/7049/`) missed three: the encoding is `add reg, 0x39c` then `call [reg]`, not a load.
They are `0x6994cc` (in the area routine `0x6992f0`), `0x69b9ea` (the end of the release routine `0x69af30`) and `0x69bc51` (in the hit routine `0x69ba90`). Each
pushes skill = `0x415360(item)`, the primary school's actor value, **usage 1** (the `SKIL` second use value) and multiplier 0, so the award is the school's use
value unscaled; `0x668c30` gives nothing when the base skill is already 100.

- **Item type gate:** only type 0 (Spell) and 5 (Poison). Powers, abilities, diseases, scrolls, enchantments, potions and ingredients award nothing here.
- **Self and Touch spells** (`0x69af30`, award at `0x69b9a4`): once, when at least one effect applied (flag at `[esp+0x14]`), a caster exists and the spell has **no
  Target-range effect** (`0x414fe0`; such a spell launches a projectile and skips this award). A Touch cast that finds no actor applies nothing: no award.
- **Target-range spells** (`0x69ba90`): the award is at impact, only for a range-2 effect, never for Telekinesis, only when the target accepts the effect, once per call;
  Poison awards per target.
- **Area**: a Fire Ball that hit one target directly awarded 1.2 Destruction XP. The same spell awarded 1.2 XP when two other actors took splash damage.
  The [original-game trial below](#area-experience-trial-7082) supersedes the per-target hypothesis from the first investigation.
- **Primary school** (`0x415360` → `0x4152c0`): the effect with the highest caster-less floored cost, ties to the first; its school is the `SCIT` school
  (`EffectItem+0x18`) when it has one, else the MGEF's (`+0x64`); schools 0 to 5 map to actor values `0x14` to `0x19` (`0x41b770`). The CS wiki says the same:
  "The school is always the same as that of the single most expensive effect."

#### Area experience trial (#7082)

In the original Steam PC build 1.2.0416, the player used the cast key in `TestingHall` with AI processing paused to hold the targets in position.
Fire Ball (`000A97E7`, 30 Fire Damage on Target in 10 feet) directly hit a test guard (`00027FCD`). The player stood 220 BU away, with
Destruction at 60 to meet the spell's mastery requirement. In the second trial, two sheep (`000151DD`) stood 100 BU to either side of the guard at the same depth.
Both sheep died in the blast. The [player ACHR save record](https://en.uesp.net/wiki/Oblivion_Mod:Save_File_Format/Player_Data) stores the exact
Destruction XP in `expPoints[10]`. No skill rank changed during the trials.

| Cast | Destruction XP before | After | Gain |
|---|---:|---:|---:|
| Fire Ball, direct hit only | 0.0 | 1.2 | 1.2 |
| Fire Ball, direct hit and two splash victims | 1.2 | 2.4 | 1.2 |
| Capstone Destruction (`00017A9E`), wall hit and one sheep damaged from 13 to 12 Health | 0.0 | 0.0 | 0.0 |

The Fire Ball trials establish one award for a successful direct hit regardless of the splash victim count in this setup. The wall-impact control
used a different, zero-cost test spell. It establishes that particular splash-only result.

### Reach and no target

ByroRedux casts a camera ray up to the load order's `iActivatePickLength`, with the original PC default of 150 BU. The first solid collider must
belong to a live actor. The ray measures distance to the collider surface, so an actor reference can be farther away than the configured ray
length. The rule comes from the original-game trials below (#7082).
With no target, a spell with the SPIT flag `0x80` (*Touch Spell Explodes w/ no Target*) and an area above 0 explodes at the caster's aim position;
without the flag nothing is applied. What Touch does to a door, a chest or a corpse: UESP *Magic Overview* documents that an effect lands only on a target it can affect (a
fireball on a chest, an Open on an NPC do nothing and feed nothing); Open acts on a locked door or container and Reanimate on an NPC's corpse (UESP *Open*, *Reanimate*), and the
engine builds neither, so it lands on live actors only.

#### Touch distance trial (#7082)

In `TestingHall` in the original Steam PC build 1.2.0416, the player aimed Cold Touch (`000A97EF`, 15 Frost Damage, zero area) straight at a
stationary scale-one test guard (`00027FCD`) on the same floor with AI processing off. Moving only the guard's forward coordinate varied the
reference-to-reference distance in `GetDistance`. The guard lost 15 Health at 176, 180, 182 and 183 BU. Its Health stayed constant at 184 BU and
beyond. This brackets the cutoff for this target and aim. The fixture measures the actor reference distance, which includes the target's geometry
between its reference position and the ray hit.

The stock `iActivatePickLength` was 150. At 75 the spell missed at 128 BU. At 225 it hit at 224 BU, which missed at the stock setting. Doubling both
the caster and target scales at stock settings still missed at 260 BU and hit at 120 BU.

The console reports stock `fCombatDistance` 128, `fHandReachMult` 0.6 and `fMagicDefaultTouchDistance` 256. Lowering
`fMagicDefaultTouchDistance` to 32 still allowed the 160 BU cast. Raising it to 512, or raising `fCombatDistance` to 256 separately, still missed
at 224 BU. Lowering `fHandReachMult` to 0.1 still allowed the 128 BU cast. These overrides did not reproduce the observed limit.

### Armor casting penalty (sourced 2026-11-20, not built)

CS wiki *Casting penalty* (a community derivation, credited to a Bethsoft forum post) and the settings pages *fMagicArmorPenaltyMin* 0.05 / *fMagicArmorPenaltyMax* 0.3: for each armor
type (light, heavy) the worn pieces add their coverage (head 10, hair 10, upper body 25, lower body 15, hand 10, foot 10, shield 30, the `iArmorDamage*Chance` settings; capped
at 100; a shield counts only with a one-handed blade or blunt in hand), scaled by `((50 - armor skill) * 2) / 100` (0 at Journeyman 50 and above); the two types' results are added
and "used to interpolate between `fMagicArmorPenaltyMin` and `fMagicArmorPenaltyMax`"; in some cases scaled again by `(current Fatigue / max Fatigue) * fMagicFatigueDrainMult +
fMagicFatigueDrainBase` (`Oblivion.esm`: 0 and 1, so no effect). UESP *Magic Overview* says armor lowers the magnitude, rounded down. Two ambiguities remain: whether the
interpolation parameter is the sum over 100, and which cases take the Fatigue scale. PSC-NG-006 keeps it out of Wave 1 (Q3).

### The projectile of a Target spell

ByroRedux flies Target effects as balls at `fMagicProjectileBaseSpeed` × `MGEF.DATA`'s speed multiplier (float at offset 28), up to
`fMagicProjectileMaxDistance`. The [Construction Set setting](https://cs.uesp.net/wiki/FMagicProjectileBaseSpeed) documents 1000 BU/s for the ball form
and [its distance setting](https://cs.uesp.net/wiki/FMagicProjectileMaxDistance) documents 10000 BU.

The [xEdit TES4 schema](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsTES4.pas) defines the projectile form in
`MGEF.DATA` flag bits 25 (Spray) and 26 (Bolt). Their combinations are neither for Ball, bit 25 for Spray, bit 26 for Bolt, and both for Fog.
Of the 72 Target-capable effects in the installed `Oblivion.esm`, 70 are Ball, `FRDG` (Frost Damage) is Fog, and `SHDG` (Shock Damage) is Bolt.
The corresponding non-ball distance settings are `fMagicSprayMaximumDistance`, `fMagicBoltMaximumDistance` and `fMagicFogMaximumDistance`.
The installed master authors the Fog maximum at 2000 BU. Separate flight behavior is outside the current player-cast scope (#7082).

### Casting noise trial (#7082)

In the original Steam PC build 1.2.0416, a test guard stood about 240 BU behind the stationary player in `TestingHall`. The guard faced away, and
[`GetLOS`](https://cs.uesp.net/wiki/GetLOS) reported no line of sight. The guard's [`GetDetected player`](https://cs.uesp.net/wiki/GetDetected) console output
gave a stationary baseline of −31. Repeated queries around the start, release and end of Heal Minor Wounds returned −31 at the stock
`fSpellCastingDetectionMod` of 20 and at the forced values 0 and 10000. Cold Touch aimed into empty space and Flare aimed at scenery also returned −31
at 10000. The console confirmed the setting override. Moving for 0.22 s as a positive control raised the score to −12 or −13 while the guard still lacked
line of sight. The trials support leaving stationary casting out of the detection noise feed. A spell impact on another actor was outside this trial.

Also: a spell with the PC-start flag (`0x04`) is added to the player when their skill reaches `iPCStartSpellSkillLevel` (25 in `Oblivion.esm`, `0x6646a5`).

## Acrobatics — jump height (clean, uncapped) + fall damage (buggy, near-useless in original)

Source: UESP *Oblivion:Acrobatics*, 2026-07-04, "Skill Benefits" section:
```
JumpHeight = 64 + AcrobaticsSkill
FinalDamage = BaseDamage × (1.25 − AcrobaticsSkill / 10000)
```
`JumpHeight` is clean affine (bias 64, coeff 1) and — notably — **not capped
at Skill 100**: Acrobatics is one of only 3 Oblivion skills (with Athletics
and Speechcraft) that keeps paying off when fortified past 100, up to an
internal cap of Skill 255. Luck has no bearing on this formula, also called
out explicitly by the source (a rare "we checked and it's NOT an input"
confirmation worth keeping as a negative data point).

`FinalDamage` is UESP-documented as **bugged**: at Skill 100 it only
reduces fall damage by ~1%, because the fall-damage-scaling GMST
(`fJumpFallSkillMult`) was tuned wrong at ship (a value of `-1.0` would have
let Skill 125+ negate fall damage entirely; the shipped value doesn't).
Notable: *Oblivion Remastered* is documented as fixing this — at Skill 100
it reduces fall damage by ~62.5%, i.e. the remaster patched original-game
mechanical math, not just visuals/engine — a rare instance worth flagging
since most remaster deltas encountered this session have been purely
cosmetic (Mastery Perk re-shuffling elsewhere in this same page is a
different, non-buggy Remastered rebalance, not a fix).

Both formulas govern jump physics / fall-damage mitigation, not an
ActorValue output — same "skill AV in, mechanic-formula out" routing as
Lockpicking/Sneak Detection in `charal-skyrim-ruleset.md`.

### Fall damage — BUILT (#7056)

Fall damage is now a rule row, `FallModel::OBLIVION` (`crates/core/src/character/fall.rs`, the profile's `fall`
row), read from the load order by `FallModel::with_gmst`. Its settings and their sources (CS wiki `fJumpFall*`
setting pages; UESP *Oblivion:Acrobatics*):

| Setting | Default | Meaning |
|---|---|---|
| `fJumpFallTimeMult` | 80 | damage per second of falling (`Oblivion.esm` authors 80) |
| `fJumpFallTimeMin` | 0.2 | no damage at all below this many seconds of falling |
| `fJumpFallTimeBase` | 0 | the floor of the base damage once it applies |
| `fJumpFallSkillBase` | 1.25 | the skill factor at Acrobatics 0 |
| `fJumpFallSkillMult` | −0.01 | per point of Acrobatics, over 100: `1.25 − Acrobatics / 10000` |

```text
base   = max(fJumpFallTimeBase, fall seconds * fJumpFallTimeMult)         (0 below fJumpFallTimeMin)
damage = base * (fJumpFallSkillBase + Acrobatics * fJumpFallSkillMult / 100)
```

The skill is not capped at 100, and the result is never negative. **Declared gap:** what counts as falling (the CS
wiki's *fJumpFallTimeBase* says a vertical threshold must be passed, with no setting named); `FallTracker` counts
the time spent below the take-off height. The mounted-rider term (`fJumpFallRiderMult`) is out: no mounts.

## Block — BUILT (#7054)

`BlockModel::OBLIVION` (`crates/core/src/character/block.rs`, the profile's `block` row). UESP *Oblivion:Block*,
"Blocked Melee Damage" and "Notes": a shield blocks 1% of the damage per level of Block, capped at 75%; a blade,
blunt weapon or bow in hand blocks 0.5% per level; bare hands block 0.25% per level, "but only against unarmed
opponents. NPC's equipped with weapons will always deal full damage"; "skill levels over 100 will not increase the
blocked damage any further"; and the reduction is "the same for melee and ranged attacks".

```text
shield  reduction = min(0.75, 0.01   * min(Block, 100))
weapon  reduction =           0.005  * min(Block, 100)
hands   reduction =           0.0025 * min(Block, 100)      (an unarmed attacker only; 0 against a weapon)
```

The shield is the worn item in biped slot 13 (UESP *Biped Object Flags*, `0x2000` = Shield). **Not modelled:** the
Novice's Fatigue drain, wear on the weapon or shield, the higher ranks' recoil, knockback, stagger and disarm, and
whether a block needs the defender to face the attacker (the page does not say).

## Archery (bow) — BUILT (#7055)

`BowModel::OBLIVION` (`crates/core/src/character/bow.rs`, the profile's `bow` row), read from the load order by
`BowModel::with_gmst`. CS wiki *fArrowSpeedMult*, *fArrowBowTimerBase*, *fArrowBowTimerMult*, *fArrowWeakSpeed*;
`Oblivion.esm` authors none of the four:

```text
draw fraction = min(1, fArrowBowTimerBase (0.25) + fArrowBowTimerMult (0.4) * draw seconds)     (full at 1.875 s)
arrow speed   = ammo speed * fArrowSpeedMult (1500) * (fArrowWeakSpeed (0.01) + 0.99 * draw fraction)
damage        = (bow + arrow) * weapon rating (Agility, Marksman, Luck) * Fatigue factor
```

The damage is the weapon-damage rule of *The Complete Damage Formula* below, its settings held by
`OblivionDamageSettings` (#7199), at full condition (bow wear is BOW-NG-007, an arrow has no health). **Declared
gap:** Oblivion's arrow range, which no source gives; the host supplies a labelled provisional constant.

## Melee weapon damage (Blade/Blunt) — BUILT — first real bilinear (attribute × skill) multiplier, shared across weapon-type skills

Source: UESP *Oblivion:Blunt* + *Oblivion:Blade*, 2026-07-04, both "Skill Benefits":

```
Damage = BaseWeaponDamage × 0.5 × (0.75 + Strength × 0.005) × (0.2 + WeaponSkill × 0.015)
```

**Byte-for-byte identical formula and constants on both pages** — Blade's page
gives the exact same `0.75`/`0.005`/`0.2`/`0.015` with `BladeSkill` swapped in
for `BluntSkill`, confirming this is a **shared weapon-type-skill damage
shape**, not a Blunt-specific one (governing skill is the only per-weapon-type
variable — `WeaponSkill` above stands for whichever of Blade/Blunt governs the
equipped weapon). Cross-check target for the remaining two weapon skills
(Marksman, Hand-to-Hand) still open — Hand-to-Hand in particular is a likely
divergence point, since FO3/FNV's Unarmed Damage is skill-chained rather than
a weapon-damage multiplier (see below).

Two multiplicative affine factors — a Strength-driven factor and a Blunt-
skill-driven factor — expand cleanly into `DerivedStatFormula::bilinear`'s
`bias + cₐ·A + c_b·B + cross·A·B` shape once the `0.5` and `BaseWeaponDamage`
scalars are pulled out (the per-weapon scalar stays a combat-layer input, same
boundary as FO3/FNV's DR/DT — CHARAL owns the STR/skill-driven *multiplier*,
not the per-weapon base):

```
Multiplier = 0.5 × (0.75 + 0.005·STR) × (0.2 + 0.015·Blunt)
           = 0.075 + 0.0005·STR + 0.005625·Blunt + 0.0000375·STR·Blunt
```

(bias `0.075`, `c_STR = 0.0005`, `c_Blunt = 0.005625`, `cross = 0.0000375` —
the exact `DerivedStatFormula::bilinear` argument order once a `Multiplier`
scope is wired for two-skill-input formulas.)

This is the **first confirmed real-data instance of a bilinear formula
crossing an attribute with a *weapon-type skill*** — every bilinear formula
found so far (FO4/FO3/FNV Health) crossed an attribute with **Level**, not
with another governing stat. It's also the first weapon-damage formula found
that reads BOTH inputs multiplicatively (FO3/FNV Melee Damage is `STR×0.5`
additive, single-input; FO4's is `×(1+STR/10)`, single-input multiplier) —
Oblivion's is the first to need genuine `DerivedScope::Multiplier` +
two-input `bilinear` at once — though **not** via `DerivedStatFormula` itself
in the end (see below).

**BUILT 2026-07-04** as `oblivion_weapon_damage_multiplier` in the new
`crates/core/src/combat.rs` module (sibling of `crate::stealth`, not under
`character/`) rather than as a `DerivedStatFormula` row: the formula needs
the Luck-chained `modified_skill()` (below) folded in first, and
`DerivedStatFormula`'s `DerivedInput` reads a raw stored AV, not a computed
intermediate — so this is combat-time function composition, the same
"reusable piece built ahead of its consumer system" pattern already used for
`stealth::detection_score` (no attack-resolution system exists yet to wire
it into). 6 tests (bias-term cross-check against this doc's own hand-
expansion, Luck monotonicity, the `[0,100]` input clamp UESP states
explicitly). Marksman uses the identical function (Agility in place of
Strength — no separate code, same shared shape now confirmed by both source
pages and the implementation). Hand-to-Hand is genuinely divergent — see the
Complete Damage Formula section below, also now BUILT.

Both Blade and Blunt increase by a flat **+0.5 XP per hit on a living target**
(inanimate targets in training rooms grant nothing) — same constant on both
pages, reinforcing the shared-shape reading above. This is a **third**
instance of the "skill/character progresses on in-fiction action, not just
level-up allocation" family, alongside FO3/FNV's Skill Rate and FNV's
Lockpicking XP-per-success reward (`charal-fnv-fo3-ruleset.md`) — Oblivion's
variant increases the **skill itself** directly (major/minor skill-use
accumulation model, already the documented Oblivion leveling mechanism), not
a flat character-XP grant, so it's a reconfirmation of the existing skill-use
leveling model rather than a new gap.

## The Complete Damage Formula — closes Marksman/Hand-to-Hand, adds Luck-chained skill + Armor Rating — all now BUILT

Source: UESP *Oblivion:The Complete Damage Formula*, 2026-07-04 — the page
Blade/Blunt both point to for "more information." Confirms the Blade/Blunt
formula exactly (full-condition weapon: `(WeaponHealth/BaseWeaponHealth+1)/4
= (1+1)/4 = 0.5`, matching the `×0.5` constant already recorded) and **also
covers Marksman** (`Attribute` = Agility for bows, same shape, `WeaponRating`
= Bow WR + Arrow WR) — so all three ranged/melee weapon skills (Blade, Blunt,
Marksman) now confirmed to share one bilinear damage shape. Several new
pieces:

**1. `ModifiedSkill` — Luck chains into combat-time skill, not just chargen.**
`ModifiedSkill = Skill + 0.4×(Luck−50)`. This is a *second* Luck-chained
formula (after the base-skill auto-calc pattern) but a structurally different
one: it modifies the skill value used **at combat time**, not the stored
skill AV itself — Skill/Fortify/Damage/Absorb-Skill effects and Luck's own
magical modifiers all fold in first. Substituting into `WeaponRating` turns
the two-input bilinear into an effectively **three-governing-stat** formula
(Attribute, Skill, Luck) — but this does NOT need a new `DerivedStatFormula`
shape: `ModifiedSkill` is itself a plain two-input **affine** formula
(`bias=−20, c_Skill=1, c_Luck=0.4`, no cross term) that **chains** into the
existing bilinear, exactly the same "derivation chains" pattern already
established for FO3/FNV's Unarmed Damage (← Unarmed skill ← SPECIAL) — just
one hop longer. **BUILT** as `combat::modified_skill(skill, luck)` — a plain
function rather than a `DerivedStatFormula` row, since it's evaluated at
combat/haggling time against transient skill values, not stored `ActorValues`
(see the module docs in `crates/core/src/combat.rs`).

**2. Hand-to-Hand confirmed divergent, as predicted** — and simpler:
```
Health_Damage = 1 + 10.5 × (Strength/100) × (ModifiedSkill/100)
Fatigue_Damage = 1 + 0.5 × Health_Damage
```
A **pure cross-term bilinear** (`bias=1, c_Strength=0, c_Skill=0, cross=0.00105`,
using the same Luck-adjusted `ModifiedSkill` as above) — no separate additive
STR or Skill term at all, unlike Blade/Blunt/Marksman's three-term shape.
`Fatigue_Damage` chains a THIRD time off `Health_Damage` (itself already a
2-hop chain) — the deepest confirmed derivation chain found yet (Luck+Skill →
ModifiedSkill → Health_Damage → Fatigue_Damage, 3 hops). Both still route as
combat-system math (damage output, not a stored AV), same boundary as every
other weapon-damage formula here. **BUILT** as
`combat::oblivion_hand_to_hand_damage(strength, skill, luck) -> (health,
fatigue)`, alongside the weapon-damage multiplier above (3 tests: the
worked-zero and worked-max cases, plus a chain-invariant test asserting
`fatigue == 1 + 0.5·health` for arbitrary inputs). Unlike the weapon-damage
multiplier, the `[0,100]` input clamp is **not** applied here — UESP states
that clamp only in the Weapon Damage section, not Hand-to-Hand, so the
implementation doesn't assume it (no-guessing).

**3. An Armor-skill-governed Armor Rating candidate — directly comparable
to Skyrim's built `LIGHT_ARMOR_RATING_COEFF`, now built the same way.** Per-piece:
```
PieceArmorRating = BaseArmorRating × (0.35 + 0.0065 × ArmorSkill) × (ArmorHealth / MaxArmorHealth)
CombinedArmorRating = Σ pieces, capped at 85
DamageReduction = (100 − CombinedArmorRating) / 100     (1 if attacker sneak-undetected + Master Sneak perk)
```
`ArmorSkill` is whichever of Light Armor / Heavy Armor governs the piece —
same "the wearer's own armor-type skill scales their own defense" shape as
Skyrim's Light Armor Rating multiplier (`charal-skyrim-ruleset.md`), making
this a **cross-game-confirmed pattern**: both TES games have an
armor-skill-driven `DerivedStatFormula::affine` term feeding armor rating,
just Oblivion's additionally multiplies by armor condition (equipment-layer
input, same boundary as weapon condition above) and sums per-piece before the
85-point cap (the cap and per-piece sum stay a combat-layer concern, not
part of this formula). **BUILT 2026-07-04** — unlike the weapon-damage
formulas above, this one *does* fit `DerivedStatFormula` cleanly (single-
input affine, no Luck term), and it was first built as two `Multiplier` rows in
`oblivion_ruleset()` keyed on `LightArmorRating` / `HeavyArmorRating`. Those rows
were removed in #7201: the engine-supplied 72-name roster has no such actor value,
so they could never register, and they restated the coefficients. The one home of
`0.35 + 0.0065 x ArmorSkill` is `ArmorRatingModel::OBLIVION` (below), which
`piece_rating` applies per piece (Light 50 → 0.675, Heavy 20 → 0.48).

**Wired 2026-11-20 (armor rating and Resist Normal Weapons).** The per-piece rating above is applied to weapon hits: the pieces a target
wears add up at its Light or Heavy Armor, cap at `fMaxArmorRating` (85), and `(100 - combined) / 100` scales the hit (1 for a Master
Sneak attack); `(100 - ResistNormalWeapons) / 100` scales it too, 1 for a silver, daedric or enchanted weapon or arrow. `ArmorRatingModel`
(`crates/core/src/character/armor_rating.rs`, the profile's `armor_rating` row, Oblivion only) holds the numbers and
`combat::weapon_defense` applies them to an actor's hit in `combat_damage_system`, melee and arrows alike. Verified live: `oblivion-armor.sh` (the iron cuirass stops 3.8% of a patron's strike at Heavy Armor 5) and `oblivion-ghost.sh` (Perennia's ghost
carries `ResistNormalWeapons` 100 from her ethereal-undead ability: an iron longsword reads `resisted 100%` and leaves her Health, a silver
longsword takes her from 18 to -10). Not modelled: armor condition (read as full) and Shield-like boosting effects.

**Weakness, creature attacks and fists (#7195).** A weakness is a negative resistance: UESP *Oblivion:Magical Effects* ("Resistances and
Weaknesses": "Total_Resistance_to_X = Resist_X - Weakness_to_X") and *Oblivion:Weakness to Normal Weapons* ("50% Weakness ... a weapon
normally dealing 20 points of damage, your health would go down by 30"), so `weapon_resistance_factor` is `max(0, 100 - R) / 100` and rises
above 1 for a negative `ResistNormalWeapons`. Two unweaponed blows are outside the resistance: a creature's natural attack (UESP
*Oblivion:Resist Normal Weapons*: "creature attacks, however, are unaffected by resist normal weapons") and a fist at Journeyman Hand to Hand
(the same page: fists are normal weapons "only if below level 50 in Hand to Hand"; UESP *Oblivion:Hand to Hand*, Journeyman: "the ability to
damage creatures that are immune to normal weapons"; the Weakness page agrees). The tier is `iSkillJourneymanMin` (50), read from the load
order. **Declared gap:** a creature that wields a weapon (a goblin's or a skeleton's) strikes with that weapon here, so its blow is resisted
unless the weapon ignores the resistance; the sources speak of "creature attacks" without saying whether an armed creature's blow is one.

**4. Sneak/power-attack multipliers, weapon condition, spell resistance** —
all confirmed real but out of CHARAL scope, same "skill AV in, combat-formula
out" routing as Sneak Detection/Lockpicking elsewhere in this doc:
SneakMultiplier is a **step function** on Sneak skill (0–24 → 4× one-handed/
2× bows; 25+ → 6×/3×, flat — does not scale further past the single
threshold, unlike the pickpocket/detection formulas' continuous curves) that
only applies while undetected-sneaking, never stacking with
PowerAttackMultiplier (2.5×, or 3× for an Apprentice-plus Standing power
attack — cross-confirms the Blade/Blunt mastery-perk tables verbatim, no new
numbers). Weapon condition contributes `(0.5 + WeaponHealth/BaseWeaponHealth
× 0.5)` — equipment-system data, same boundary as FNV's `ItemValue`
condition-decay finding. Spell damage magnitude is scaled by
`(100−MagicResistance+MagicWeakness)/100` (elemental effects get an
additional, separate resistance/weakness ratio) — MagicResistance/Weakness
are AVs CHARAL could own the *slot* for, but the consuming formula is the
spell-system's, not a derived character stat.

**5. GMST names now captured for the whole weapon-damage pipeline** (AUTHORED,
not hardcoded, per the WATAL/CHARAL split): `fDamageWeaponMult=0.5`,
`fDamageStrengthBase=0.75`/`fDamageStrengthMult=0.5`,
`fDamageSkillBase=0.2`/`fDamageSkillMult=1.5`,
`fDamageWeaponConditionBase/Mult=0.5/0.5`; Hand-to-Hand's own family
`fHandHealthMin=1`/`fHandHealthMax=15`/`fHandDamageStrengthMult=0.75`/
`fHandDamageSkillMult=1`/`fHandFatigueDamageMult=0.5`; sneak/power-attack
GMSTs (`fPerkSneakAttackMelee*Mult`, `fDamagePowerAttack*Bonus`); armor
(`fMaxArmorRating=85`). All future formula rows built for Oblivion should read
these by name (CHARAL §8 item 6), not re-hardcode the numeric constants captured here.
**Read since #7199:** `OblivionDamageSettings` (`crates/core/src/combat.rs`) holds the weapon family, the
condition pair, the whole Hand-to-Hand family (`fHandHealthMin`/`Max`, `fHandDamageStrengthBase`/`Mult`,
`fHandDamageSkillBase`/`Mult`, `fHandFatigueDamageBase`/`Mult`), the Luck pair of the effective skill
(`fActorLuckSkillMult` 0.4 / `iActorLuckSkillBase` -20, CS wiki *Effective Skill Level*, which the spell-cast row
takes its defaults from) and the Fatigue pair (`fFatigueBase` 1.0 / `fFatigueMult` 0.5, CS wiki *Fatigue Game
Settings*), each overlaid from the load order by `OblivionDamageSettings::with_gmst`; the bow's Oblivion damage
rule carries it (`BowModel::with_gmst`). `fMaxArmorRating` is read by `ArmorRatingModel::with_gmst`.

## Fatigue — base pool cross-checked (already BUILT), regen formula now BUILT too, Remastered divergence recorded

Source: UESP *Oblivion:Fatigue*, 2026-07-04. The base pool formula —
`Fatigue = Strength + Willpower + Agility + Endurance` — is **already BUILT**
(`crates/core/src/character/tes.rs::oblivion_fatigue_formulas`, landed
2026-07-01, `2b9147ae`), as four affine rows summed under one output id
(the four-attribute sum exceeds `DerivedStatFormula`'s two-input shape, so
it's expressed as 4 rows rather than 1 — already documented in that module's
own doc comment). This fetch is a **direct cross-check against the shipped
code**: the wiki's plain sum matches exactly, no discrepancy. Two new items
this page adds beyond what's built:

**1. FatigueRegen — a new formula-shape gap, not a derived-stat gap.**
```
FatigueRegen = Endurance × fFatigueReturnMult(0.0) + fFatigueReturnBase(10.0)
             = flat 10/sec in vanilla (the Endurance term ships at coefficient 0)
```
Notable in the same way Oblivion's Enchant skill having zero functional role
was notable: the GMST *exists* (Endurance could scale regen) but ships
authored at `0.0` — another "the hook is there, the coefficient is zero"
case. Architecturally this is **not** a `DerivedStatFormula` row at all — it
is a per-second *rate*, consumed by a tick system (mirroring the affliction
mechanism's `affliction_tick_system`, not the on-demand `derived_value`
model).

**BUILT 2026-07-04** — the new `crates/core/src/character/regen.rs` module
(the first CHARAL system needing a **fixed 60 Hz tick** decoupled from the
variable frame rate, since regen rates are stated "per real-time second," not
per-frame): `PoolRegenAccumulator` (a `Resource`, one global fixed-step clock
mirroring `crates/physics::PhysicsWorld`'s own accumulator — the only other
fixed-timestep precedent in the engine) drains a variable `frame_dt` into
`POOL_REGEN_DT = 1/60` increments, capped at `MAX_REGEN_SUBSTEPS` (8) so a
hitch/load stall can't dump an unbounded batch of regen into one frame; the
constant `FATIGUE_REGEN_PER_SEC = 10.0` is applied via `ActorValues::restore`
(which already floors damage at `0.0`, so no separate max-pool clamp is
needed). `pool_regen_tick_system(world, frame_dt)` is registered in
`Stage::Update` via `add_exclusive_with_access`
(`byroredux/src/boot/schedule/update/` — the registration moved out of
`main.rs` under #3170's boot split; #4109 swept the stale `main.rs`/
`boot/` references). **Live on Oblivion since #7010:** `CharacterRulesProfile::build_tes_pool_regen`
installs `PoolRegenConfig` with the Fatigue, Magicka, Willpower and `StuntedMagicka` records the
engine-supplied roster resolves, on the first cell load (`cell_loader/references/mod.rs`), and since
#7197 its rates are the load order's four `f*Return*` settings (`TesPoolRegen`, the Endurance term
included) rather than the constants named above, which remain the defaults. FO3/FNV install the
config for their Action Points lane only (#6350). 5
new tests (formula match, 60 Hz tick counting, fractional-time carry-over
across frames, substep cap, and a full ECS integration test proving both
Fatigue and Magicka regen end-to-end).

**2. Oblivion Remastered ships a wholesale-different Fatigue model — a
*second* confirmed "remaster patches original mechanical math" instance**
(after Acrobatics fall-damage, above):
```
Fatigue      = (Agility×0.3333 + Willpower×0.6666) × 4.0    (drops STR + END entirely)
FatigueRegen = (Agility × 8.0/100) + 12.0, ×1.5 out of combat, 2s post-action delay
Melee damage is no longer modified by Fatigue at all in Remastered.
```
A genuine ruleset fork, not a visual-only remaster delta: different governing
attributes (2 of 4, reweighted 1:2 Agility:Willpower), a fundamentally
different regen shape (rate now scales with Agility, not flat), and the
Fatigue→damage coupling deleted outright. **The "which patch level is the
compat target" question raised here and at Acrobatics is already answered
in code**, not actually open: `oblivion_health_gain_per_level`'s doc comment
(`crates/core/src/character/tes.rs`) explicitly states classic Oblivion
(2006, Gamebryo — ByroRedux's target) is in scope and Remastered's diverging
formulas are not adopted; `crates/core/src/combat.rs` states the same. Every
Remastered variant recorded in this doc (Fatigue/Health/Magicka regen,
Acrobatics fall damage) is captured for completeness and cross-game context,
not as a live fork decision.

**3. Confirms the fatigue/damage ratio is genuinely uncapped** (refines the
Complete Damage Formula entry above, doesn't contradict it): "if your current
Fatigue is higher than your normal maximum... you'll do more damage" — the
`(Fatigue/MaxFatigue+1)/2` term is explicitly *not* clamped to 1, so
Fortify Fatigue effects pushing current above max keep paying off past the
nominal ceiling. A real, sourced confirmation of the earlier "combat-system
consumer" formula's edge behavior, not a new formula.

## Pool scope (Health / Magicka / Fatigue) — player-only is UNsourced (#6431)

None of the Fatigue, Health or Magicka captures states whether the formula
applies to NPCs or only to the player. The three base-pool rows in
`crates/core/src/character/tes.rs` are `.player_only()` as an **analogy to the
Fallout convention** (NPCs ship baked pool values) — and that convention is itself
FO4-evidence-specific (FO4 NPCs read a baked `DNAM`). It is a conservative choice,
not a captured fact: the sibling Armor Rating rows above are `ActorGeneral` because
the source is silent on a player/NPC split there (those rows were removed in #7201). The
choice is observable since #7010 wired the ruleset: the rows reach the live player, and an
NPC's pools come from the population model, not from them. Pinned by
`oblivion_pool_rows_are_player_only_pending_a_source`. Resolve by finding a
citation (or an in-engine NPC pool check) and update this note and the code together.

## Health — both built formulas cross-checked; vanilla has ZERO passive regen; Remastered's is a 3-input cross-term formula

Source: UESP *Oblivion:Health*, 2026-07-04. Both already-BUILT pieces
cross-check exactly: base pool `2×Endurance`
(`oblivion_health_formula`) and per-level accrual `0.1×Endurance` rounded down
by the caller (`oblivion_health_gain_per_level`, landed alongside Fatigue on
2026-07-01) — the page's own worked table (Endurance 98 at level-up → +9,
`floor(9.8)`) matches the shipped `0.1×endurance` formula bit for bit. No
code changes; this is a pure verification pass.

**Vanilla Oblivion Health has NO passive regeneration at all** — a genuine
cross-stat contrast worth keeping straight: the page's regen paragraph
("passively regenerates every second... 6 second delay") is explicitly
`{{OBR}}`-tagged (Oblivion-Remastered-only), and the "Original Oblivion"
section never mentions passive regen — health only changes via the per-level
accrual above, explicit Restore/Absorb Health effects, resting, or fast
travel. This differs from vanilla **Fatigue**, which *does* regen passively
(flat 10/sec, `charal-oblivion-ruleset.md` above) — so "each pool has its own
regen policy" is now a confirmed fact, not an assumption; don't generalize
one pool's regen behavior to the others without a citation.

**Oblivion Remastered's Health formula is a new 3-input, cross-term shape**:
```
Health = Strength×0.3333×2 + Endurance×0.6666×2 + Endurance×0.1×(Level−1)
       = 0.6666·STR + 1.2332·END + 0.1·END·Level
```
Unlike vanilla (level-independent base + separate per-level accrual side
mechanic), Remastered folds level directly into one formula that "always uses
current attributes... no longer a permanent penalty to not leveling
Endurance." This is a 3-input formula (STR, END, Level) with a cross term
only between two of the three (END×Level) — it does **not** fit a single
`DerivedStatFormula::bilinear` row (which holds exactly 2 inputs + 1 cross),
but decomposes cleanly into **2 summed rows**, the same "N-row-sum under one
output id" pattern already used for vanilla Fatigue's 4-row sum — just with
one row being `affine(STR, 0.6666)` and the other `bilinear(END, LEVEL,
c_END=1.2332, cross=0.1)` instead of 2 (or 4) plain affine rows. Confirms the
row-sum pattern generalizes to mixed affine+bilinear rows, not just uniform
affine ones. Remastered `HealthRegen = (END×0.34/100) + 0.16`, ×7.5 out of
combat — same "new per-second rate" bucket as Remastered FatigueRegen, but
Remastered itself is out of scope (see the compat-target note at Fatigue
above), so neither this nor vanilla Health (which has no regen at all) has
anything to wire into the now-BUILT `regen.rs` tick system.

**Also surfaced, not investigated further**: Oblivion has its **own native
Fame/Infamy** stats (this page: too much Infamy locks out certain Wayshrine/
altar healing) — distinct from FNV's same-named Fame/Infamy reputation system
already built in `charal-fnv-fo3-ruleset.md`. No formula or mechanism captured
yet (this page only uses it as a gating example); flagged as an open research
thread for a future fetch (*Oblivion:Fame* / *Oblivion:Infamy*), not assumed
to share FNV's 2-axis monotonic model just because the names match — Karma
vs. FNV-Reputation earlier in this same investigation is the standing
reminder that same-named stats can have different shapes across games.
Equipment health/durability (percentage-of-max display, degrades on hit,
repaired via Armorer skill) reconfirms the already-established "equipment
condition is item-system data, not CHARAL" boundary — no new formula, same
routing as FO3/FNV's `ItemValue`.

## Magicka — base formula cross-checked; the third and most interesting regen policy (now BUILT); first quadratic formula found

Source: UESP *Oblivion:Magicka*, 2026-07-04. Base pool `Magicka = Intelligence
+ Intelligence×fPCBaseMagickaMult(1.0) = 2×Intelligence` matches the already-
BUILT `oblivion_magicka_formula` exactly — third and last of the three base
pools now cross-checked against shipped code, all matching (Health/Fatigue
above).

**Magicka's regen policy is the third distinct shape found across the three
pools** — completing a pattern worth stating plainly now that all three are
known: **Fatigue regens at a flat rate** (Endurance coefficient ships at
literal 0), **Health does not regen at all** in vanilla, and **Magicka
regens via a real attribute-driven formula**:
```
MagickaRegen = (Willpower × 0.02 + 0.75) × (MaxMagicka / 100)
```
Willpower-governed (an affine term) additionally **scaled by the character's
own MaxMagicka** — the first regen formula that chains through another
derived stat (`MaxMagicka = 2×Intelligence`, itself already built) rather
than reading a raw attribute alone; higher-Magicka characters regen faster in
absolute terms at the same relative rate. Confirms "don't generalize one
pool's regen behavior to the others" (flagged provisionally at the Health
entry) is now a **fully evidenced three-way split**, not a guess from one
data point.

**BUILT 2026-07-04** alongside Fatigue regen, same `crates/core/src/
character/regen.rs` module and `pool_regen_tick_system`:
`magicka_regen_per_sec(willpower, max_magicka, stunted)` reads `MaxMagicka`
by calling `CharacterRuleset::derived_value` inside the tick system (chaining
through the already-built `2×Intelligence` formula live, per-actor, per
tick) rather than requiring a separately-tracked `MaxMagicka` AV — the first
CHARAL system to read a derived value as another system's *input* at
runtime, not just at formula-definition time.

**Stunted Magicka is a binary gate on this formula**, not a formula itself:
the Atronach birthsign, the *Astral Vapors* disease (cross-references the
Disease section above — a disease *can* apply a stat-system status effect,
not just flat attribute drains as previously characterized there; worth a
mental amendment, not a doc correction, since Astral Vapors was already
listed as one of the two Oblivion diseases with escalating/special behavior),
and one Shivering Isles item all set a flag that **zeroes regen entirely**
regardless of Willpower/MaxMagicka. A status-effect boolean multiplying the
regen formula to 0, same shape as a `temporary_mod` gate elsewhere in CHARAL.
`magicka_regen_per_sec`'s `stunted` parameter makes the *formula* complete,
and the tick reads it from the `StuntedMagicka` actor value (`PoolRegenConfig::stunted_magicka_avif`):
a positive value stops Magicka regeneration and leaves Fatigue alone
(`stunted_magicka_stops_only_magicka_regeneration`).

**Oblivion Remastered's MagickaRegen is the first confirmed quadratic
formula in the whole Fallout+Oblivion corpus** (Skyrim's Enchanting
`SkillMultiplier`, noted earlier, was the only other quadratic found, and
that's a different game/system):
```
MagickaRegen = 0.0003×Willpower² + 0.015×Willpower
```
No longer scaled by MaxMagicka at all (a real mechanical simplification, not
just a rebalance — third Remastered-diverges-from-vanilla regen formula found
this session, after Fatigue and Health). Architecturally this needs **no new
`DerivedStatFormula` shape**: a single-variable quadratic is just `bilinear`
with **both inputs aliased to the same AVIF** (Willpower for both `A` and
`B`), `cross=0.0003` doing the squaring, `coeff_a=0.015` (or split across
`coeff_a`/`coeff_b` since they'd be redundant when A≡B), `coeff_b=0`. Worth
recording as a confirmed encoding trick, not a gap — the two-input struct
already covers this case for free.

Race/birthsign Magicka bonuses (Breton +50, High Elf +100; Mage +50,
Apprentice +100 w/ permanent Weakness to Magic, Atronach +150 w/ permanent
Stunted Magicka) are flat chargen-time AUTHORED bonuses, same bucket as every
other race/class bonus table found so far — not a new CHARAL formula, just
data for the `Background`/chargen population path.

## Movement — first ATTRIBUTE-driven (not skill-driven) physics formula, plus a concrete Athletics/Armor cross-check

Source: UESP *Oblivion:Movement Formulas*, 2026-07-04. Same routing as
Acrobatics/Sneak Detection/Lockpicking above — "AV in, physics-formula out,"
not a CHARAL derived stat — but worth capturing precisely since it's the
**first movement formula governed by an *attribute*** (Speed) rather than a
skill, and it independently corroborates two things noted only in passing
earlier in this doc.

```
BaseSpeed = 90 + (130−90) × Speed/100          # clean affine, Speed attribute, uncapped past 100
LandSpeed = EncumbranceMod × WeaponMod × BaseSpeed × SneakMod × MoveTypeMod
```

`BaseSpeed` is clean affine on the **Speed attribute** — Oblivion's first
movement (or indeed any) formula found so far driven by an attribute alone
with no skill or Luck term, and (like Acrobatics' JumpHeight) explicitly
**uncapped past Speed 100**, continuing to scale linearly with Fortify Speed
effects.

**Running speed cross-confirms the "Athletics scales past 100" claim with
real numbers** (Acrobatics section above flagged Athletics as one of only 3
skills — with Acrobatics and Speechcraft — that keeps paying off past Skill
100, but hadn't yet seen Athletics' own formula):
```
MoveTypeModifier(running) = 3 + 1 × Athletics/100     # up to 4× BaseSpeed at Athletics 100, uncapped beyond
```
Swimming has two more Athletics-scaled variants (walk/run) with smaller
multipliers (0.02, 0.1) — same shape, lower stakes, not reproduced in full
here.

**Armor mastery perks reach into encumbrance, extending the already-noted
weapon-mastery-perk pattern to armor skills**: Light Armor Expert zeroes
worn-Light-Armor weight from the speed penalty entirely (`0.0` multiplier);
Heavy Armor Expert halves it, Heavy Armor Master zeroes it too (UESP's own
aside: "this actually means you run *faster* than in cloth" at Master Heavy
Armor — a vanilla balance oddity, not a bug, worth keeping as flavor). Same
"skill-tier unlocks a perk" shape as Blade/Blunt's Mastery Perks, now
confirmed for Light/Heavy Armor too — armor skills gate a *physics* modifier
here, distinct from the Armor Rating *combat* formula found via the Complete
Damage Formula page.

**Two explicit negative data points**, same "worth recording what's NOT an
input" discipline as Acrobatics/Luck: (1) **Fatigue does not affect movement
speed at all** — draining Fatigue reduces damage dealt, never movement, and
the page notes there is no game-settings-only way to add a fatigue-based
movement penalty (would need per-actor scripting). (2) **Strength does not
offset encumbrance's speed penalty** — only worn weight (mediated by Armor
skill mastery perks above) matters; a strong character in heavy armor is
exactly as slowed as a weak one. Both are real, cited absences, not
oversights — useful for anyone tempted to wire Strength or Fatigue into a
future movement-speed consumer system.

Creature and Flying speed reuse the identical `Min + (Max−Min)×Speed/100`
shape with different constants (Creature 5–300, Flying 5–300) — confirms the
formula *shape* (not just the vanilla player constants) is a shared engine
mechanism, monster/flying data is content not a new formula.

## Commerce — third confirmed use of the Luck-chained `ModifiedSkill`, a Disposition reputation-family candidate, and a third mastery-tier-perk instance

Source: UESP *Oblivion:Commerce*, 2026-07-04.

**Haggling acceptance is a cross-actor formula, and the THIRD confirmed use
of `ModifiedSkill = Skill + 0.4×(Luck−50)`** (after weapon-rating and Hand-
to-Hand damage on the Complete Damage Formula page — this is now clearly a
*general-purpose* "effective skill" quantity reused across combat AND
economy systems, not a combat-specific one-off):
```
Value = 0.5×floor(0.4×(Disposition−10)/4)
      + (100 + min(PlayerModifiedSkill,100) − min(MerchantModifiedSkill,100)) / 10
      − SliderPosition × 0.55
```
Trade accepted iff `Value ≥ 0`. Reads **both actors' Mercantile skill**
(Luck-chained via `ModifiedSkill`, each capped at 100 before the difference),
the merchant's **Disposition** toward the player, and a player-chosen
`SliderPosition` (UI input, not an AV — same non-AV-input shape as FO3/FNV's
`ItemValue` in the pickpocket formula). Same "reads two actors' state at
once" shape already seen in FNV's Sneak Detection and Oblivion's own Nerve-
style companion buffs (well, Fallout's) — Bethesda repeats this cross-actor
pattern across very different systems (stealth, dialogue persuasion now
confirmed generalizes to trade too). Routed as a gameplay-system (commerce)
input, not a `derived`-table row — same boundary as every other persuasion/
barter formula found across every game so far.

**Disposition itself is a new reputation-family candidate, not yet spec'd.**
This page treats it as a stored, per-NPC, roughly 0–100+ continuous AV
(explicitly "capped at 100" for haggling purposes) that the player raises via
Speechcraft/gifts/etc. — this is presumably the **exact "older TES games'
Disposition stat"** that `charal-skyrim-ruleset.md`'s Relationship Rank
write-up already says Skyrim explicitly broke away from ("no longer visible…
cannot be affected by performing speechcraft"). That means Oblivion's own
Disposition is likely the **missing predecessor** in the reputation-family
lineage (Oblivion Disposition → Skyrim Relationship Rank), continuous where
Skyrim's replacement is a discrete 9-value rank — but this page gives no
formula for Disposition's own gain/decay/base value, only its role as an
*input* elsewhere. Flagged as an open research thread (a future *Oblivion:
Disposition* fetch), not assumed to share Skyrim's or FNV's shape.

**Skill training cost is a clean, simple, previously-uncaptured formula**:
`TrainingCost = 10 × CurrentSkillLevel` — one of the few explicitly
*non-negotiable* prices (with houses, horses, repairs, recharging, beds,
enchanting/spellmaking). A genuinely new tiny affine formula, single-skill-
input, no attribute/Luck term at all.

**Mercantile mastery is a THIRD confirmed skill-tier-unlocks-a-perk
instance** (after Blade/Blunt weapon Mastery Perks and Light/Heavy Armor's
movement-encumbrance perks): reaching **100 Mercantile** (base skill only,
Fortify doesn't count) removes the merchant markup entirely — buy and sell
both land at exactly 100% of base price. Separately, **Expert Mercantile**
unlocks a one-time "invest 500 gold" action per shop (raises that shop's
available trading gold by 500), and **Master Mercantile** adds a flat +500
gold to *every* shop automatically (stacking with player investment for
+1000 in an invested shop) — notably, this Master-tier bonus explicitly
**does** apply even via a temporary Fortify Mercantile spell pushing skill
into Expert range (an explicit exception called out on the page, contrasting
with "unlike all other Fortify Skill spells"). All perk-tier gates, gameplay-
system data (shop economy), not a CHARAL derived stat — but the third
data point confirming Oblivion's "skill mastery tier ⇒ discrete unlocked
behavior" pattern is a shared, recurring shape across weapon/armor/economy
skills alike.

## Disposition — closes the reputation-family research thread from Commerce; a genuinely new composite shape

Source: UESP *Oblivion:Disposition*, 2026-07-04 — the fetch flagged as an
open thread at the end of the Commerce section above. Confirms the
prediction: this **is** the "older TES games' Disposition stat" that
`charal-skyrim-ruleset.md`'s Relationship Rank write-up says Skyrim
explicitly broke away from — Oblivion's predecessor, now spec'd. But it's
structurally **richer** than every other reputation-family instance found so
far (Karma's 1 signed AV, FNV Reputation's 2-axis monotonic pair, FO4
Affinity's 1 signed AV) — Disposition is a **composite score summed from
~7 independent sources**, not a single ledger:

```
Disposition = NpcPersonality
            + PersonalityDifferential(PlayerPersonality, NpcPersonality)
            + RaceReaction + FactionReaction(rank) + FameBonus + InfamyEffect
            + CircumstantialMods (weapon drawn, witnessed crime)
```

**1. Base + cross-actor Personality differential — a genuinely new
attribute-driven, cross-actor formula.** Base disposition equals the NPC's
own **Personality** score (an attribute governing an *opinion* stat directly,
not through a skill — new for this investigation). Modified by the
player-vs-NPC Personality gap:
```
if PlayerPersonality ≥ NpcPersonality: differential = +floor((Player−Npc)/4)
else:                                   differential = −ceil((Npc−Player)/4)
```
(derived from the page's own worked examples: NPC 40 / Player 39 → −1
`(ceil(1/4)=1)`; Player 35 → −2 `(ceil(5/4)=2)` — asymmetric-around-equal,
same "don't assume symmetry" caution Karma's ±249/−250 band boundary already
taught). **Personality keeps paying off past 100** for this purpose ("this
benefit applies to Personality increases past 100") — a fourth confirmed
"scales past 100" Oblivion stat, joining Acrobatics/Athletics/Speechcraft
(skills) and Speed (attribute, movement) with Personality (attribute,
disposition).

**2. Race + Faction reactions — AUTHORED per-pair/per-rank tables**, same
bucket as FNV's per-faction reputation-threshold arrays: race reactions are
a race×race modifier table (not enumerated here); faction reactions scale
**per rank attained**, with per-faction rates given as examples (Thieves
Guild +3/rank, Fighters Guild +10/rank, Mages Guild +20/rank, Dark
Brotherhood +31/rank) — AUTHORED content, not an engine constant, matching
the "per-faction thresholds are authored, the *shape* is engine-supplied"
split already established for FNV Reputation.

**3. Fame/Infamy effect — introduces a brand-new NPC attribute,
`Responsibility`, gating how Infamy lands.** Fame is simple and monotonic:
`+3 disposition per 10 Fame`, capped at **+20** (reached at 67 Fame) — clean
affine, capped, same shape as every other Fame-consuming formula. **Infamy is
gated by the witnessing NPC's own `Responsibility` score** (0–100, not
previously seen in this investigation): high-Responsibility NPCs (100) lose
disposition fast (`~1 point per 2 Infamy`); low-Responsibility NPCs (25) lose
it slowly (`~1 per 16`); **very low Responsibility (10) NPCs *gain*
disposition from Infamy instead** — the sign of the Infamy term flips
depending on a *third character's* attribute, not the actor's own. Both
directions capped at ±20. This is a new formula shape for this corpus:
**a per-NPC attribute (Responsibility) that doesn't scale a formula's
magnitude, it can flip the formula's *sign***.

**4. Crime-witnessed disposition penalties — a per-crime-type flat/linear
table**: Assault −10, Trespass −20, Pickpocket −25, Murder −50, Stealing
`−0.5 × stolen gold value` (the only non-flat entry — linear in loot value,
uncapped as stated). Doubled if the crime is committed directly against the
witnessing NPC vs. witnessed against a third party. AUTHORED gameplay-event
data, same bucket as FO3/FNV's Karma point-grant table (quest/event-driven,
not a derived formula).

**5. Two genuinely PENDING items, not guessed**: the Speechcraft-minigame
disposition **ceiling** is skill-dependent (one example given: cap 79 at
Speechcraft 50) but no closed formula is stated on this page for the general
skill→cap relationship — left PENDING, same discipline as Lockpicking's
force-lock chance. Bribery cost/efficacy is explicitly deferred to a
separate CS-wiki *Bribery* page (not fetched this session) — also PENDING.

**Design implication for CHARAL's reputation-family model**: Disposition
doesn't fit the existing `{1-2 AVs + band/grid classifier}` shape at all —
it's a **running sum of independently-computed additive terms**, several of
which (the Personality differential, the Responsibility-gated Infamy flip)
are themselves small formulas, not flat constants. If Oblivion Disposition is
ever built, it likely wants a small ordered pipeline of contributor
functions summed into one AV, closer in spirit to the Fatigue/Health
"N-row-sum" pattern than to Karma's single clamped ledger — worth keeping in
mind as a structurally distinct 6th reputation-family member, not force-fit
into the existing classifier shape. Not built — no per-NPC Disposition
storage or any of these contributor formulas exist in the engine yet.
## Leveling — LOCKED

Oblivion uses skill-use leveling rather than character XP: using a skill banks experience, enough experience raises the skill by one, and
a level becomes available after 10 increases of major skills (`iLevelUpSkillCount`), applied by a night's sleep. At level-up the player
raises three attributes; each receives a bonus from the number of governed skill increases tallied for it, **counting every governed
skill's increase, not only major skills** (V13: the tally runs on every rank-up, the major count only on major ones): 0 increases give +1,
1–4 give +2, 5–7 +3, 8–9 +4, 10 or more +5 (`iLevelUp01Mult` to `iLevelUp10Mult`, `0x5480a0`). The experience a skill needs from value `L` is
`(fSkillUseFactor · L) ^ fSkillUseExp`, times `fSkillUseSpecMult` for a specialization skill, times `fSkillUseMajorMult` for a major skill
or `fSkillUseMinorMult` otherwise (executable defaults 1, 1, 0.75, 0.75, 1.25; `Oblivion.esm` authors Factor 0.35 and Exp 1.5).
`fSkillUseCurve` 1.95 is **Skyrim's** setting, not Oblivion's. Source: [UESP Oblivion:
Leveling](https://en.uesp.net/wiki/Oblivion:Leveling) and the executable decode in [skill-use.md](skill-use.md) (#7049, V1, V2, V13).
**Built (#7050-#7056):** the [Oblivion skill-use leveling PRD](../prd/oblivion-skill-use-leveling.md) and
[skill-use.md](skill-use.md); 16 of the 21 skills are fed, the level is taken at a completed sleep, and the
level-up settings above are read from the load order (#7196).

## Engine-supplied actor values and stat models — BUILT at HEAD (#7008, OAV-RES / OAV-POP / OAI)

`Oblivion.esm` authors no `AVIF` record: the executable hardcodes its 72 actor
values. The engine therefore supplies one `AVIF` per index, so every consumer
that resolves an actor value by record (conditions, magic effects, ObScript,
`ActorValues`, CHARAL) reads Oblivion exactly as it reads the games whose
masters author them. Parents: [oblivion-actor-values.md](../prd/oblivion-actor-values.md)
(what the values are and who fills them) and
[oblivion-actor-value-identity.md](../prd/oblivion-actor-value-identity.md)
(identity, slot and saves). State below was read from the code at commit
`99bf86ee4`; it says what exists, not that each path is tested or wired to
every consumer (the PRD's Phase 2 census measures that).

### The 72 records

The roster is the one `const` table `OBLIVION_ACTOR_VALUES`
(`crates/plugin/src/esm/records/oblivion_actor_values.rs`), in xEdit's TES4
`wbActorValueEnum` order with the Construction Set's script spellings as
EditorIDs:

| Index | Values |
|---|---|
| 0–7 | Strength, Intelligence, Willpower, Agility, Speed, Endurance, Personality, Luck |
| 8–11 | Health, Magicka, Fatigue, Encumbrance |
| 12–32 | The 21 skills, seven per specialization: Combat 12–18 (Armorer … HeavyArmor), Magic 19–25 (Alchemy … Restoration), Stealth 26–32 (Acrobatics … Speechcraft) |
| 33–36 | Aggression, Confidence, Energy, Responsibility (from `AIDT`) |
| 37–71 | Bounty, Fame, Infamy, MagickaMultiplier, NightEyeBonus, AttackBonus, DefendBonus, CastingPenalty, Blindness, Chameleon, Invisibility, Paralysis, Silence, Confusion, … Vampirism, Darkness, ResistWaterDamage |

**Index coverage: 70 / 70** vanilla-resolvable indices (0–37 and 40–71) resolve by number and by name, 72
records installed, `None` for 38, 39 and anything above 71 (checked 2026-11-17 against UESP *Oblivion Mod:Actor
Value Indices*: all 72 EditorIDs match its names, Fame and Infamy being its reserved rows; pinned by
`install_and_resolve_the_roster_without_reserved_vanilla_values`).

Fame and Infamy (38, 39) are installed but excluded from index and name
resolution unless the caller asks for reserved values
(`oblivion_actor_value(index, include_reserved)`, `vanilla_name`). That
mirrors the executable: vanilla `GetActorValue`, `SetActorValue` and
`ModActorValue` reject them while the dedicated player fame and infamy
commands and OBSE accept them (see [charal.md §9](charal.md)).

### Identity and slot

- Durable identity: `FormIdPair { ENGINE_PLUGIN_ID, local = index + 1 }`.
  `ENGINE_PLUGIN_ID` is a UUID v5 in the engine's own namespace
  (`crates/core/src/form_id.rs`), so no plugin filename can derive it; the
  `+ 1` is because `LoadOrderIdentity::global_form_id` rejects local id 0.
- Runtime key: the engine slot composed with the local id. The slot is the
  lowest unused `Regular` slot in `0x00..=0xFB` after every real plugin
  (`LoadOrderIdentity::allocate_engine_slot`, `crates/scripting/src/load_order.rs`;
  `0xFC` and above belong to alias owners and other form spaces). A loaded
  plugin that already carries the engine identity, a duplicate slot or plugin
  identity, and a full table (252 slots) are errors with a named message, never
  a reuse of a plugin's slot.
- Install: `EsmIndex::install_oblivion_actor_values(slot)` inserts the 72
  records atomically and idempotently, and refuses a slot that already holds
  other records.
- Saves: `FORMAT_MAJOR` is 133 and the actor-value save codec
  (`crates/scripting/src/actor_value_save.rs`) writes `FormIdPair` keys, never
  the session's slot. A pair whose plugin is not loaded is dropped with a
  diagnostic (OAI-SAVE-005).

### Stat models

`CharacterRulesProfile::OBLIVION` selects `NpcStatModel::OblivionNpc` for
`NPC_` and `NpcStatModel::OblivionCreature` for `CREA`
(`crates/core/src/character/profile.rs`); both dispatch to
`oblivion_population::derive` (`crates/plugin/src/esm/records/oblivion_population.rs`).
Consumers branch on the record kind, never on the game.

| Record | Model | Values |
|---|---|---|
| `NPC_`, neither `ACBS` bit 4 nor bit 7, and the `Player` record (form id 7) | Stored | Attributes 0–7, 21 skills and Health from `DATA`; Magicka and Fatigue from `ACBS`; Encumbrance 5 × Strength |
| `NPC_`, bit 4 or bit 7 | Class auto-calc | The rules of [NPC auto-calc stats](#npc-auto-calc-stats--locked-from-the-executable-7008-oav-pop-003) from the race (`ATTR`, `DATA`), the class (favored attributes, specialization, seven major skills) and the level; no values are stamped if the race or class record is missing or the class is malformed (the model declines, it never invents) |
| `CREA` | Creature data | Attributes from `DATA`; each skill is its group's value (Combat for 12–18, Magic for 19–25, Stealth for 26–32); Health, Magicka and Fatigue are multiplied by the level when `ACBS` bit 7 is set (CS *Stats Tab - Creatures*); Encumbrance 5 × Strength |
| Any actor with `AIDT` | Extra values | 33–36 from the first four bytes |

The level an auto-calc or level-offset actor spawns at follows the
executable: bit 7 adds the actor's level to the player's level, a positive
minimum and then a positive maximum clamp it, and the floor is 1
(`0x467807`–`0x467872`; zero bounds are disabled). Bit 7 clear uses the
authored level.

**Creature skills have two views.** The executable remaps each skill group
onto one current-value index (Combat 12, Magic 19, Stealth 26) and routes
Marksman (28) to Combat for the *current* value but to Stealth for the *base*
value (`0x6253c0`–`0x625456` against `0x51d56a`–`0x51d577`). The engine models
this with per-skill aliases (`creature_skill_aliases`): each non-group skill
reads its current value from the group's source (28 from 12) and its base
value from the group head, so a script or magic effect that modifies the
group is seen by every skill that shares it. Stamping one raw number per
skill would lose that sharing.

### Known gaps against the executable

These are measured differences found by reading the code against the traces
in the next section; none of the open ones has a test that pins it yet. (Two were found
and fixed: Magicka truncates `Int * (1 + fNPCBaseMagickaMult)` with the setting
read from the load order, pinned by
`auto_calc_magicka_truncates_and_follows_the_setting`; and the favored-attribute
bonus reads `fAttributeClassPrimaryBonus` for the class's first favored
attribute and `fAttributeClassSecondaryBonus` for the second, the primary
alone when both name one attribute, pinned by
`favored_attribute_bonuses_follow_their_settings_in_class_order`.)

1. **Rounding.** Attribute and skill rounding is round-half-to-even, which
   assumes the x87 default of the executable's `fistp`; the control word was
   not measured.
2. **Creature level scaling** rests on the Construction Set text alone; the
   executable's creature path was not traced.
3. **Flag timing.** Which trigger applies at spawn, level change and reload
   for each of the four flag combinations is [#7022](https://github.com/matiaszanolli/ByroRedux/issues/7022).
4. **Magicka Multiplier (#7198).** The player's Magicka is `trunc(Int x (1 + fPCBaseMagickaMult))` times
   actor value 40 / 10 (the player's pools table below), with a zero multiplier read as 1.0. The engine
   builds the first factor (`oblivion_ruleset_with_gmst`, `fPCBaseMagickaMult` and `fPCBaseHealthMult` read
   from the load order) but not the second: no derived row can carry a factor that is rewritten at zero, and
   nothing reads `MagickaMultiplier` (installed in the roster, index 40). An effect that writes it changes
   nothing here; vanilla leaves it unset, so vanilla is unaffected. How often vanilla content writes it was
   not measured.
5. **PC-offset creature skills and attack damage (#7206).** #7040 locked the level multiply for Health,
   Magicka and Fatigue only. A PC-offset creature's three skills and its attack damage are stamped as the
   record authors them, with no level term:
   - *Skills:* UESP *Oblivion:Leveled Lists* gives `CombatSkill = Combat + 2*CreatureLevel` (the same for
     Magic and Stealth); the Construction Set's *Stats Tab - Creatures* is silent on skills, and the
     executable's skill path was not traced for a level term (the capture decodes only the two-view remap).
   - *Attack damage:* the CS gives "the entered value is added to 1/2 the level of the creature"; the creature
     section's second word form (`0x51c980`) multiplies by `fCreatureCalcDamage`, which `Oblivion.esm` does not author and whose
     executable default is 1.0. Which coefficient the game applies is unresolved.

   Both wait on an observation in the original game (the #7022 smoke harness is the natural place) rather
   than a choice here: 284 vanilla `CREA` records set the offset bit, so a PC-offset creature hits with its
   authored base damage and carries base skills at every player level.

### The player's pools (#7037)

Measured on 2026-11-17 (`--player` in AnvilTheCountsArms, `obscript.cmd 0x14 GetAV`): the live player
reads Health 45, Magicka 0, Fatigue 150 with Intelligence 50, Endurance 40, Strength 50, Agility 30,
Willpower 30. These are the `Player` `NPC_` record's own stored values (`ACBS` flags 0, race `0x907`,
class `CharactergenClass`, every skill 5, `DATA` Health 45, `ACBS` Magicka 0 and Fatigue 150): the
chargen placeholder. The ruleset's player-only rows (Health `2 x End` = 80, Magicka `2 x Int` = 100,
Fatigue `Str + End + Agi + Wil` = 150) exist but do not apply: `RulesetBuilder::Oblivion` builds the
ruleset with `retain_player_bases`, so `player_derived_stats_system` leaves the authored bases alone.
Two consequences: the Magicka bar read full (a zero maximum is a full bar, `hud::fraction`), and Health
disagrees with the formula by 35.

**Fixed for Magicka (#7037).** The ruleset now retains the carried base per stat
(`CharacterRuleset::retain_player_base`): Health and Fatigue keep the record's values, Magicka follows its
formula. Live: the player's Magicka reads 100 at Intelligence 50, 120 after `ModAV Intelligence 10`, and the
HUD bar reads 0.75 after `DamageAV Magicka 30`. Pinned by
`oblivion_profile_refreshes_all_three_player_pools` (it first retained Health and Fatigue per stat; both
retains were dropped under #7041, below).

**Fixed for Fatigue (#7041).** Once the load-time zeroing was found (below), Fatigue joined Magicka: it is its
formula alone. Fatigue is a drained and regenerated pool like Fallout's Action Points, so the refresh moves
the maximum (the base) and leaves the damage layer, which the regeneration tick (10 per second) restores.
Live: 150 at the placeholder attributes; `DamageAV Fatigue 60` reads 0.60 on the HUD and regenerates to the
maximum; `ModAV Endurance 10` raises the maximum to 160.

**Fixed for Health (#7041).** The last retain is gone (`retain_player_base` was removed with it): Health is
`2 x End` like the other two. Pinned by the same test and by
`oblivion_player_template_derives_all_three_pools_from_the_formulas`. The reasoning is in the next paragraph.

**What the engine's player level-up does with Health (#7041, checked 2026-11-17).** Nothing, for Oblivion.
`LevelingModel::OBLIVION` is `SkillUse`: it has no XP curve, so `roll_levels` reaches no level, and
`scripted_skill_system` advances skills for Skyrim's model only. The remaining writers of the player's level
are ObScript `SetLevel` (it sets `CharacterLevel.level` and, as the GECK says, "not any of his stats") and the
Skyrim-shaped `levelup pool` pick. No Oblivion code adds `0.1 x End` per level to Health
(`oblivion_health_gain_per_level` is a tested helper with no caller). So the only Health the engine can
reach today is the level-1 one, `2 x End`, and the retained record value (45) is simply wrong for it. When
Oblivion skill-use leveling is built, the per-level gain must be kept apart from the refreshed base (the
original adds it to the stored term, `0x66c3e0`). **Storage built (#7044):** `ActorValue::stored`, a layer of its
own that `undamaged()` adds, and `apply_oblivion_level_up_health` (whole-number tenth of Endurance, rounded
down) writes it. The permanent-modifier layer was the other candidate; it also carries abilities and gear, so a
Fortify Health ability coming and going could not tell its share from the gains. Pinned by
`oblivion_level_up_health_gains_survive_the_refresh_and_an_ability_coming_and_going`. The level-up calls the helper
since #7052 ([skill-use.md](skill-use.md) §5): at a completed sleep, with the Endurance read after the level's attribute picks; a `FORMAT_MAJOR` bump (v136)
rejects older saves, per the format's rule (`crates/save/src/snapshot.rs`).

What `Oblivion.exe` does for the player. The base of actor values 8 to 11 is read through one function
(`0x5ead00`, taken only when the actor is the player singleton at `0xb333c4`): `(stored + derived) x
multiplier`, with `derived` from `0x5e2210`:

| AV | derived (`0x5e2210`) | multiplier |
|---|---|---|
| 8 Health | `trunc(End x fPCBaseHealthMult)` (`0x548020`, `0xb37720`, exe default 2.0) | 1 |
| 9 Magicka | `trunc(Int x (1 + fPCBaseMagickaMult))` (`0x5482b0` with the player flag set), GMST 1.0, so `2 x Int` | AV 40 (Magicka Multiplier) / 10 (`0xa3f3e8` is the double 10.0) |
| 10 Fatigue | `Str + End + Agi + Wil` (`0x5479d0`) | 1 |

`stored` is the base form's value read through its virtual at `+0x128`. **Decoded (#7041):** on `TESNPC`
that slot is `0x5232d0`, which sends AV 8 to 11 to `TESActorBase`'s getter (`0x519d90`): Health is the
record's stored health field (`0x519c10`), Magicka and Fatigue are the `ACBS` values (getters at `+0x48`
and `+0x4c` of the `ACBS` sub-object at `+0x24`), and AV 11 is derived from Strength. So `stored` is the
record's own `ACBS` / health value, as the engine already reads it. The character-creation calls of the
calculator (`0x596ae5`, `0x5c29e6`, `0x5c2ac0`) pass 1 and skip the pools, so nothing there rewrites the
`Player` record's pool fields. The flag-0 callers `0x50c77c` and `0x50cf9d` run the calculator on a freshly
allocated actor, not the player. **Worked through (#7041).** The combine in `0x5ead00` is a plain `(stored + derived) x multiplier`: the
zero test at `0x5ead3d` only rewrites a zero *multiplier* to 1.0 (the callee's two outputs are the derived
term and the multiplier; arguments traced from the stack frame). The player level-up path (`0x66c3e0`) adds
to the stored Health: `stored = trunc(stored + End x [0xb37708])` through the setter `0x519c50`, i.e. the
stored term is the accumulation, and the formula term is separate. **Settled (#7041): the original zeroes the `Player` record's stored pools when it loads.** The tail of the
`TESNPC` load function (`0x523fc0`..`0x5240f2`: vtable slot `0xe4` test, the calculator at `0x523ff4`) ends,
for every NPC, with `cmp [esi+0xc], 7` at `0x5240b2` (form id 7, the `Player`), and then stores 0 through
the Fatigue setter (`0x5240bc`), the Health setter (`0x5240c5`) and the Magicka setter (`0x5240ce`). The
record's Health 45, Magicka 0 and Fatigue 150 are therefore placeholders that never reach the game; the
player's pools start as the formula term alone (`2 x End`, `2 x Int`, `Str + End + Agi + Wil`), which is
what UESP gives, and the level-up path (`0x66c3e0`) then accumulates into the stored Health on top. (The
Health getter is read at `0x524096` before the zeroing, so a record Health of 0 or less still marks the
actor dead first; the `Player`'s 45 does not.) The other callers of the pool setters: `0x5115ff` /
`0x511617` / `0x51162c` are in a function that allocates a new actor record and, when the source actor is
the player singleton, writes the player's computed Health, Fatigue and Magicka (through `0x5f1910` →
`0x5ead00`) into it, which is a copy of the player, not the player's own start; `0x51a025` / `0x51a036` /
`0x51a047` are the generic base-value setter on `TESActorBase`, whose own callers were not traced.
Consequence for the engine: Fatigue and Magicka are stateless and take their formula; Health is the formula
plus a separately kept accumulation, which the engine's retained total does not model.

Not observed in the original game: the values a new game shows. The executable reading above predicts the
formula values (Health `2 x End`, Magicka `2 x Int`, Fatigue the four-attribute sum) at level 1.

## NPC auto-calc stats — LOCKED from the executable (#7008, OAV-POP-003)

The `ClassAutoCalc` stat model of the Oblivion Actor Values PRD
([oblivion-actor-values.md §3.4](../prd/oblivion-actor-values.md)). Source:
the stat calculator `0x5222d0` of the local classic `Oblivion.exe` (SHA-256
`a8f313845c1545e9a60e1e995961eef4c033115da9443f6d756341df3c2b7dc6`, `.text`
unencrypted) and the `Oblivion.esm` GMST records, with UESP *Oblivion:NPCs*
as the cross-check (level-1 values "identical to those for a player character
created using the same race, gender, and class"). Static analysis only: what
the calculator does is settled, **when** each flag combination reaches it is
[#7022](https://github.com/matiaszanolli/ByroRedux/issues/7022).

`n = level - 1`. Every rounded value is stored with x87 `fistp`
(`0x52248d`, `0x5225c2`), a bare store of a float local, so it rounds by the
thread's control word. **Settled from the executable (#7040): round to nearest
even.** The calculator holds no control-word instruction. Below `0x980000`
every `fnstcw` of the program is followed by exactly two `fldcw` (set, then
restore): no code writes the word one way. The C runtime's math routines
compare it with `0x027F` and reload that constant from `0xaa5098` when it
differs; `0x027F` is round to nearest even at 53-bit precision. Direct3D 9
uses single precision with round to nearest unless the device is created with
`D3DCREATE_FPU_PRESERVE` ([Microsoft: D3DCREATE](https://learn.microsoft.com/en-us/windows/win32/direct3d9/d3dcreate)),
which keeps the thread's word. Every case rounds to nearest even, and the
operands are float locals (`fstp dword [esp+0x14]` before each store), the
width the engine computes in: `round_ties_even` in
`crates/plugin/src/esm/records/oblivion_population.rs` is the same rule. Not
observed in the running game; the flag the game passes to Direct3D was not
decoded, and it does not change the rounding mode.

| Value | Rule (address) |
|---|---|
| Skill | minor `5 + 0.1n`; major (one of the class's 7 major skills) `25 + n`; the skill's specialization equals the class's: add `5 + 0.5n`; add the race skill bonus (7 pairs, `race+0x50`); cap 100 (`0x5224ff`–`0x5225ca`; doubles `0xa2fc80` 0.1, `0xa3f3f0` 5.0, `0xa492b0` 25.0, `0xa2faa0` 0.5). Per level this is the PRD's +0.1 / +0.6 / +1.0 / +1.5 |
| Attribute | race base for the sex (`race+0x74` male, `+0x80` female) `+ fAttributeClassPrimaryBonus` if it is the class's first favored attribute `+ fAttributeClassSecondaryBonus` if the second (`0x5223ab`, `0x5223c2`; both exe defaults 5.0, no `Oblivion.esm` GMST) `+` for each skill it governs: `n` if the skill is major, `0.2n` otherwise (`0x5223d5`–`0x522464`, double `0xa38538`); cap 100. Each attribute has three governed skills, so the per-level step is `0.2(3 - m) + m` = 0.6 / 1.4 / 2.2 / 3 for `m` = 0 / 1 / 2 / 3 major skills |
| Personality | Skipped for every NPC; the authored `DATA` byte stays (`0x522345`–`0x52234f`). Calculated only for form id 7, the `Player` NPC |
| Luck | Calculated like any attribute, but no skill is governed by Luck: race base plus the favored bonus, at every level. The authored `DATA` byte is replaced |
| Health | `base = trunc((Str + End) * fNPCAttributeHealthMult)`; `sum = base + n * (iNPCBasePerLevelHealthMult + [End is a favored attribute] + s)` with `s` = +1 Combat, -1 Magic, 0 Stealth; `factor = fLowLevelNPCBaseHealthMult + (1 - fLowLevelNPCBaseHealthMult) * n / iLowLevelNPCMaxLevel` when `n < iLowLevelNPCMaxLevel`, else 1; `Health = trunc(sum * factor)` (`0x547f80`; truncation `0x9828c0`). `Oblivion.esm` GMSTs: 0.5, 4, 0.4, 4 (the exe defaults differ: 0.5, 4, 0.25, 3). Form id 7 uses `End * fPCBaseHealthMult` (exe default 2.0, no GMST) through `0x548020` |
| Magicka | `trunc(Int * (1 + fNPCBaseMagickaMult))`, GMST 1.5, so 2.5 x Intelligence (`0x5482b0`); the player record uses `fPCBaseMagickaMult` (GMST 1.0) |
| Fatigue | `Str + End + Agi + Wil` (`0x5479d0`; arguments pushed at `0x5226b3`–`0x5226f5`) |
| Encumbrance | Not in the calculator; 5 x Strength stays sourced from UESP only |

Health, Magicka and Fatigue are skipped when the calculator's argument is
nonzero (`0x522689`).

**Callers (#7040).** Every call site resolved to its owner by the function's
data references (the command table, the class vtables):

| Call site | Owner | Argument |
|---|---|---|
| `0x44733a` | the walker `0x447300` over every NPC with ACBS bit 7; its callers are the data-load tail `0x40df80` (`0x40ebaa`, `0x40f45d`), the savegame load `0x465860` (`0x466990`), the level-up routine `0x66c3e0` (`0x66c551`) and the console command `RevertWorld` (`rw`, opcode `0x162`, `0x500c90`, call `0x500cf3`) | 0 (full) |
| `0x523ff4` | a `TESNPC` virtual (`0x523f40`), after the bit 4 test | 0 |
| `0x50ab73` | the script command `DuplicateNPCStats` (opcode `0x114f`, `0x50aaa0`): copies the source NPC's class (`+0x104`) and level to the destination, then recalculates it | 0 |
| `0x50c77c` | `SetClass` (opcode `0x10f0`, `0x50c6e0`): stores the class, then recalculates | 0 |
| `0x50cf9d` | `SetLevel` (opcode `0x1133`, `0x50cd30`): a non-player NPC base is recalculated after the level and its bounds are written; the player (`0xb333c4`) skips the whole tail, as the CS says ("will change the player's level, but not any of his/her stats", [CS: SetLevel](https://cs.uesp.net/wiki/SetLevel)) | 0 |
| `0x596ae5` | `ClassMenu` virtual (`0x596980`, vtable offset `0x38`), the character-creation class screen | 1 (attributes and skills) |
| `0x5c29e6`, `0x5c2ac0` | `RaceSexMenu` virtual (`0x5c2870`, vtable offset `0x28`), the race screen | 1 |

The player's level-up is confirmed: `0x66c3e0` is reached from `0x5ac6a0`, then
`0x5ac730`, then a `LevelUpMenu` virtual (`0x5ac7b0`, offset `0xc`). The
calculator itself tests no ACBS flag: it needs a race (`+0xe8`) and a class
(`+0x104`) and nothing else, and form id 7 (the `Player`) takes the player
branch. The flag tests belong to the callers (bit 4 at the NPC load, bit 7 in
the walker), so `SetLevel`, `SetClass` and `DuplicateNPCStats` recalculate
any NPC whatever its flags, which is evidence for #7022. The engine does not
do that: its `SetLevel` writes the level only
(`crates/scripting/src/obscript_more/tail.rs`, `cmd_1133`) and `SetClass`
stores the class only, so a script that retunes an NPC's level or class
leaves its stats as spawned.

**Triggers.** Two independent tests reach the calculator:

- Bit 4 of `ACBS` flags ("Auto-calc stats"): the NPC load path calls the NPC
  vtable slot `0xe4` (`0x519d40`, test at `0x519d46`) and then the calculator
  at `0x523ff4`.
- Bit 7 ("PC Level Offset"): the walker `0x447300` runs the calculator on
  every NPC with the bit set (`0x44732b`–`0x44733a`). Its callers: the
  data-load tail of the program start (`0x40df80`, calls at `0x40ebaa`,
  `0x40f45d`), the savegame load (`0x465860`, call at `0x466990`) and
  `0x66c3e0` (call at `0x66c551`, after a store through `0x519c50`; reached
  from the `LevelUpMenu` virtual `0x5ac7b0`, so it is the player's level-up),
  and the console command `RevertWorld` (`0x500cf3`).

The engine therefore treats an NPC with either bit as auto-calc. Which of the
two the editor's own controls map to, and the result of each of the four flag
combinations over spawn, level change and reload, is the observation in #7022.

### When the calculator runs — decoded, observation pending (#7022)

What the executable's code paths say for each combination of ACBS bit 4 (auto-calc) and bit 7 (PC level
offset). The in-game observation that checks it is [oblivion-autocalc-timing.md](../smoke-tests/oblivion-autocalc-timing.md)
(a four-NPC test plugin, `scripts/oblivion-autocalc-plugin.py`); until it is run, every row is a prediction from
static analysis, not an observed value.

- **Bit 4** is tested once per NPC form in the form's init virtual, `0x523f40` (`TESNPC` vtable offset `0x6c`).
  It returns at once when the form's flag `0x8` is set and sets it at the end (`0x46ab80`), so it runs once per
  form; it links the race and class (`+0x104`), calls the calculator when the vtable `+0xe4` test (`0x519d40`:
  form type `NPC_` and ACBS bit 4) holds, flags a Health of 0 or less as starts-dead (`0x46ab40`) and zeroes the
  `Player` record's stored pools (`0x5240b2`). The calculator's level is `0x4677f0`, which for bit 7 clear is the
  ACBS level. What calls the init pass, and so when in the session it runs, was not decoded.
- **Bit 7** is tested in the walker `0x447300` over every NPC base form (`[0xb33a98]`, the data handler). It
  runs at the end of the data load, on a savegame load, when the player levels up (`LevelUpMenu` to `0x66c3e0`)
  and from the console command `RevertWorld`. With bit 7 set the level `0x4677f0` returns is the record's level
  plus the player's current base level, within Calc Min and Max.
- **Neither is on a cell-change path.** Every caller of the walker and of the calculator is in the lists above
  (the data load, the savegame load, the level-up, `RevertWorld`, the three script commands, the two character
  creation menus); none is cell-load or spawn code. A reload of a cell leaves the base form's values as the last
  event left them, and an actor reads them from the base form on each use (`0x5ead00`).

| Bits (auto-calc, offset) | Calculated | Level used | Recalculated |
|---|---|---|---|
| 0, 0 | never | none: the authored values | never (except by `SetLevel`, `SetClass`, `DuplicateNPCStats`, which need no flag) |
| 1, 0 | once, in the init pass | the ACBS level | never |
| 0, 1 | in the walker | player level + offset | data load, savegame load, level-up, `RevertWorld` |
| 1, 1 | in the init pass, then in the walker | player level + offset | as 0, 1 |

The engine stamps these values once, when an NPC spawns (`derive`, `oblivion_population.rs`), from the player's
level at that moment (`player_level_for_spawn`). It agrees with the table for an NPC spawned after the last
walker event, and for the auto-calc-only combination at any time; it differs for an already-spawned offset NPC
when the player levels up (the executable recalculates every offset NPC; the engine keeps the spawn-time
values). Oblivion has had a level-up since #7052, so the difference is reachable: this divergence is declared by the [skill-use leveling PRD](../prd/oblivion-skill-use-leveling.md) (comparison row 5, OSL-NG-003: the CS says the world follows the player's level only after a loading door or a cell change; owned by #7022 / #7040).

### Creature level scaling — LOCKED from the executable (#7040)

The CS says a PC-level-offset creature's Health, spell points and Fatigue are
"multiplied by the level of the creature" ([CS: Stats Tab - Creatures](https://cs.uesp.net/wiki/Stats_Tab_-_Creatures)).
The executable agrees, and shows where. Creatures have no calculator: the
base form answers the value on every read.

- The base value of an actor is `0x5ead00`; for every actor but the player it
  asks the base form's virtual at `+0x128` and adds nothing (the multiplier is
  1). For a `TESCreature` that virtual is `0x51d540`, which hands the pools
  to `TESActorBase`'s getter `0x519d90`.
- That getter reads Health through the form at `this+0x80` (virtual `+0x10`)
  and spell points and Fatigue through the ACBS object at `this+0x24`
  (virtuals `+0x48`, `+0x4c`). `TESCreature` overrides all three:
  `0x51ca10` (Health), `0x51caa0` (spell points, ACBS word `+8`) and
  `0x51cad0` (Fatigue, ACBS word `+0xa`).
- Each returns the stored word unchanged when ACBS bit 7 ("PC Level Offset",
  `acbs >> 7 & 1`) is clear. With it set, each returns `stored x max(level, 1)`
  in 16-bit arithmetic (`imul cx, ax` then `movzx`), so a product past 65 535
  wraps.
- The level is the ACBS level getter `0x4677f0`: with bit 7 set it is the
  record's level plus the player's base level (`0xb333c4`, the form's `+0x24`
  data), then the Calc Min (`+0x10`) and Calc Max (`+0x12`) bounds; zero
  bounds are disabled.

The engine matches this (`creature_values` and `level` in
`oblivion_population.rs`) with two differences, one fixed here:

- **Fixed:** the product was an `f32` multiply, so 400 Health at level 200
  read 80 000; the executable's is 14 464. It wraps now
  (`creature_pools_scale_by_the_level_in_sixteen_bit_arithmetic`, which fails
  on the old product).
- **Open:** the executable multiplies on every read, so a PC-offset creature's
  maxima follow the player's level while it lives; the engine stamps them once
  at spawn (`actor_level_at_spawn`). The two agree until the player levels
  with such a creature loaded, which is reachable since Oblivion's level-up
  landed (#7052); the divergence is declared by the [skill-use leveling
  PRD](../prd/oblivion-skill-use-leveling.md) (row 5, OSL-NG-003, owned by #7022 / #7040).
- A second word form of the creature (`0x51c980`, vtable offset `0x10` of another sub-object) scales as
  `stored + level x fCreatureCalcDamage` with the same level. The CS says the attack damage of a PC-level-offset
  creature is "added to 1/2 the level of the creature", which is that formula at a setting of 0.5;
  `Oblivion.esm` authors no `fCreatureCalcDamage` (byte scan, 2026-11-22), and the executable's default is 1.0,
  so the coefficient is unadjudicated. The engine does read the attack damage (`stamp_creature_attack`,
  `byroredux/src/npc_spawn/stamps.rs`, stamps `CreatureAttack` from the `DATA` value since 2026-11-15) but
  stamps it **unscaled**: Known gaps, item 5 (#7206).

## Essential bleedout and encumbrance — LOCKED / NOT WIRED (#6284)

The two executable-derived facts `CharacterRulesProfile::OBLIVION` carries (#5921).

- **Essential bleedout.** The essential-actor down time is **10 s**; Oblivion.exe
  loads it as a constant beside the setting's name (there is no GMST to read), and
  the health regained on getting up is **0.3** (`scripting.md`, "Essential and
  Protected actors, bleedout").
- **Over-encumbrance — NOT wired** (`OverEncumberedRule::None`). UESP
  *Oblivion:Encumbrance*: when the carried weight exceeds the maximum, "you will
  become over-encumbered and will not be able to move". Encumbrance is a
  hard-coded actor value with no `AVIF`, so the engine keeps no over-encumbrance
  rule for Oblivion yet; the Movement section above covers the speed formula only.
- **Barter.** No barter model is wired (`barter: None`); the Commerce section
  above holds the sourced price formula.

## Crime fines — theft, assault and murder built, the rest sourced and unbuilt

Sources: CS wiki *Crime Overview*, *iCrimeGoldXX*, *fCrimeGoldSteal*,
*GetPCFactionSteal*; UESP *Oblivion:Crime*. Built as
`character::CrimeModel` (`crates/core/src/character/crime.rs`), a
`CharacterRulesProfile` row (`crime()`, Oblivion and Skyrim: `CrimeModel::OBLIVION` / `CrimeModel::SKYRIM`) with the load order's
GMSTs overriding the defaults, consumed by `byroredux/src/crime.rs`.

| Crime | Bounty | Built |
|---|---|---|
| Theft | `ceil(item value × fCrimeGoldSteal)`, minimum 1; `fCrimeGoldSteal` 0.5 (Oblivion.esm does not author it) | yes — witnessed only |
| Lockpicking an owned lock | 5 | no |
| Trespass | 5 (after a 30 s warning) | no |
| Pickpocket | 25 | no |
| Assault | 40 (`iCrimeGoldAttack`; Oblivion.esm authors only `iCrimeGoldAttackMin` = 500) | yes — a non-hostile NPC, once per victim, witnessed only |
| Jailbreak | 50 (`iCrimeGoldJailBreak`, authored) | no |
| Horse theft | 250 (`iCrimeGoldStealHorse`, authored) | no |
| Murder | 1000 (`iCrimeGoldMurder`, not authored), plus one infamy (one per 1000 gold a crime adds, UESP *Infamy*) | yes — a non-hostile NPC; the victim's factions get the Murder flag unwitnessed |

- The faction Steal flag is set by any theft from a faction's property,
  witnessed or not (`GetPCFactionSteal`: "this does not have to be a crime
  that was witnessed by anyone"). Only a faction owner marks a flag; an NPC
  owner's own factions are not resolved.
- A witness is any living, non-creature NPC that passes the hostility pass's
  sight test (`fSneakMaxDistance`, line of sight). The wiki's "successfully
  sneaking prevents being detected" has no stealth roll here yet.
- Witnessed crime calls the guards (`crime.rs`): the guards within
  `iCrimeAlarmRecDistance` (authored, 4000) of a witness, the
  `iNumberGuardsCrimeResponse` (4, not authored) nearest the player, learn of it
  and walk up to open their greeting (the force-greet the dialogue runtime
  already runs), which the authored arrest greeting answers. The two settings'
  CS descriptions are the whole source; reading them as "within earshot of a
  witness" and "nearest the player" is an interpretation.
- A wanted player is rushed on sight: a guard who can see the player with a
  bounty at or past `iCrimeGoldAttackMin` ("the minimum bounty level required
  for guards to rush a player on sight"; authored 500 in Oblivion.esm, and the
  Gray Cowl of Nocturnal's script sets the bounty to at least that so every
  guard comes) walks up and opens its greeting. The wiki's "arrest on sight over
  about 1000, attack over about 4000" are not settings: they are conditions in the
  authored greetings (`GetCrimeGold < 1000` for the lenient one, the pay-the-fine
  topic gated on `< 5000`), so they come with the dialogue data and nothing is
  hardcoded. A rushing guard waits 30 s before rushing again (engine pacing: the
  original's arrest greeting has no way out, ours closes with the page).
- Guards are authored data, not engine code. A guard's arrest talk is
  `GREETING` INFOs gated on `GetCrimeGold > 0`, `CanPayCrimeGold`, `IsGuard`
  and `IsTurnArrest` (Oblivion.esm: 46 INFOs use them), with result scripts
  for the choices. Built: the
  condition vocabulary (`ConditionFunction::from_index_in`, the Oblivion
  dialect: 116 / 125 / 127 / 171 / 193 / 249 / 251 / 329), the CTDA "Run On
  Target" bit, and the `CLAS` Guard flag (`guards.rs`), so 39 of those 46 INFOs
  are now evaluable (the rest need `GetActorValue`, `GetDisposition` and
  `IsWeaponOut` on Oblivion). The result commands are built too (`jail.rs`):
  `PayFineThief` (the bounty's gold leaves, the bounty is wiped), `PayFine`
  (that, the stolen goods confiscated, and the player sent to the exit of the
  nearest jail) and `GoToJail` (bounty wiped for free, goods confiscated,
  the player sent to the cell to serve `bounty / iCrimeDaysInPrisonMod` days,
  ended by resting in a bed there: `InteractionKind::Sleep`, offered only during
  a sentence).
  Jails are the load order's linked `PrisonMarker` door pairs (14 in
  Oblivion.esm); "nearest" is the closest exit in the player's worldspace, and
  a player indoors or somewhere with no jail gets a declined command with
  nothing half done. The confiscated goods go into the jail's `StolenGoods` chest
  (the container placed in the jail's interior cell; all 14 in Oblivion.esm
  author no contents of their own), whether or not that cell is loaded: a
  loaded chest receives them directly, an unloaded one has them added to its
  parked inventory (`reference_state::deposit_into_reference`), and the ledger's
  `evidence` holds them until they arrive. Not built: the skill loss for time
  served (UESP names no rule for which skill).
- Assault is charged once per victim (the original turns the victim
  hostile; a hostile victim is self-defence). A kill in one blow is only a
  murder. "Non-hostile" is the hostility pass's own judgement plus not
  already fighting the player; the Attack flag only marks factions the
  player belongs to (`GetPCFactionAttack`). Not built: a guard kill that
  incurs a bounty with no witness, hostile spell effects as assault,
  creatures as victims, the victim's disposition drop.
- Shivering Isles tracks a separate bounty; one value is kept.
- Beds are told from chairs by the `furnituremarkerXX` a legacy position
  refers to: 1 - 10 beds, 11 - 20 chairs (CS *GetFurnitureMarkerID*), checked
  against all 75 furniture NIFs in Oblivion's meshes archive (every bed 1 - 4,
  every chair, bench, stool and throne 11 - 14) and FO3 / FNV's (beds 1 - 6,
  seats 11 - 20, work and lean markers 22). Only beds are told apart; everything
  else keeps the Sit default. This also makes NPC Sleep take beds on Oblivion,
  FO3 and FNV (#5390).
