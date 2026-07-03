# CHARAL — Skyrim character ruleset (data capture)

Living capture of the **Skyrim** `CharacterRuleset` (CHARAL §5) gameplay-system
formulas that sit just downstream of what's already built (`AttributeSet::SKYRIM`,
`SkillSet::SKYRIM`, `LevelingModel::SKYRIM` — see `crates/core/src/character/
skyrim.rs`). Rows are **LOCKED** (sourced) or **PENDING**. No guessing
([[feedback_no_guessing]]). Parent: [charal.md](charal.md).

## Pickpocket chance — LOCKED, but a gameplay-system input, not a CHARAL derived stat

Source: UESP *Skyrim:Pickpocket*, 2026-07-04. Named game settings (GMSTs) — the
strongest possible sourcing, same tier as the FO4 `fAVD*` constants:

```
fPickPocketActorSkillBase  = 20      fPickPocketTargetSkillBase = 20
fPickPocketActorSkillMult  = 1       fPickPocketTargetSkillMult = -0.25
fPickPocketAmountMult      = -0.1    fPickPocketWeightMult      = -4
fPickPocketMaxChance       = 90      fPickPocketMinChance       = 0
fPickPocketDetected        = 25
```

```
BaseChance = 15 + PlayerSkill − TargetSkill/4
           (= (20+PlayerSkill)·1 + (20+TargetSkill)·(−0.25), the two Base/Mult GMSTs folded in)

Chance = BaseChance − GoldOrItemValue/10 − 4·ItemWeight + SneakBonus
                     + LightFingers + NightThief + (Cutpurse if stealing gold)
                     + EffectsBonus
         clamped [0, 90]
```

`Detected = 25` is a **separate** penalty applied only on a repeat attempt against
a target that has already caught you once — not part of the base formula (an
earlier misreading would fold it in; the page explicitly separates it out).
Perk terms: `LightFingers` = `20 × rank` (rank 0–5, so 0–100 — this is the
dominant term at high skill); `NightThief` = flat `+25` if the target is asleep;
`Cutpurse` = flat `+50`, gold-only. `EffectsBonus` is Fortify Pickpocket
potions/apparel (additive post-patch; multiplicative pre-patch — the page notes
this changed with an official patch, a genuine version-dependent behavior
difference worth remembering if compat targets a specific patch level).
`SneakBonus` is referenced but not decomposed further on this page — PENDING if
ever needed at that precision.

**Governed by the Pickpocket *skill*, not Sneak** — this is the interesting
cross-game divergence: FO3/FNV's pickpocket formula (`charal-fnv-fo3-ruleset.md`)
is Sneak-skill-governed (Fallout has no separate Pickpocket skill), while
Skyrim split Pickpocket into its own of 18 skills (`SkillSet::SKYRIM`) and
governs pickpocketing with it directly instead. Same family of formula
(skill-chained percent chance, item-value/weight penalty, hard-capped), same
"gameplay-system input, not a CHARAL derived stat" routing as the Fallout
version and as FO4/FO76's own chance formulas — CHARAL produces the Pickpocket
skill AV; this formula consumes it downstream, same boundary as every other
member of this family so far.

**Maximum chance is hard-capped at 90%,** unraisable by any means (explicit
wiki note — unlike FO3/FNV's cap, which is a "practical" 85% from the formula's
own arithmetic, Skyrim's 90% is an unconditional engine ceiling regardless of
build).

## Speech — Persuade / Intimidate / Bribe — LOCKED, gameplay-system input

Source: UESP *Skyrim:Speech*, 2026-07-04. Three distinct dialogue-check
formulas, all consuming the Speech skill AV plus other actor state — same
"CHARAL produces the AV, this formula is downstream" boundary as Pickpocket.

**Persuade** — flat skill-threshold check, no formula at all: Very Easy/Easy/
Average/Hard/Very Hard require Speech 10/25/50/75/100 respectively. The
Persuasion perk (50 Speech) drops all but Very Easy by 30%: 10/18/35/53/70.
The lowest attainable Speech is 15, so Very Easy is unfailable by construction.

