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
