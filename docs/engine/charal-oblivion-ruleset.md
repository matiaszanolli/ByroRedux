# CHARAL — Oblivion character ruleset (data capture)

Living capture of **Oblivion** gameplay-system formulas that sit downstream
of the already-complete core ruleset (`AttributeSet::OBLIVION`,
`SkillSet::OBLIVION`, `LevelingModel::OBLIVION`, `oblivion_attribute_bonus`,
`oblivion_health_formula` — all shipped, see `crates/core/src/character/
tes.rs` and `charal.md` §5, which calls Oblivion "CHARAL-complete"). This doc
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

Skill-gated spell-tier access (Novice/Apprentice/Journeyman/Expert/Master —
same 5-tier structure as every other Oblivion skill) and a flat 3 XP per
successful cast are standard patterns already established elsewhere, not
new. Not a CHARAL derived stat (spell-casting economy, not a character AV),
same routing as every other magic-cost/persuasion/barter formula this
session — documented for completeness, not a build target.

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
Lockpicking/Sneak Detection in `charal-skyrim-ruleset.md`. Documented for
completeness, not a CHARAL build target.

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
input affine, no Luck term), so it's wired the same way as Skyrim's Light
Armor Rating: `ARMOR_RATING_SKILL_COEFF`/`ARMOR_RATING_SKILL_BIAS` in
`crates/core/src/character/tes.rs`, two rows in `oblivion_ruleset()` (one
each for `LightArmor`→`LightArmorRating` and `HeavyArmor`→
`HeavyArmorRating`, both `Multiplier`-kind, `ActorGeneral` scope since the
source doesn't distinguish player/NPC here unlike Skyrim's). Test asserts
both worked values (Light 50 → 0.675, Heavy 20 → 0.48).

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
these by name once GMST parsing lands (CHARAL §8 item 6), not re-hardcode the
numeric constants captured here.

## Fatigue — base pool cross-checked (already BUILT), regen formula + Remastered divergence new

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
model). No such tick/regen system exists yet for any pool (Health/Magicka/
Fatigue all lack regen in the engine today) — recording this as the first
concrete number for that future system, not routing it into the existing
derived-stat table.

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
combat — same "new per-second rate, no tick-consumer exists yet" bucket as
Remastered FatigueRegen; a second data point for that same future system, not
a second gap.

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

## Magicka — base formula cross-checked; the third and most interesting regen policy; first quadratic formula found

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
data point. Still the same "rate for a future tick-consumer, not a
`derived_value` row" bucket as the other two pools' regen formulas — no code
yet for any of the three.

**Stunted Magicka is a binary gate on this formula**, not a formula itself:
the Atronach birthsign, the *Astral Vapors* disease (cross-references the
Disease section above — a disease *can* apply a stat-system status effect,
not just flat attribute drains as previously characterized there; worth a
mental amendment, not a doc correction, since Astral Vapors was already
listed as one of the two Oblivion diseases with escalating/special behavior),
and one Shivering Isles item all set a flag that **zeroes regen entirely**
regardless of Willpower/MaxMagicka. A status-effect boolean multiplying the
regen formula to 0, same shape as a `temporary_mod` gate elsewhere in CHARAL.

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