**Intimidate:**
```
Player's Scariness = PlayerLevel × (1 + max(−1, (PlayerSpeech − NpcSpeech)/100)) ^ fIntimidateSpeechcraftCurve
NPC's Scariness     = NpcLevel × fIntimidateConfidenceMult_<NpcConfidenceLevel>
```
Success requires Player's Scariness > NPC's Scariness. One GMST per NPC
confidence tier (Cowardly/Cautious/Average/Brave/Foolhardy). Player's
Scariness is further modifiable by perks with the "Mod Player Intimidation"
entry point (same entry-point-table pattern as `perk_entry_points.md`).

**Bribe:**
```
NpcMorality = NpcMoralityAV × fBribeMoralityMult + 1.0
v11 = (NpcLevel × fBribeNPCLevelMult + PlayerLevel) − ((PlayerSpeech − NpcSpeech) − 100) × fBribeSpeechCraftMult
bribeValue = v11 ^ fBribeCostCurve × fBribeScale × NpcMorality
```
Bribe success is just "player has ≥ bribeValue gold" — no roll.

None of these three are CHARAL derived stats; they're gameplay-system
consumers of the Speech AV, same routing as Pickpocket above.

## Barter prices — LOCKED, gameplay-system input

Source: UESP *Skyrim:Speech* "Prices" section, 2026-07-04. Named GMSTs
`fBarterMax=3.3`, `fBarterMin=2.0`:

```
basePriceFactor = fBarterMax − (fBarterMax − fBarterMin) × min(Speech,100)/100
                = 3.3 − 0.013 × min(Speech,100)

sellPriceModifier = HagglingS × AllureS × (1 + FortifyBarterPotion) × (1 + Σ FortifyBarterEquip/Blessing)
buyPriceModifier  = HagglingB × AllureB × (1 − FortifyBarterPotion) × (1 − Σ FortifyBarterEquip/Blessing)

sellPrice = round(itemValue × sellPriceModifier / basePriceFactor)
buyPrice  = round(itemValue × buyPriceModifier  × basePriceFactor)

Trade price cap: sellPrice ≤ itemValue × 1.00,  buyPrice ≥ itemValue × 1.05
```
`HagglingS` = 1.10/1.15/1.20/1.25/1.30 at ranks 1–5; `AllureS` = 1.10.
`HagglingB` is the **reciprocal of `HagglingS`, rounded to 2 decimal places**
(0.91/0.87/0.83/0.80/0.77) — the wiki explicitly warns not to use the
untruncated reciprocal. This is the same "buy = 1/sell reciprocal shape" the
FO4 Barter Prices formula already showed (`charal-fo4-ruleset.md`), now
confirmed as a cross-game pattern rather than an FO4 coincidence, with Skyrim
additionally rounding the reciprocal to a fixed 2-decimal table instead of
computing it live.

Same routing as the checks above: Speech AV is a CHARAL output, this pricing
formula is a downstream gameplay-system consumer, not a derived stat itself.

## Lockpicking — LOCKED (community-derived, not named GMSTs), gameplay-system input

Source: UESP *Skyrim:Lockpicking*, 2026-07-04. Weaker sourcing tier than
Pickpocket/Speech above — the page labels this "as it is currently understood"
(community reverse-engineering, no named GMSTs cited), so treat the constants
as good-but-unofficial.

```
SweetSpotDeg     = 60 × 2^(−LockDifficulty) × (0.82 + 0.6·Level/100) × MatchingPerk × (1 + Enchantment + Potion)
PartialPickDeg   = (26 − 4·LockDifficulty) × (0.775 + 1.5·Level/100)     [one zone each side of the sweet spot]

LockDifficulty = 1(Novice)..5(Master)
MatchingPerk   = 1.75 + 0.25·LockDifficulty   if the tier-matching perk is unlocked, else 1
Enchantment    = sum of equipped Fortify Lockpicking magnitude (0.5 = +50%)
Potion         = active Fortify Lockpicking potion magnitude
```

