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
`…LuckBonusMult=0.5`). Worked: END 5 + Luck 5 → Unarmed 15.

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

Action Points: `65 + 2·AGI` (FO3, cap 85) / `65 + 3·AGI` (FNV, cap 95) — same
`fAVDActionPoints{Base,Mult}` GMST family as FO4 (`60 + 10·AGI`) and FO76
(`105 + 10·AGI`); base 65, AGI mult 2→3→10 across the line. Source: fandom *Action
Points*.

✅ **The FNV / FO3 derived table is complete** — all six stats locked (Health, Action
Points, Carry Weight, Critical Chance, Melee Damage, Unarmed Damage). Together with the
locked SPECIAL, skills + governing, auto-calc base, tag + Skill Rate, XP curve and level
caps, **the FNV / FO3 ruleset is fully spec'd** — the second complete Fallout ruleset
alongside FO4.

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

## NPC stat storage — NOTE (distinct from FO4)

FO3/FNV predate FO4's `PRPS` `(AVIF, value)` array. Auto-calc-OFF NPCs store explicit
skill/SPECIAL values in their `NPC_` record (DNAM-era layout); auto-calc-ON NPCs are
computed from class base attributes (the #1663 path). This is the FO3/FNV analog of
FO4's "read PRPS / read DNAM Calculated Health" — different wire format, same idea
(stored-or-computed). Full byte layout: a follow-up when wiring FO3/FNV NPC derived
stats.
