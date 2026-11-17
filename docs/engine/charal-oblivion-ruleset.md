# CHARAL — Oblivion character ruleset (data capture)

Living capture of **Oblivion** gameplay-system formulas that sit downstream
of the already-complete core ruleset (`AttributeSet::TES_CLASSIC` — shared
with Morrowind, hence the name rather than a per-game one —
`SkillSet::OBLIVION`, `LevelingModel::OBLIVION`, `oblivion_attribute_bonus`,
`oblivion_health_formula` — all shipped, see `crates/core/src/character/
tes.rs` and `charal.md` §5, which describes the Oblivion ruleset **builder**
as complete but unwired — `CharacterRulesProfile::OBLIVION` is
`RulesetBuilder::None`, and `Oblivion.esm` authors no `AVIF` records at all,
so wiring needs a legacy actor-value resolver first). This doc
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
`boot/` references) — on Oblivion it no-ops today because `PoolRegenConfig`
(the per-game resolved AVIF ids) is inserted by no Oblivion path (FO3/FNV install
it for their Action Points lane only, #6350): the
`oblivion_pool_regen_config` constructor that once lived in `tes.rs` was
deleted under #3848 (`git show e13985dfc` recovers it), and re-adding it
is gated on Oblivion's ruleset actually being wired — deliberately
blocked on the pre-AVIF actor-value resolver (#3768), not on "a live
`CharacterRuleset` landing" (a ruleset alone never installs the config,
because the Fatigue/Magicka lanes are Oblivion-shaped; FO3/FNV install it
for the Action Points lane only).
So the Oblivion lanes are wired and tested but have no live consumer until that
resolver lands. 5
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
the source is silent on a player/NPC split there. Oblivion has no wired ruleset
today, so nothing observes the choice; it matters when #3768 lands. Pinned by
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
but no status-effect component exists anywhere in the engine yet to carry
this flag, so `pool_regen_tick_system` always passes `false` today — a known,
documented gap (module docs in `regen.rs`), not a silent omission.

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

Oblivion uses skill-use leveling rather than character XP: a level becomes
available after 10 increases in major skills. At level-up, each selected
attribute receives a +1 to +5 bonus based on governed major-skill increases
(0, 1–4, 5–7, 8–9, and 10+). The engine's `fSkillUseCurve` is 1.95 for the
skill-use progression curve. Source: [UESP Oblivion:
Leveling](https://en.uesp.net/wiki/Oblivion:Leveling).

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
`oblivion_profile_refreshes_the_players_magicka_and_keeps_health_and_fatigue`.

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
stored term is the accumulation, and the formula term is separate. **Still not settled, and it conflicts
with UESP:** at the placeholder attributes a literal sum gives Magicka 0 + 100 = 100 (agrees), but Fatigue
150 + 150 = 300 and Health 45 + 80 = 125, where UESP gives `Str + End + Agi + Wil` and `2 x End`. For both to
hold the player's stored pools must be 0 when a new game starts, i.e. something clears the `Player`
record's 45 / 150 first. No clearing writer is identified yet; the callers of the pool setters (`0x519c50`
Health, `0x467290` Magicka, `0x4672b0` Fatigue: `0x5115ff` / `0x511617` / `0x51162c`, `0x51a025` /
`0x51a036` / `0x51a047`, the calculator, `0x5240bc`..`0x5240ce`, and the level-up) were listed, and only
the calculator and the level-up were read. Magicka is 100 under every one of these
readings, which is why it is applied (#7037); Health is `80 + accumulated` or `45 + 80`.

Not run: the `Player` record is also what a new game starts from before character creation assigns the
class, race and birthsign (OAV-NG-001 puts those menus out of scope), so the placeholder may be what the
original shows until chargen. That is unobserved.

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
(`0x52248d`, `0x5225c2`); the rounding mode is the process control word,
which was not checked (round to nearest is the default, so this is the
assumption until measured).

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
nonzero (`0x522689`). The calls at `0x44733a`, `0x50c77c`, `0x50cf9d` and
`0x523ff4` push 0 (full recalculation); the calls at `0x596ae5`, `0x5c29e6`
and `0x5c2ac0` push 1 (attributes and skills only); `0x50ab73` was not
resolved.

**Triggers.** Two independent tests reach the calculator:

- Bit 4 of `ACBS` flags ("Auto-calc stats"): the NPC load path calls the NPC
  vtable slot `0xe4` (`0x519d40`, test at `0x519d46`) and then the calculator
  at `0x523ff4`.
- Bit 7 ("PC Level Offset"): the walker `0x447300` runs the calculator on
  every NPC with the bit set (`0x44732b`–`0x44733a`). Its callers: the
  data-load tail of the program start (`0x40df80`, calls at `0x40ebaa`,
  `0x40f45d`), the savegame load (`0x465860`, call at `0x466990`) and
  `0x66c3e0` (call at `0x66c551`, after a store through `0x519c50`; its
  only caller is `0x5ac724`, which fits a player level-up and is inferred
  from the call shape).

The engine therefore treats an NPC with either bit as auto-calc. Which of the
two the editor's own controls map to, and the result of each of the four flag
combinations over spawn, level change and reload, is the observation in #7022.
Eight direct calculator call sites exist (`0x44733a`, `0x50ab73`, `0x50c77c`,
`0x50cf9d`, `0x523ff4`, `0x596ae5`, `0x5c29e6`, `0x5c2ac0`); the functions
holding `0x50ab73`, `0x50c77c`, `0x50cf9d`, `0x596ae5`, `0x5c29e6` and
`0x5c2ac0` have no direct caller (command table or vtable) and were not
decoded.

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