Lockpick durability decays only while "struggling" (pick held outside both the
sweet spot and partial-pick zones); time-to-break is skill- and
difficulty-gated:
```
BaseBreakTime(DifficultyTier) = 2.00s Novice / 1.00s Apprentice / 0.75s Adept / 0.50s Expert / 0.25s Master
BreakTime = BaseBreakTime × (1 + 0.5·Level/100)      [level 100 → 1.5× base]
```
Same "skill AV in, minigame-timing formula out" shape as the sneak-detection
and pickpocket formulas — Lockpicking AV is a CHARAL output, this minigame
tuning is a downstream consumer, not a derived stat itself.

## Sneak Detection (Skyrim) — LOCKED (Sound/Distance halves), PENDING (Visual/skill halves) — out of CHARAL scope

Source: UESP *Skyrim:Sneak*, 2026-07-04. A **second engine's** full stealth
detection formula — same "real, sourced, one layer downstream of CHARAL"
bucket as the FNV formula already built in `crates/core/src/stealth.rs`
(see `charal-fnv-fo3-ruleset.md`), not a candidate for the `derived` table.

```
DetectionValue = fSneakBaseValue
               + (SoundFactor + VisualFactor + NoticerSkillFactor) × DistanceAttenuation
               + (NoticerSkillFactor − SneakerSkillFactor)
```
The wiki's own formula has `NoticerSkillFactor` appearing twice (once inside
the attenuated sum, once again unattenuated against `SneakerSkillFactor`) —
transcribed as given, flagged as a possible source-page redundancy rather than
silently "fixed," since neither `VisualFactor` nor either `SkillFactor` term is
decomposed numerically on this page (see PENDING below).

