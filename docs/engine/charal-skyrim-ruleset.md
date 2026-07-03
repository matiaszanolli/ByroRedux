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
