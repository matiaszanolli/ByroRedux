# CHARAL — FNV / FO3 character ruleset (data capture)

Living capture of the **Fallout: New Vegas** and **Fallout 3** `CharacterRuleset`
(CHARAL §5) — the **skill-based** Fallout family (SPECIAL + skills + XP), as opposed
to FO4's perk-only model ([charal-fo4-ruleset.md](charal-fo4-ruleset.md)). FO3 and FNV
share ~90 %; per-game deltas are called out. Rows are **BUILT** (already implemented),
**LOCKED** (sourced), or **PENDING**. No guessing ([[feedback_no_guessing]]). Parent:
[charal.md](charal.md).

The SPECIAL→skills auto-calc population already shipped here (#1663,
`actor_value_derive.rs`) — see [[actor_value_population]]. This doc records the full
ruleset around it.

## Attributes — LOCKED

7 SPECIAL, AV codes 5–11 (identical to FO4), EditorIDs `Strength`…`Luck`. Chargen:
each starts at **5**, 40 total (vs FO4's 1+21). Range 1–10.

## Skills — BUILT (auto-calc) + LOCKED (governing table)

13 skills each. **FNV:** Barter, Energy Weapons, Explosives, Guns, Lockpick, Medicine,
Melee Weapons, Repair, Science, Sneak, Speech, Survival, Unarmed. **FO3:** same minus
{Guns, Survival}, plus {Small Guns, Big Guns}.

Governing SPECIAL (per `actor_value_derive.rs::SKILLS`, geckwiki *SPECIAL*):

| Skill | Gov | | Skill | Gov |
|---|---|---|---|---|
| Barter | Charisma | | Repair | Intelligence |
| Energy Weapons | Perception | | Science | Intelligence |
| Explosives | Perception | | Sneak | Agility |
| Guns *(FNV)* | Agility | | Speech | Charisma |
| Lockpick | Perception | | Survival *(FNV)* | Endurance |
| Medicine | Intelligence | | Unarmed | Endurance |
| Melee Weapons | Strength | | Small Guns *(FO3)* | Agility |
| | | | Big Guns *(FO3)* | Endurance |

**Skill base (auto-calc), BUILT:** `skill = 2 + 2·governing + ceil(Luck/2)`
(geckwiki *Derived Skill Settings*; `fAVDSkillBase=2`, `…PrimaryBonusMult=2`,
`…LuckBonusMult=0.5`). Worked: END 5 + Luck 5 → Unarmed 15. Re-confirmed by the
*Speech (FNV)* page (CHA 5 + Luck 5 → Speech 15), same formula.

**Skill *checks* are consumer-side, not ruleset data** — FNV changed them from FO3's
probabilistic "% chance" to a **deterministic threshold**: a dialogue option unlocks iff
`skill ≥ required` (100% success, can't save-scum). That is exactly a `GetActorValue
(skill) ≥ N` condition on the dialogue/quest record — the **already-wired** GetActorValue
path ([[condition_system]], #1663), no new CHARAL machinery. The per-check thresholds
(15/20/25/… on each NPC) are quest data, not ruleset. So CHARAL produces the skill AV;
the scripting/condition layer consumes it — same boundary as Karma/Reputation effects.

## Skill progression (PLAYER) — LOCKED

Three components stack into a player skill value:

**1. Base (chargen)** — `2 + 2·governing + ceil(Luck/2)` from SPECIAL (BUILT, above).

**2. Tag skills** — pick **3**; each gets a flat **+15** (FO3 *and* FNV — the Skill
Rate page: FO3 "no longer increase twice as fast, instead a 15 point increase"; FNV
"function the same as before"). The classic +20 % / 2×-growth model is **FO1/2 only**;
in FO3/FNV a tag is purely +15.

**3. Skill Rate** — points allocated **per level-up** (a.k.a. "Skill Rate", a derived
stat governed by Intelligence):

| Game | Skill Rate / level | Educated perk | Notes |
|---|---|---|---|
| **FO3** | `10 + Intelligence` | **+3**/level | INT 10 → 20 (23 w/ Educated) |
| **FNV** | `10 + Intelligence/2` | **+2**/level (L4+) | odd-INT 0.5 **carries** (INT 9 → 14,15,…); max 17/level |

Sources: fandom *Skill point* + *Skill Rate* (2026-06-29, cross-confirmed). Temporary
INT (apparel/chems) does **not** change Skill Rate — only the base INT at level-up.
This is the skill-based analog of FO4's "SPECIAL-or-perk" point; together with the
base + tag it's the **complete player skill model** for FO3/FNV.

> **NPC auto-calc growth** is a *separate* mechanism (the engine distributes points by
> class skill weights, not free player allocation) and the per-level distribution rule
> is still uncited — but the **+15 tag bonus is now confirmed** and `10+INT`-style is
> the sourced pool-size candidate. Standing [[actor_value_population]] deferral, now
> narrowed to just the NPC distribution rule.

## Derived statistics

| Stat | Gov | FO3 formula | FNV formula | Status |
|---|---|---|---|---|
| Health | END + level | `90 + END·20 + Level·10` | `100 + END·20 + (Level−1)·5` | **LOCKED** (player) |
| Action Points | AGI | `65 + 2·AGI` (cap 85) | `65 + 3·AGI` (cap 95) | **LOCKED** |
| Carry Weight | STR | `150 + 10·STR` | `150 + 10·STR` | **LOCKED** (actor-general) |
| Critical Chance | Luck | `Luck × 1%` (cap 10%) | `Luck × 1%` (Luck>10 inert) | **LOCKED** (`critchance` AV) |
| Melee Damage | STR | `STR × 0.5` | `STR × 0.5` | **LOCKED** (additive bonus) |
| Unarmed Damage | **Unarmed skill** | `ceil((10 + Unarmed)/20)` | same | **LOCKED** (skill-governed) |
| Radiation Resistance | END | `(END−1)·2` (cap 85%) | `(END−1)·2` (cap 85%) | **LOCKED** (actor-general, `RadResist` AV) |
| Poison Resistance | END | `(END−1)·5` (uncapped) | `(END−1)·5` (uncapped) | **LOCKED** (actor-general, hidden, `PoisonResist` AV) |

Health: `fAVDHealthLevelMult` = **10** (FO3) / **5** (FNV); base **90 → 100**. Player
formulas (NPCs derive separately). Source: fandom *Hit Points*.

Carry Weight: `fAVDCarryWeight{Base=150, Mult=10}` — actor-general (NPCs/companions
too), same GMST family as FO4 (`Base=200`) and FO76 (`Mult=5`); FO4 only raised the
base. Source: fandom *Carry Weight*.

Critical Chance: base `Luck × 1%` is the `critchance` AV; the per-hit crit is
`base × weapon Critical Multiplier` (+15 % in VATS) — the weapon multiplier is a combat
-layer factor, not part of the derived AV. FO4's Luck instead drives crit-meter
*recharge* (table-based) — different mechanic, same governing stat. Source: fandom
*Critical Chance*.

Unarmed Damage: `ceil((10 + Unarmed)/20)` — governed by the **Unarmed *skill***, not
STR (the page is explicit: STR drives Melee Damage only). Added to unarmed weapon /
bare-fist damage; VATS doesn't double the bonus. Source: fandom *Unarmed Damage*.

> **Design signal for `DerivedStatFormula` (new):** an FO3/FNV derived stat can be
> governed by a **skill** AV, not just a SPECIAL attribute — so derivation **chains**
> (Unarmed Damage ← Unarmed skill ← `2+2·END+ceil(Luck/2)` ← SPECIAL). The formula's
> *input* must be any AVIF id (attribute **or** skill), and a deriver must resolve the
> skill layer before the stats that depend on it. Adds to the FO4-derived
> kind {Absolute, Multiplier} + scope {player, actor-general} refinements.

Melee Damage: `STR × 0.5` — an **additive** bonus to Melee Weapon damage (VATS doubles
base before STR is added; Unarmed has its own stat above). Cross-game arc: FO1/2
`STR − 5` → FO3/FNV `STR × 0.5` (both additive) → FO4 `1 + STR/10` (multiplier). Source:
fandom *Melee Damage*.

Radiation Resistance: `(Endurance − 1)·2 = 2·END − 2`, **identical FO3==FNV**, capped at
**85 %** (both). Worked: END 5 → 8 %. A `RadResist` derived AV expressed as a percentage
(0–85) — actor-general (governed purely by END, no player-only flag). Armor / Rad-X /
perks add on top via the AV mod layers, *not* the base formula: the **Rad Resistance
perk** = +25 % (FNV, 1 rank) → `permanent_mod`; **Rad-X (FNV)** = `(25 + Medicine/2) %`
(→ 75 % at Medicine 100) → `temporary_mod`, and note its magnitude is itself a small
derived formula **chaining off the Medicine *skill*** (same skill-chain shape as Unarmed
Damage). FO1/2 used `END·2` (out of scope). This is the **resistance half** of the
radiation *affliction family* — the `Rads` pool AV + the poisoning threshold→SPECIAL-
penalty band are the other half (pending the *Radiation poisoning* page). Source: fandom
*Radiation Resistance*.

Poison Resistance: `(Endurance − 1)·5 = 5·END − 5`, **identical FO3==FNV**, the
**Radiation-Resistance twin** — same `(END−1)·k` shape, coefficient **5** instead of 2.
Worked: END 5 → 20 %. **No documented FO3/FNV cap** (so the formula is left uncapped —
don't invent one; FO1/2's 75/95 % caps are out of scope). A **hidden** `PoisonResist`
derived AV (FO3/FNV don't surface it on the Pip-Boy), actor-general. Boosters (Snakeater
/ Tribal Wisdom / gecko-backed armor / antivenoms) layer via the AV mods, not the base
formula. The *poison affliction* mirror of radiation: poison damage also applies a
**temporary SPECIAL debuff** (explicit on the FO4 page), so the same `{pool damage +
resistance AV + SPECIAL-penalty via temporary_mod}` affliction shape covers both — two
members now, confirming it's a reusable family, not a radiation one-off. Source: fandom
*Poison Resistance*.

Action Points: `65 + 2·AGI` (FO3, cap 85) / `65 + 3·AGI` (FNV, cap 95) — same
`fAVDActionPoints{Base,Mult}` GMST family as FO4 (`60 + 10·AGI`) and FO76
(`105 + 10·AGI`); base 65, AGI mult 2→3→10 across the line. Source: fandom *Action
Points*.

✅ **The FNV / FO3 derived table is complete** — all six stats locked (Health, Action
Points, Carry Weight, Critical Chance, Melee Damage, Unarmed Damage). Together with the
locked SPECIAL, skills + governing, auto-calc base, tag + Skill Rate, XP curve and level
caps, **the FNV / FO3 ruleset is fully spec'd** — the second complete Fallout ruleset
alongside FO4.

### Boundary — defensive stats (DT / DR) are equipment AVs, *not* derived

Damage Threshold (FNV) and Damage Resistance (FO3/FNV/FO4) are "derived statistics" in
the wiki sense but are **governed by armor**, base value **0** — they carry **no
SPECIAL/skill formula**, so they are **not** `derived`-table entries. In CHARAL they are
plain actor values: base `0`, value supplied by equipped armor + perks through the
[`ActorValues`] `permanent_mod` layer. The damage-reduction maths (DR % first, then DT
subtraction, floored at 20 % of incoming; FO4 adds per-damage-type reduction curves for
poison / rad / explosive / power-attack / VATS-crit) is a downstream **combat** system
that *consumes* the AV — not part of the character ruleset. The boundary is clean:
**SPECIAL/skill-governed → `DerivedStatFormula`; equipment/perk-modified →
`permanent_mod`.** The DR infobox makes it explicit — **"governed by: None", base 0**
across FO1/2/FO3/FNV/FO4. Sources: fandom *Damage Threshold* + *Damage Resistance*.

[`ActorValues`]: ../../crates/core/src/ecs/components/actor_values.rs

## XP / level curve — LOCKED

```
XP_to_next(L) = 150·L + 50          # 200 at L1, +150/level (both FO3 & FNV)
```

Source: fandom *Level*. Steeper than FO4's `75·L+125`, fitting the hard caps. Same
`a·L+b` shape → `LevelingModel::XpCurve { a:150, b:50 }`.

**Level cap:** FO3 **20** (30 with *Broken Steel*); FNV **30** (50 with the four
add-ons, +5 each). **Perk cadence:** FO3 = 1 perk **every level**; FNV = 1 perk
**every other level** (well-known; not on the pages pulled so far — mark for a citing
pass).

**Level-up reward:** modify **skills** (the Skill Rate points above) + gain a perk on
the cadence — and crucially **SPECIAL is *fixed* after chargen**: FO3/FNV grant **no**
SPECIAL point at level-up (only the Intense Training perk raises one). This is the key
`LevelReward` contrast with FO4, where a level grants **+1 SPECIAL *or* a perk**. Source:
fandom *Level* ("Level up": *"modify skills but not primary statistics … also gain
special perks"*). So `LevelReward` for FO3/FNV = `SkillPoints { base: 10, int_mult: 1.0
(FO3) / 0.5 (FNV), perk_cadence: 1 (FO3) / 2 (FNV) }`, SPECIAL immutable; FO4 =
`SpecialOrPerk`.

## NPC stat storage — NOTE (distinct from FO4)

FO3/FNV predate FO4's `PRPS` `(AVIF, value)` array. Auto-calc-OFF NPCs store explicit
skill/SPECIAL values in their `NPC_` record (DNAM-era layout); auto-calc-ON NPCs are
computed from class base attributes (the #1663 path). This is the FO3/FNV analog of
FO4's "read PRPS / read DNAM Calculated Health" — different wire format, same idea
(stored-or-computed). Full byte layout: a follow-up when wiring FO3/FNV NPC derived
stats.

## Karma — LOCKED (new category: signed, clamped, event-driven reputation AV)

Source: fandom *Karma (Fallout 3)* + *Karma (Fallout: New Vegas)*. **Karma is a stored
ActorValue**, not derived — both pages confirm `player.getav karma`, so it occupies an
AVIF FormID like any SPECIAL or skill. It is the first CHARAL stat that is **signed**,
**clamped at both ends**, and **mutated by world events** (quests/actions) rather than by
a formula. No `END`-style input; the engine just adds/subtracts and re-clamps.

```
karma   : signed int, starts 0, clamped to [-1000, +1000]   (FO3 == FNV, identical)
```

**FO3 and FNV are mechanically IDENTICAL here** — same 2000-point linear scale, same 0
start, **bit-for-bit identical band cut points**. The *only* delta is the title strings
(cosmetic, below). So the Karma band table is a **single shared canonical constant**
across the Fallout family, not a per-game one.

**Five bands (threshold table), shared FO3 == FNV cut points:**

| Band       | Range            |
|------------|------------------|
| Very Good  | +750 … +1000     |
| Good       | +250 … +749      |
| Neutral    | −249 … +249      |
| Evil       | −250 … −749      |
| Very Evil  | −1000 … −750     |

The bands are the gameplay-load-bearing part: condition functions (`GetIsKarmaType` /
the `[Karma]`-tagged speech checks, companion-recruitment gates — Clover/Jericho need
Evil, Cross/Fawkes need Good, RL-3/Butch neutral) test **which band** the value falls
in, not the raw number. So CHARAL needs a `karma_band(value) -> KarmaBand` classifier
backed by an ordered `[(lower_bound, band)]` table — a *reputation* analog of the
derived-stat table, but a pure lookup (no SPECIAL inputs).

**Karma titles** (the 30-row × {Good/Neutral/Bad} string grid — FO3 "Vault Guardian" …
"Messiah", FNV "Samaritan" … "Messiah") are **UI cosmetic only**: title-row = a
character-level bracket, column = the band above. They drive no gameplay (HUD/Pip-Boy
display string), so CHARAL records them as presentation data, not ruleset logic. The
grids differ per-game (different strings, FNV caps rows at "30+", FO3 needs *Broken
Steel* past row 20) — but since they're cosmetic, that per-game difference is a string
table, not ruleset logic.

**FNV companion gating is softer than FO3** — FNV recruits no companion *by* Karma band
(FO3 hard-gated Clover/Jericho=Evil, Cross/Fawkes=Good). The one FNV case (Cass leaving
at Karma ≤ −250 after warnings at −100/−150) is **scripted dialogue keyed off raw Karma
thresholds**, i.e. condition/quest data — reinforcing that band→effect lives in the
scripting layer, not CHARAL.

**Point grants are game-data, not engine rules** — every "+50 quest good act / −100 kill
non-evil / ±1000 Megaton" entry is authored on the quest/script/perk record (a Reward or
script `RewardKarma`), so it flows through the *scripting* runtime, **not** a
`DerivedStatFormula`. CHARAL owns only: the AV slot, the clamp bounds (`fKarmaMod*` /
`iKarma*` GMSTs — to be GMST-sourced), and the band classifier.

**Design impact:** Karma generalises CHARAL beyond "stat = base+mods or derived". It adds
a **reputation family** = `{ clamped signed AV + ordered band table }`. Clamp/band bounds
are AUTHORED (GMST), the band→effect wiring is condition/quest data.

**FNV Reputation is a DISTINCT sub-shape — correction to an earlier assumption.** The FNV
page states Fame and Infamy "cannot be lost (which can lead to a 'Mixed' reputation)",
whereas Karma moves both ways. So FNV per-faction Reputation is **NOT** a single signed
clamped AV like Karma — it is **two independent *monotonic* (non-decreasing) accumulators
per faction** (Fame AV + Infamy AV), and the faction standing band is a **2-D
classification of the (Fame, Infamy) pair** (Mixed = both high). That is a *second*
reputation sub-family:

- **Karma** = 1 signed clamped AV → 1-D band lookup.
- **FNV Reputation** = (Fame, Infamy) monotonic pair per faction → 2-D band lookup;
  "Mixed" is the diagonal cell that Karma's single axis cannot express.

Both still belong to the reputation *family* (an AV + a band classifier, effects in the
scripting layer), but the classifier arity differs. Full spec below.

## FNV Reputation — LOCKED (the 2-axis reputation variant)

Source: fandom *Fallout: New Vegas reputations*. Confirms the 2-axis monotonic model and
gives every constant.

**Two AVs per faction, both monotonic (offset-only):** Fame (console **1**) + Infamy
(console **0**). `player.addreputation <factionFormID> <0|1> <editorInt>` adds points;
neither pool can decrease (only be out-weighed). Resets (NCR/Legion/Freeside story beats,
faction-armor *temporary* disguise) are scripted exceptions, not a decrement op.

**Condition functions** (mirror Karma's `GetIsKarmaType`): `GetReputation` = raw pool
value; **`GetReputationThreshold`** = the Range 0–3 band. Gameplay/dialogue reads the
**threshold**, not the raw value (the page even documents a vanilla bug where Mick/Ralph's
discount script wrongly used `GetReputation` instead of `GetReputationThreshold`). So the
band classifier is the gameplay-load-bearing output, exactly as with Karma.

**Bump-magnitude lookup — ENGINE-SUPPLIED shared constant.** The editor stores a 1–5
"bump type"; the engine maps it to points via a fixed non-linear table:

| Editor int | 1 | 2 | 3 | 4 | 5 |
|------------|---|---|---|---|---|
| Points     | 1 | 2 | 4 | 7 | 12 |

(Very Minor → Very Major.) `addreputation … 5` adds 12. This `[_,1,2,4,7,12]` table is a
single shared constant, not per-faction.

**Per-faction threshold arrays — AUTHORED.** Each axis (Fame *and* Infamy, independently)
crosses Range 0→1→2→3 at these per-faction minimums (one array, applied to both axes):

| Faction / Settlement | R0 | R1 | R2 | R3 |
|----------------------|----|----|----|----|
| Boomers              | 0  | 8  | 25 | 50 |
| Brotherhood of Steel | 0  | 3  | 10 | 20 |
| Caesar's Legion      | 0  | 15 | 50 | 100|
| Followers of the Apocalypse | 0 | 8 | 25 | 50 |
| Great Khans          | 0  | 5  | 15 | 30 |
| Powder Gangers       | 0  | 5  | 15 | 50 |
| NCR                  | 0  | 12 | 40 | 80 |
| White Glove Society  | 0  | 2  | 5  | 10 |
| Freeside             | 0  | 11 | 35 | 70 |
| Goodsprings          | 0  | 3  | 8  | 15 |
| Novac                | 0  | 3  | 10 | 20 |
| Primm                | 0  | 5  | 15 | 30 |
| The Strip            | 0  | 6  | 20 | 40 |

These are per-faction AUTHORED data (live on the faction's record in the ESM; hardcode
only as a fallback, GMST/record-source them like the rest of CHARAL).

**Title = a 4×4 canonical grid** of (Fame range × Infamy range) → 16 standing titles
(shared across all factions; positive=green, mixed=black, negative=red):

| Infamy ↓ \ Fame → | 0 | 1 | 2 | 3 |
|-------------------|---|---|---|---|
| **0** | Neutral | Accepted | Liked | Idolized |
| **1** | Shunned | Mixed | Smiling Troublemaker | Good-Natured Rascal |
| **2** | Hated | Sneering Punk | Unpredictable | Dark Hero |
| **3** | Vilified | Merciful Thug | Soft-Hearted Devil | Wild Child |

"Mixed" is the (1,1) diagonal cell — the standing a single signed axis (Karma) cannot
express, which is exactly why Reputation needs two axes.

**CHARAL model for the reputation family (now fully general):**

| Instance        | Scope          | Axes | Classifier       | Decrement? |
|-----------------|----------------|------|------------------|------------|
| Karma           | global         | 1 (signed) | 1-D band     | yes (clamped ±1000) |
| FNV Reputation  | per-faction    | 2 (Fame,Infamy) | 4×4 grid | no (monotonic, offset-only) |
| FO4 affinity    | per-companion  | 1 (signed) | threshold→perk | yes |

All three reduce to `{ one-or-two AVs + a band/grid classifier }`, with point grants and
band→effect wiring in the scripting/quest layer. CHARAL owns the AV slot(s), the
threshold/grid data (AUTHORED per-faction, shared grid), and the classifier — never the
effects. The `[_,1,2,4,7,12]` bump table is the one engine-supplied numeric constant.