**Fully locked** (named GMSTs):
```
fSneakBaseValue = −15
DistanceAttenuation = (1 − distance/fSneakMaxDistance)^fSneakDistanceAttenuationExponent
                    = (1 − distance/2500)^2                        [fSneakMaxDistance=2500, exponent=2]

SoundFactor = (Movement + Action) × (1 if Sneaker has LoS to Noticer else fSneakSoundLosMult=0.3)
  Movement = (fSneakEquippedWeightBase + fSneakEquippedWeightMult·ArmorWeight) × (fSneakRunningMult if running) × Muffle
           = (12 + 0.5·ArmorWeight) × (2 if running) × Muffle      [Base=12, WeightMult=0.5, RunningMult=2]
  Action   = ActionSound × fSneakActionMult = ActionSound × 2
```
`Muffle` = 1 − magic-effect magnitude (Muffled Movement perk = 0.5 muffle i.e.
50% noise reduction; Silence perk = 0.0 muffle i.e. silent). **Sneak skill
itself does NOT reduce Sound or Visual factor — the wiki is explicit that it
only reduces the Noticer's skill factor term**, the inverse framing from FNV
(where `TargetSkill` reads the *sneaking* actor's own Sneak AV directly). Worth
remembering as a real per-engine formula-shape difference, not just different
constants.

**PENDING** — `VisualFactor` (light-level/LOS-cone dependent, described
qualitatively only: brighter light increases it, equipped-spell glow adds to
it, enchantment aura does not) and both `NoticerSkillFactor`/
`SneakerSkillFactor` (not decomposed to a formula on this page) have no
citable numeric form yet — no guessing, [[feedback_no_guessing]].

Sneak attacks: flat, perk-gated damage multipliers by weapon type (Unarmed/Bow
×2, Two-handed ×2, Sword/Mace/Axe ×3 → ×6 with Backstab, Dagger ×3 → ×6 with
Backstab → ×15 with Assassin's Blade, doubled again by 4 specific Dark
Brotherhood gloves) — a perk-effect table, not a derived-stat formula, same
bucket as every other perk-damage-multiplier table already captured.

Not wired into `stealth.rs` — that module's `DetectionInputs`/`detection_score`
is FNV-specific by construction (its own doc comment says so); a Skyrim
variant would be a second, parallel formula (different shape per the muffle
note above), not a drop-in reuse. Flagging as a build candidate if/when the
Visual/skill-factor gap closes, not building speculatively ahead of that.

## Light Armor Rating Bonus — LOCKED, BUILT (player only)

Source: UESP *Skyrim:Light Armor*, 2026-07-04.
```
ArmorRatingMultiplier = 1 + 0.004 × LightArmorSkill    (player)
ArmorRatingMultiplier = 1 + 0.015 × LightArmorSkill    (NPC — distinct, higher, constant)
```
Structurally different from every other Skyrim finding so far: it's a clean
affine **multiplier** output driven by a **skill** AV, not an attribute (Skyrim
has none) — the shape `DerivedStatFormula` already supports (`DerivedInput`
takes any AVIF FormID, not just attributes). **Built 2026-07-04** in
`skyrim_ruleset()` (`crates/core/src/character/skyrim.rs`,
`LIGHT_ARMOR_RATING_COEFF`) — the first entry in Skyrim's previously-empty
derived table, and the first skill-derived (not attribute-derived) entry
anywhere in CHARAL. Marked `player_only` (`DerivedScope::PlayerOnly`) because
NPCs use the distinct `0.015` constant, not modelled — same "NPCs derive
differently" pattern `fallout.rs` already applies to Health/Action Points; a
future NPC-scoped variant would need its own formula entry, not a scope flip.
Resolves against `"DamageResist"` (the CK/AVIF editor ID for the Armor Rating
AV) and `"LightArmor"` (confirmed against `SkillSet::SKYRIM` in `skill.rs`) —
`"DamageResist"` is standard Bethesda-modding knowledge, not independently
re-verified against a parsed AVIF set this session; the resolve-or-skip
contract means a wrong EditorID just skips the entry rather than breaking
anything. 2 new tests (`light_armor_rating_bonus_matches_uesp`,
`_skipped_when_unresolved`), core 508 green, workspace green, 0 new clippy
warnings.

Heavy Armor's presumed-symmetric constant is still unsourced — if it turns up
later it lands as its own entry, not a blocker for this one anymore.

## Armor Rating / Damage Reduction — full formula LOCKED, confirms the built coefficient, most of it out of scope

Source: UESP *Skyrim:Armor*, 2026-07-04. Gives the **complete** per-item armor
formula that the Light Armor coefficient above is only one term of —
independently confirms both constants built above exactly, and closes the
Heavy Armor symmetry question along the way:

```
ItemArmorRating = CEILING[ (BaseRating + ItemQuality) × (1 + 0.4×(Skill+SkillEffect)/100) ]
                 × (1 + UnisonPerk) × (1 + MatchingSet) × (1 + ArmorPerk)

DisplayedArmorRating = Σ ItemArmorRating (worn pieces) + ArmorEffects

DamageReduction% = DisplayedArmorRating × 0.12 + 3.00 × PiecesWorn   [hidden +3%/piece, incl. shield]
                    capped at 80%

PhysicalDurabilityMultiplier = 100 / (100 − DamageReduction%)        [×5 at the 80% cap]
```
`UnisonPerk` = Custom Fit (light) / Well Fitted (heavy); `ArmorPerk` = Agile
Defender (light) / Juggernaut (heavy) — the same perk-pair-per-armor-type
shape already seen in `charal-skyrim-ruleset.md`'s Light Armor perk tree.
**For NPCs the skill coefficient is `1.5` instead of `0.4`, and Custom Fit is
`1.25² = 1.5625` instead of `1.25`** — the `0.4/100 = 0.004` and `1.5/100 =
0.015` here are the **exact same constants** already built into
`LIGHT_ARMOR_RATING_COEFF` (skyrim.rs) and its documented NPC counterpart
above, independently confirmed by a second source. **Heavy Armor uses the
identical `0.4`/`1.5` coefficients** — the formula is generic over armor type,
so there's no separate "Heavy Armor constant" to wait for; the presumed
symmetry above is now confirmed, not just presumed.

**Why the rest stays out of `DerivedStatFormula` / uncoded:** `ItemArmorRating`
sums over **individually-tempered worn items** (Smithing's `ItemQuality` is a
per-item state, not a character AV) with a `CEILING` and three multiplicative
perk gates, then `DamageReduction%` additionally needs the **count of pieces
worn** — none of this is expressible as a function of a fixed 1-2 actor-value
inputs the way `DerivedStatFormula` models Health/Carry Weight/this coefficient.
It's a genuine equipment/combat-system calculation (iterate worn items, sum,
convert to a capped percentage), same "real formula, wrong layer for CHARAL's
per-AV derived table" bucket as the full Alchemy potion-strength formula.
`LIGHT_ARMOR_RATING_COEFF` in `skyrim.rs` remains a correct, useful, but
**partial** piece of this larger picture — the per-item skill-scaling term
only, confirmed correct by this page, not a complete Armor Rating pipeline.
A `DamageReduction%`/durability-multiplier calculator would be a standalone
function (worn-item iteration + the capped percentage formula above), same
shape as `stealth.rs` — real and buildable, but belongs in a combat/equipment
module, not `character/`, and not attempted without being asked given no such
module exists yet.

