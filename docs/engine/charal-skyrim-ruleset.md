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

## Light Armor Rating Bonus — LOCKED, first Skyrim skill-derived multiplier candidate

Source: UESP *Skyrim:Light Armor*, 2026-07-04.
```
ArmorRatingMultiplier = 1 + 0.004 × LightArmorSkill    (player)
ArmorRatingMultiplier = 1 + 0.015 × LightArmorSkill    (NPC — distinct, higher, constant)
```
Structurally different from every other Skyrim finding so far: it's a clean
affine **multiplier** output driven by a **skill** AV, not an attribute (Skyrim
has none) — the shape `DerivedStatFormula` already supports (`DerivedInput`
takes any AVIF FormID, not just attributes), just never yet populated for
Skyrim because `skyrim_ruleset()`'s empty `derived` table was reasoned about
in terms of **attribute**-derived pools only (see `skyrim.rs` module doc: "no
attribute-derived pools"), not skill-derived multipliers — this is a
genuinely new category, not a contradiction of what's already built. A real
candidate to populate Skyrim's `derived` table for the first time, but holding
off on code until Heavy Armor's matching constant is also sourced (expected to
exist by symmetry, unconfirmed) so both land together rather than one skill at
a time.

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
