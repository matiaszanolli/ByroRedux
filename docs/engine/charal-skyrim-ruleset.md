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

**`BaseCost` closed** (source: UESP *Skyrim Mod:Mod File Format/INGR*,
2026-07-04 — a binary record-format spec, not a gameplay page, fetched to
resolve the dangling "calculated according to the formula listed here"
reference above):
```
effect_base_cost = auto-calc'd from the MGEF's own EFIT struct via:
  effect_cost = effect_base_cost_constant × (Magnitude × Duration / 10)^1.1
  (Magnitude < 1 treated as 1; Duration = 0 treated as 10)
```
Record-format detail, not a new gameplay formula — closes the reference, does
not add a new derived stat. `INGR` (and the shared `MagicItem` struct this
formula lives on) is already parsed in `crates/plugin/src/esm/records/items.rs`,
so this doesn't surface a parser gap either.

## Disease — a genuinely different mechanism from the Fallout affliction family

Source: UESP *Skyrim:Disease*, 2026-07-04. Skyrim disease is the natural place
to expect the affliction-family pattern (`crates/core/src/character/
affliction.rs`, built for FO3/FNV/FO4/FO76 Radiation/Poison/Disease
Resistance) to reapply — **it doesn't**. Noted explicitly in `resistance.rs`'s
module doc so nobody reuses `AfflictionTable` here without redesigning it:

- **No pool, no threshold bands.** Each disease (Ataxia, Bone Break Fever,
  Rockjoint, …) is a discrete **binary** status — caught or not — carrying a
  **fixed** flat percentage penalty (e.g. Ataxia: "picking locks and
  pickpocketing 25% harder"). There's no accumulating damage value crossing
  escalating thresholds the way Rads/poison do in Fallout.
- **Survival Mode's escalation is a 3-rung state machine, not a continuous
  pool.** Untreated diseases progress Normal → Severe → Crippling after a
  fixed 24 real/game hours, each rung a **fixed** percentage (typically
  25%/50%/75%) — discrete state transitions gated by elapsed time, not
  `reevaluate_affliction`'s continuous-value diff.
- **Resistance is flat immunity, not Endurance-derived.** Argonian/Bosmer
  have a constant 50% Resist Disease; werewolves/vampires get 100% (and are
  mutually exclusive as a result — a werewolf can never become a vampire).
  No attribute feeds a resistance formula the way FO3/FNV's `(END−1)·k` does.
- **No numeric infection-chance formula given** on this page — contact with a
  disease-carrying creature/trap presumably rolls a chance, but no percentage
  or formula is stated.

If Skyrim disease is ever modelled, it needs its **own** mechanism (a
discrete status-effect marker + fixed-penalty table, optionally a 3-state
escalation timer for Survival Mode) — not a reuse of the pool/threshold
`AfflictionStatus`/`AfflictionTable` pair. Not built — no consumer system
(status effects / potions) exists yet to wire it into, same reasoning as
every other "real mechanism, no consumer yet" item this session.

## Vampirism stage progression — a DIFFERENT shape again, closer to the affliction family than Disease was

Source: UESP *Skyrim:Vampirism*, 2026-07-04. Unlike Disease immediately
above, Vampirism's 4-stage progression is **quantitatively clean** and
worth full capture:

```
Stage advances +1 per 24 real/game hours without feeding on a sleeping NPC.
Feeding always resets to Stage 1.  Stage ∈ {1,2,3,4} (no Stage 5+ — caps at 4).

Weakness to Sunlight (Health/Magicka/Stamina penalty while outside 5am–7pm):
  Penalty = −15 × Stage        (Stage 1→−15, 2→−30, 3→−45, 4→−60 — clean affine)

Resist Frost / Weakness to Fire, vanilla:    25 × Stage %        (25/50/75/100)
Resist Frost / Weakness to Fire, Dawnguard:  10 + 10 × Stage %   (20/30/40/50)
Resist Disease / Resist Poison:              flat 100% at every stage (not stage-scaled)

Vampiric Drain (Destruction power):
  HealthDrain/sec = Stage + 1        (2/3/4/5 — clean affine)
  MagickaCost/sec = 6/10/13/17       (NOT affine — deltas 4,3,4, a near-linear table)
```

**This is structurally much closer to the affliction family's shape than
Disease was**: a monotonic "time since last reset" value (hours since
feeding) crossing fixed thresholds (24/48/72/96h) that gate escalating flat
penalties, reset to zero on a trigger event (feeding) — that's exactly
`AfflictionTable`'s `{pool + bands + reset}` shape, just with the pool being
**elapsed time**, not accumulated damage. `AvPenalty{avif, delta}` already
models "−15×Stage to Health/Magicka/Stamina" as one `AvPenalty` entry per
pool AV per band. The one repurposing this would need: `ActorValue::damage`
is normally populated by `apply_damage()` from combat hits, not a periodic
time-tick — using it for "hours since feeding" would need a new system that
increments it once per game-hour (via the existing `TotalTime`/`DeltaTime`
resources) rather than reusing combat's damage-application path. Not
contradicting the Disease finding above — they're genuinely different
Skyrim status mechanisms (Disease: fixed binary status; Vampirism: a
monotonic timer crossing bands), and this one plausibly **does** fit
`affliction.rs`'s existing mechanism with a time-tick adapter, unlike Disease.
Not built — flagging as a stronger build candidate than Disease specifically
*because* the mechanism shape matches, but still no consumer system (a
feed/reset trigger, a day/night cycle check for the sunlight penalty) exists
yet to justify it.

**Cross-game confirmed 2026-07-04**: Oblivion's Vampirism (`charal-oblivion-
ruleset.md`) uses the exact same 24h-per-tier, feed-resets mechanism shape,
independently — this is now a two-game pattern, not a Skyrim one-off.

## Weapon skill → damage — RESOLVED: it's a perk-rank step table, not a smooth multiplier

Sources checked 2026-07-04: UESP *Skyrim:Combat*, *Skyrim:Damage*,
*Skyrim:Two-handed*, *Skyrim:One-handed*, and now *Skyrim:Archery* — the
fifth confirms the mechanism directly and closes the thread. *Skyrim:Archery*
states plainly: "the trajectory and speed of arrows are independent of a
character's skill level in Archery; only damage increases with skill level."
That sounded at first like the smooth multiplier the first four pages
couldn't locate — but the Skill Perks section reveals the actual mechanism:

```
Overdraw (5 ranks, unlocked at Archery 0/20/40/60/80):
  Rank 1 (Archery ≥0):  Archery damage ×1.2
  Rank 2 (Archery ≥20): Archery damage ×1.4
  Rank 3 (Archery ≥40): Archery damage ×1.6
  Rank 4 (Archery ≥60): Archery damage ×1.8
  Rank 5 (Archery ≥80): Archery damage ×2.0
```
This is a **discrete 5-step table keyed to skill thresholds**, structurally
identical to the Vampire Lord/Werewolf **level**-bracket tables already
documented above — just bracketed by **skill** instead of **level**. It
confirms, rather than contradicts, the conclusion from the previous four
pages: there is **no intrinsic smooth `×(1+k·Skill)` term** for weapon
damage (unlike Armor Rating/Alchemy/Enchanting) — "damage increases with
skill level" in Archery's case means *exactly* "you unlock Overdraw's next
rank," a perk-gated step function, not a continuous curve. One-Handed's
`Armsman` and Two-Handed's `Barbarian` are presumably the same shape
(5-rank, matching thresholds, matching multipliers — not independently
re-verified here, but named in earlier pages as direct parallels to
Overdraw). **A sixth confirmed instance of the bracketed-step-table shape**
this session (alongside Vampire Lord Damage/Resist, Werewolf Claws Damage/
Armor) — just the first one bracketed by skill rather than level. Not built
— `DerivedStatFormula` doesn't model step tables, same reasoning as the
Vampire Lord/Werewolf tables above, and this is squarely a perk-effect table
rather than a CHARAL derived stat regardless.

## Vampire Lord — LOCKED, a fourth distinct "stats scale with level" shape

Source: UESP *Skyrim:Vampire Lord*, 2026-07-04. The Dawnguard transformation
form built on top of ordinary Vampirism (above). Its Health/Magicka/Stamina/
Damage/Resist bonuses scale with **player level**, but as a **9-bracket step
table**, not a continuous formula — a genuinely different shape from
everything else level-derived found so far (FO4's continuous
`bias+coeff·Level` Health formula, Skyrim's own `+10/level` pool-pick,
Vampirism's `24h-tier` timer):

```
Level bracket:  1–10 / 11–15 / 16–20 / 21–25 / 26–30 / 31–35 / 36–40 / 41–45 / 46+
Damage bonus:    +0  /  +5   / +10   / +15   / +20   / +25   / +30   / +35   / +40
Health bonus:   +50  / +75   /+100   /+125   /+150   /+175   /+200   /+225   /+250
Magicka bonus:  +40  / +60   / +80   /+100   /+120   /+150   /+160   /+180   /+200
Stamina bonus:   +0  / +10   / +20   / +30   / +40   / +50   / +60   / +80   /+100
Resist Damage:  100  /125    /150    /175    /200    /225    /250    /275    /300
```
Note the bracket widths aren't uniform (the first spans levels 1–10, the rest
span 5 each) — this **cannot** be expressed as `DerivedStatFormula`'s
continuous affine/bilinear shape; it would need a lookup-table deriver, a
structurally different mechanism (same family as the FO4 companion
level-list HP snapshots noted in `charal.md` §7.1, not the smooth-formula
family). Vampiric Drain's health-drain-per-second and Raise Dead's reanimate
duration follow the same level-bracket table, not shown in full here.

**Perk-cost progression is a clean arithmetic sequence, unlike the stat
table**: the Vampire Lord skill tree's kill-count-to-unlock-next-perk starts
at 5 and increases by 2 each time (`5, 7, 9, 11, …`), i.e.
`KillsToUnlock(n) = 2n + 3` for the nth perk (1-indexed) — an affine formula,
just over "perk index" rather than a character AV.

Not built — this is a Dawnguard-DLC transformation-form mechanic with no
consumer system (no transformation/form-swap mechanic exists in the engine),
same reasoning as every other "real data, no consumer" item this session.

## Lycanthropy (Werewolf) — same level-bracket table shape, confirms the perk-cost formula cross-mechanism

Source: UESP *Skyrim:Lycanthropy*, 2026-07-04. Skyrim's other monstrous
transformation, structurally parallel to Vampire Lord but with real
differences worth keeping straight:

```
Werewolf Claws (Beast Form), 9-bracket step table — SAME bracket edges as Vampire Lord:
Level bracket:  1–10 / 11–15 / 16–20 / 21–25 / 26–30 / 31–35 / 36–40 / 41–45 / 46+
Damage bonus:    +0  /  +5   / +15   / +25   / +30   / +35   / +40   / +50   / +60   ← irregular deltas (5,10,10,5,5,5,10,10), NOT clean
Armor Rating:     0  /  50   / 100   / 150   / 200   / 250   / 300   / 350   / 400   ← clean +50/bracket (DG only)
```
Unlike Vampire Lord's Damage column (clean `+5/bracket`), Werewolf's Damage
deltas are irregular (5,10,10,5,5,5,10,10) — genuinely hand-tuned, not a
formula, so don't force-fit it. Armor Rating **is** clean (`50×bracket`),
same step-table shape as Vampire Lord's Resist Damage (`100+25×bracket`) —
different base/slope, same mechanism, now confirmed used for **two**
transformation forms, not a Vampire-Lord one-off.

**Base Beast Form bonuses are flat, not level-scaled** (contrast the
level-bracketed Claws table above): Health +50, Stamina +100 (+150/+200 with
Ring of the Hunt), stamina regen 5%→20% of total, Carry Weight +2000, base
unarmed damage 4→20, reach 96→150 — all flat constants regardless of level.

**No time/feeding-based stage-escalation system, unlike Vampirism.** This is
the clearest mechanism-shape divergence between Skyrim's two monstrous
transformations: Vampirism/Vampire Lord penalizes you more the longer you go
*without* feeding (a real timer crossing thresholds); Werewolf/Beast Form has
no equivalent — transformation is simply time-limited per use (once/day, or
unlimited with the Ring of Hircine) with no in-form escalating penalty. Feeds
only reset the *skill tree progress* counter, not a stage/hunger state.
Becoming a werewolf cures all diseases including any vampirism stage on
first transformation — mutually exclusive with vampirism, confirmed from
both the vampire pages and here (third independent confirmation).

**Perk-cost formula is IDENTICAL to Vampire Lord's, confirmed exactly**: "the
first perk requires five feedings; after that, each new perk requires two
more feedings than the last" — the same `KillsToUnlock(n) = 2n+3` sequence
(5,7,9,...) as Vampire Lord's kill-count formula, now a **confirmed
cross-mechanism pattern**, not independently reinvented per form. The page's
own total — "165 feedings to complete the entire tree" (11 perks) — is an
exact match: `Σ(n=1..11) (2n+3) = 132+33 = 165`. Strong validation of the
formula via an independently-stated total.

Not built — same DLC/no-consumer-system reasoning as Vampire Lord.

## Unarmed Combat — confirms no Hand-to-Hand skill exists, cross-validates two level-bracket tables

Source: UESP *Skyrim:Unarmed Combat*, 2026-07-04. Explicit confirmation:
"Unarmed Combat does not have its own skill tree and cannot be developed
like other skills" — Skyrim genuinely dropped Hand-to-Hand as a skill
(matches `SkillSet::SKYRIM`'s 18-skill roster, no Hand-to-Hand entry). Base
unarmed damage is a flat per-race constant (4 for Men/Mer, 10 for Khajiit/
Argonian) that **does not scale with level or any skill at all** — not the
same "weapon skill has no formula" question from the previous section (there
weapon skills exist but no smooth term was found; here there's no skill AV to
begin with), but the same practical shape: another flat/perk-only Skyrim
combat stat with zero smooth-scaling term.

**Independently cross-validates two already-documented level-bracket
tables** via numeric restatement: "Vampire Claws increase unarmed damage by 5
for every five levels [starting at 11]... maximum of 40 points at level 46"
matches the Vampire Lord Damage column exactly (`+0..+40` in `+5`
increments); "[werewolf] bonuses... from 0 at level 10 or less to 60 at level
46" matches the Werewolf Claws Damage column's endpoints exactly (`+0` →
`+60`). Both transcriptions confirmed correct by an independent source page.

Also notes `Bestial Strength` (Werewolf perk, up to 4 ranks) as a flat
`×1.25/1.5/1.75/2.0` damage multiplier depending on rank taken — a clean
perk-rank arithmetic sequence (`1 + 0.25×rank`), not skill-derived, same
"perk table, not a formula" bucket as everything else perk-gated this
session. No CHARAL/build action — purely confirmatory + minor flavor data.

## Magicka — confirms the built pool model, surfaces two new items

Source: UESP *Skyrim:Magicka*, 2026-07-04. **Directly confirms the already-
built model**: "each time you level up your character, you may add ten
points of magicka" — matches `SKYRIM_POOL_BASE=100` + `LevelingModel::SKYRIM
.pool_pick_gain()=10` (`skyrim.rs`) exactly. Also confirms High Elves get
+50 Magicka over other playable races — a flat race bonus **not currently
modelled** (Skyrim's `AttributeSet` is empty by design; this would need a
per-race bonus table, not an attribute-derived formula — same shape as
Oblivion's per-race skill starting bonuses already captured elsewhere).

**New PENDING item**: "the cost of a spell goes down as you improve the
skill corresponding to its school" — a genuine skill-derived formula (spell
Magicka cost as a function of the relevant Magic school skill), but this
page only gives two worked *endpoints* (a 1426-cost spell → 424 at skill 100
+ perk; a dual-cast 2344-cost spell → 696 at skill 100 + perk) without the
intermediate curve. The actual formula lives on the page this one links to,
*Skyrim:Magic Overview* § Spell Cost — not fetched this session. Worth
checking: this is the Magic-school analog of the weapon-damage question
just resolved, and could turn out either smooth (contra weapon damage) or
perk-stepped (like Overdraw) — genuinely unknown until that page is read.

**NPC Magicka is a 3-part composition, a new shape**: `race base (0-200,
players start 50) + per-NPC fixed adjustment (-50 to 20000, mostly 0) +
0-10/level from class` — structurally closer to the FO4 companion
level-list HP snapshots (`charal.md` §7.1, a per-entity table) than to any
clean formula; not pursued further as a build target, just recorded so an
eventual NPC Magicka deriver doesn't get modelled as a clean attribute
formula when it's actually race+override+class composition.

## Relationship Rank (Disposition) — a fifth reputation-family instance, explicitly SPEECH-DECOUPLED

Source: UESP *Skyrim:Disposition*, 2026-07-04. A genuine architectural break
from earlier games worth flagging explicitly: "the Disposition stat is no
longer visible to the player **and cannot be affected by performing
speechcraft**." Skyrim replaced whatever continuous, SPECIAL/Speech-driven
Disposition score older TES games used with a **discrete 9-value
Relationship Rank**, changed **only** by quest/favor completion — not by any
skill, attribute, or dialogue check:

```
Rank:  -4         -3      -2   -1     0             1        2          3      4
Name:  Archnemesis Enemy   Foe  Rival  Acquaintance  Friend   Confidant  Ally   Lover
Theft threshold (max item value takeable, not counted as theft):
                                              —        25       50        100    500
```
Same `{ score + band classifier → gameplay effect }` shape as the reputation
family (Karma / FNV Reputation / FO4 Affinity / Starfield Affinity), making
this a **fifth instance** — but the coarsest one by far: a 9-value discrete
scale (not a wide continuous range), changed by direct quest-scripted
rank-sets (`=1`, `0→1`, `min 1`, `=-1` — explicit target/floor/ceiling
operations, not additive deltas like FO4's `±15/±35` Affinity reactions), and
**zero skill/attribute influence of any kind** — the first reputation-family
instance that's entirely disconnected from CHARAL's AV substrate. This
matters for scope: Skyrim's already-documented Persuade/Intimidate/Bribe
checks (above) do **not** feed into this rank at all — they're one-off
dialogue-success gates with no accumulating reputation consequence, fully
decoupled from Relationship Rank. Combat allegiance (Ally/Friend vs. hostile)
is also gated off this same rank via NPC faction, not a separate stat.

Not built — no per-NPC relationship storage or quest-scripting-trigger system
exists yet; same "real family member, no consumer" reasoning as the rest of
the reputation family before their own components got built.