Also on this page, not skill-derived (equipped-weight-derived, applies
regardless of armor skill): **movement speed penalty** = `min(15%, TotalEquippedWeight × 3/23)` —
a universal encumbrance mechanic, not part of the Light Armor governance
graph, noted here only because it appeared on the same page.

## Alchemy potion/poison strength — LOCKED, crafting-system output (not an actor stat)

Source: UESP *Skyrim:Alchemy* "Formula" section, 2026-07-04. Named GMSTs
`fAlchemyIngredientInitMult=4`, `fAlchemySkillFactor=1.5`:
```
Result = fAlchemyIngredientInitMult × BaseMag × SkillMult
        × AlchemistPerk[1.0–2.0] × BenefactorPerk[1.25] × PhysicianPerk[1.25]
        × PoisonerPerk[1.25] × SumOfEnchantments[≥1.0] × SeekerOfShadows[1.1]

SkillMult = 1 + (fAlchemySkillFactor − 1) × Skill/100 = 1 + 0.005·Skill

If Result < BaseMag, Result = BaseMag (magnitude floor).
```
Same "clean affine SkillMult term" shape as Light Armor's `1+0.004·Skill`
Armor Rating bonus above — second confirmation that Skyrim's
skill-drives-a-multiplier pattern is a real recurring shape, not a one-off.
**Routed further out of CHARAL scope than every other finding so far**: the
output isn't even an actor stat modifier (like Pickpocket chance or Barter
price) — it's the potency of a **crafted item** (a potion/poison), which then
separately affects whoever consumes it. Firmly a crafting-system formula, not
a derived stat or even a gameplay-system consumer of an actor's own stats.

## Enchanting effect strength — LOCKED, crafting-system output, genuinely different formula shape

Source: UESP *Skyrim:Enchanting Effects*, 2026-07-04. Third crafting-system
skill-multiplier found this session (after Alchemy, Light/Heavy Armor), but
this one **breaks the affine pattern**:
```
NetMagnitude = BaseMagnitude × SoulMultiplier × SkillMultiplier
             × (1+PotionEffect) × (1+EnchanterPerk) × (1+SpecificPerkMod) × (1+SeekerOfSorcery)
             , floored

SkillMultiplier = 1 + (Skill/100) × (Skill/100 − 0.14) / 3.4     ← QUADRATIC in Skill, not affine
```
At Skill=100: `1 + 1×0.86/3.4 = 1.2529` — matches the source's own worked
claim ("100 skill points grants ~25.29%"). This is the **first quadratic
skill-derived term found in CHARAL research** — every other skill-multiplier
this session (Alchemy `1+0.005·Skill`, Armor `1+0.004·Skill`) was a clean
affine line; Enchanting's curve accelerates at high skill instead. Worth
remembering if a future audit assumes "skill multipliers are always affine" —
they aren't, universally.

`SoulMultiplier` is a discrete table (Grand ×1, Greater ×2/3, Common ×1/3,
Lesser ×1/6, Petty ×1/12) keyed by soul-gem size, not a continuous formula.
Weapon enchantments additionally have a **Charges Per Use** formula
(`3×(BaseCost×Magnitude/MaxMagnitude)^1.1×(1−√(Skill/200))`) governing uses
before recharge — another skill term, this one under a square root, a third
distinct shape. Soul charge capacities are a matching discrete table (Grand
3000 → Petty 250).

Same routing as Alchemy: output lands on a **crafted item** (an enchanted
weapon/armor piece), not an actor stat — firmly a crafting-system formula, not
a CHARAL derived stat or even a gameplay-system consumer of the actor's own
stats. Documented for the formula-shape diversity, not as a build candidate.
