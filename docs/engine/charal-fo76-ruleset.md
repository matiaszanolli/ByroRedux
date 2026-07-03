# CHARAL — FO76 character ruleset (data capture)

Living capture of the **Fallout 76** `CharacterRuleset` (CHARAL §5), sourced
2026-07-03 from the fandom *Fallout 76 SPECIAL* page (`api.php?action=parse` — direct
WebFetch 402s on fandom, see [[feedback_no_guessing]]). Every row is **LOCKED**
(sourced) or **PENDING**. Parent design: [charal.md](charal.md). Not yet in the
CHARAL §8 rollout order (implied to reuse the FO4 population path — same AVIF/no-skill
shape — but its own derived-stat constants, confirmed below, are *not* identical to
FO4's).

## Attributes — LOCKED

Same 7 SPECIAL, same order, same EditorIDs as FO3/FNV/FO4 — `AttributeSet::FALLOUT`
covers FO76 with no changes. Two things are FO76-specific:

- **Base range differs**: leveling grants 1 SPECIAL point per level up to level 50,
  capping **base** allocation at **56 points total** (max 15 in a single stat via
  leveling; Legendary SPECIAL perks can push a further +30 points/+86 total).
- **Effective range 1–100**: temporary buffs/debuffs can push an individual stat to
  100 or down to 1 (patch 1.1.0.8 floor), far beyond FO3/FNV/FO4's 1–10(+1) range —
  several derived formulas below are explicitly **piecewise by SPECIAL band**
  (0–10 / 10–20 / 20+) to handle this extended range, unlike FO4's clean linear forms.

## Skills — LOCKED (none)

FO76 has no skills — perk cards replace them, gated by SPECIAL point investment
(same shape as FO4 perks, `skills: []`).

## Leveling — LOCKED

```
XP_to_next(L) = 160·L − 120
```

Linear `a·L+b` shape shared with FO3/FNV/FO4 (own constants `a=160, b=-120`), verified
against the page's own table (L2→200, L10→1480, L50→7880). No documented level cap
(SPECIAL/perk points stop accruing at 50, but leveling itself continues — XP curve
given through L50000 on the page, effectively uncapped).

## Derived statistics

| Derived stat | Governing SPECIAL | Formula | Status |
|---|---|---|---|
| Carry Weight | Strength | `150 + 5·STR` | **LOCKED** |
| Melee Damage (1H/2H) | Strength | `×(1 + STR/20)` | **LOCKED** |
| Melee Damage (unarmed) | Strength | `×(1 + STR/10)` | **LOCKED** |
| Health | Endurance | `250 + 5·END` (no level term) | **LOCKED** |
| Disease Resistance | Endurance | piecewise, see §below | **LOCKED** |
| Sprint AP drain | Endurance | `11.55 − 0.22·END` AP/s | **LOCKED** |
| Barter buy/sell multipliers | Charisma | piecewise, see §below | **LOCKED** |
| Quest XP / Caps bonus | Charisma | `1 + 0.05·min(CHR,25)` (table) | **LOCKED** |
| Experience-point multiplier | Intelligence | `×(1 + 0.03·INT)` | **LOCKED** (matches FO4) |
| Scrapping / crafting-condition multipliers | Intelligence | tables, hardcap 20–25 INT | **LOCKED** (table-based) |
| Action Points | Agility | `60 + 10·AGI` | **LOCKED** (matches FO4) |
| Sneak / compass detection | Perception, Agility | — | PENDING (GMST names known, no closed form given) |
| Critical Hit recharge | Luck | `1.5·LCK + 5` (+ weapon-dependent term) | **LOCKED** (partial — see caveat) |

**Cross-game confirmation:** Action Points (`60+10·AGI`) and the XP multiplier
(`×(1+0.03·INT)`) are **byte-for-byte identical to FO4** — first direct confirmation
that this GMST family is shared unchanged between FO4 and FO76, not just "presumably"
(as flagged in `charal-fo4-ruleset.md`). Carry Weight keeps FO3/FNV's base (150) but
halves the multiplier (`Mult=5` vs `10`) — already captured in
`charal-fnv-fo3-ruleset.md`.

**Divergence from FO4:** Melee Damage is **weapon-type-split** in FO76 (`STR/20` for
1H/2H, `STR/10` for unarmed) vs FO4's single `STR/10` for everything — FO76 added a
weapon-type dimension FO4 doesn't have. Health has **no level term** at all (FO3/
FNV/FO4 all scale Health with level; FO76 doesn't) — a genuine per-game shape
difference, not just different constants.

### Disease Resistance — LOCKED (new resistance-family member)

```
DiseaseResistMult(END) = 2.1 − END/10                        for 0 ≤ END ≤ 10
DiseaseResistMult(END) = min(0.85, 1.1 − (END−10)/20)         for END ≥ 10
```

Multiplicative modifier on catch-chance (lower = better; floors at 0.85 = 15% catch-
chance reduction cap). This is a **third AFFLICTION-family member** alongside
Radiation/Poison Resistance ([[charal_character_layer]]) but with a different shape:
piecewise-linear multiplier rather than the FO3/FNV `(END−1)·k` percent-reduction
form, and Endurance-governed like the other two. FO76-only (FO3/FNV/FO4 have no
Disease Resistance stat).

### Sprint AP drain — LOCKED

```
ActionPointsPerSecond = (1.05 − 0.02·Endurance) × 11 = 11.55 − 0.22·Endurance
```

Named GMSTs (page comment): `fSprintActionPointsEndBase = 1.05`,
`fSprintActionPointsEndMult = −0.02`, `fSprintActionPointsDrainMult = 11.0` — same
GMST *names* as FO4's sprint-drain formula (`charal-fo4-ruleset.md`) but different
*values* (FO4: `EndMult=-0.05, DrainMult=12.0`) — another confirmation the whole
family is AUTHORED-GMST, not hardcoded shape.

### Charisma — LOCKED (piecewise, table-based)

Barter buy/sell multipliers are 3-band piecewise formulas over Charisma (bands at
0–10 / 10–20 / 20+, each with its own linear coefficient and a floor/ceiling clamp on
the top band) — too irregular for `DerivedStatFormula`'s single affine/bilinear shape;
routes as a **lookup-table gameplay input**, same treatment as FO4's table-based Luck
crit-recharge. Quest XP/Caps bonus is a clean `1 + 0.05·CHR` capped at CHR 25
(`×2.20`), tabulated 1–25 on the page — this one IS affine+capped, could fit
`DerivedStatFormula::capped`.

### Critical Hit recharge — LOCKED (partial)

Two figures given, not fully reconciled: the derived-stats table's primary formula is
weapon-dependent (`5 + W + <non-linear Luck term>`, `W` a per-weapon constant, "usually
1 but occasionally 5"); the Miscellaneous Statistics table gives a simpler
`(LCK × 1.5) + 5`. Both are **Luck-governed but not clean single-input affine forms**
— routes as a gameplay-system input like FO4's crit recharge (§ already noted there as
table-based), not the `derived` table. The non-linear Luck term and per-weapon `W`
table are not given on this page — PENDING if ever needed at that precision.

## Open items

- **Sneak / detection formulas**: GMST names listed (`fSneak*`, ~20 constants) but no
  closed form given — same "dead end on this source" situation as FO4's sneak-
  detection row (`charal-fo4-ruleset.md`).
- **NPC SPECIAL storage for FO76**: not researched — FO76 has no traditional NPCs in
  the FO4 sense (multiplayer, mostly hostile spawns), likely out of scope for the
  NPC-population path entirely. Not blocking (FO76 isn't in the §8 rollout order).
- **FO76 has no traditional companions** (already noted in `charal.md` §7.1) — the
  `LevelingModel`/ruleset scope here is player-only by default.
