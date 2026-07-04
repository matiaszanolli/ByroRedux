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
