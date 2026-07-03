# CHARAL — FO4 character ruleset (data capture)

Living capture of the **Fallout 4** `CharacterRuleset` (CHARAL §5), assembled from
public sources as provided. Every row is **LOCKED** (sourced) or **PENDING** (needs
a citable source — no guessing, [[feedback_no_guessing]]). Parent design:
[charal.md](charal.md).

## Attributes — LOCKED

7 SPECIAL, AV codes 5–11 (shared with FNV), EditorIDs `Strength`, `Perception`,
`Endurance`, `Charisma`, `Intelligence`, `Agility`, `Luck`.

- Base range 1–10; **boostable past 10** by temporary (chems) and static (apparel)
  mods, with functional benefit.
- **Bobbleheads** raise a stat +1 permanently and **count toward perk requirements**.
- Survival mode can *lower* stats (hunger / thirst / disease / fatigue); X-cell
  addiction drops all stats −1.

Source: FO4 SPECIAL page. Player chargen: each starts at **1**, **+21** points to
allocate = **28** total starting (vs 40 in FO3/NV). *(Chargen is player-only; NPC
SPECIAL storage is the open item below.)*

## Skills — LOCKED (none)

FO4 has **no skills** — perks replace them. `skills: []`.

## Derived statistics — core formulas LOCKED, gameplay inputs governance-only

The SPECIAL → derived-stat **governance graph** from the FO4 SPECIAL page. The page
states which SPECIAL drives which derived stat but **not the formulas** ("not
described in full detail … see the respective articles"). Each coefficient needs its
own citable source before it enters the computed `derived` table (CHARAL §6).

| Derived stat | Governing SPECIAL | Formula | Status |
|---|---|---|---|
| Carry Weight | Strength | `200 + 10·STR` (`fAVDCarryWeight{Base,Mult}`) | **LOCKED** (§below) |
| Melee Damage | Strength | `×(1 + STR/10)` *(multiplier)* | **LOCKED** (§below) |
| V.A.T.S. weapon accuracy | Perception | `≈ +3.167 pp / PER` (cap 95 %) | **LOCKED** (approx, empirical) |
| Health | Endurance + level | `77.5 + END·4.5 + Lvl·2.5 + Lvl·END/2` | **LOCKED** (player — §below) |
| Sprint AP drain | Endurance | `(1.05 − 0.05·END) × 12` AP/s | **LOCKED** (§below) |
| Dialogue persuasion success | Charisma | `CHR·15% − DifficultyMod`, clamped [5,100]% | **LOCKED** (§below) |
| Barter prices | Charisma | `Buy = 3.50 − CHR·0.15` (floor 1.2); `Sell = 1/Buy` (ceil 0.8) | **LOCKED** (§below) |
| Max settlement population | Charisma | `10 + CHR` | **LOCKED** (§below) |
| Experience-point multiplier | Intelligence | `×(1 + 0.03·INT)` *(multiplier)* | **LOCKED** (§below) |
| Hacking (dud-word reduction) | Intelligence | — | PENDING |
| Action Points | Agility | `60 + 10·AGI` (`fAVDActionPoints{Base,Mult}`) | **LOCKED** (§below) |
| Pickpocket chance | Agility | `+1 pp / AGI` *(additive)* | **LOCKED** (§below) |
| Sneak detection | Agility | — | **DEAD END** — source itself says "unknown formula" |
| Critical Hit recharge rate | Luck | — | PENDING |

Routing of these once the coefficients arrive:

- **In the `derived` table** (computed from SPECIAL, CHARAL §6) — all four locked:
  **Health**, **Action Points**, **Carry Weight** (absolute-value outputs), and
  **Melee Damage** (a **multiplier** output — different kind, see §below).
- **SPECIAL-driven multipliers** (applied at award / use time, not stored AVs):
  **XP multiplier** `×(1 + 0.03·INT)` — LOCKED (Intelligence §); **Melee Damage**
  (above). **Critical recharge** (Luck) is the one remaining: Luck-driven but
  **table-based** (hits-to-fill-meter per Luck value), not a clean formula — stays a
  gameplay-system input.
- **Gameplay-system inputs** (consume the SPECIAL AVs but live in their own systems,
  not the `derived` table): persuasion (per-check difficulty offset applied at use
  time), barter (reciprocal-pair shape, not affine/bilinear), hacking, sneak detection
  (unresolvable per source). Settlement population cap (`10+CHR`) fits the `derived`
  table shape but is a settlement-system value, not a per-actor AV — same routing
  class as VATS accuracy. V.A.T.S. accuracy and pickpocket chance now have locked
  per-point coefficients (table above).

### Health — LOCKED (player formula)

```
HP = 77.5 + Endurance·4.5 + Level·2.5 + (Level · Endurance)/2
```

Verified against the page's own example: END 2, Level 2 → 77.5 + 9 + 5 + 2 =
**93.5** (the Pip-Boy truncates the display to 93; the actual value is the float).
Endurance may exceed 10, and health rescales **dynamically** with any Endurance /
level change — there is no permanent/temporary split for player HP.

**Triple-confirmed.** The Endurance (FO4) page independently re-derives the same
formula and gives a cleaner level-aware decomposition (better for a
`CharacterLevel`-driven `derived`):

```
HP(L1)        = 80 + 5·END            # base at level 1
HP_per_level  = 2.5 + END/2           # gained each level after 1
TotalHP       = floor( HP(L1) + HP_per_level·(L − 1) )
              = floor( 77.5 + 4.5·END + 2.5·L + 0.5·L·END )
              = floor( (END + 5)(L + 9) / 2 ) + 55      # factored form
```

The `floor` costs ≤1 HP only when END and L are both even; otherwise exact.

**Two caveats that gate *applying* this (the formula itself is locked data):**

1. **Player-only.** The page states the formulas "generally only apply to the
   player character." NPC health uses a different path (NPC level-list / auto-calc),
   so this does **not** populate NPC health in the current
   `derive_npc_actor_values` — NPC health derivation is still open
   ([[actor_value_population]] derived-attribute deferral).
2. **No player-actor entity yet.** There is still nowhere to apply a *player* health
   formula (`scene.rs`'s `player_entity` is an `AnimationPlayer`) — same block noted
   in [[actor_value_population]]. So this is locked **data**, application deferred.

**Cross-game Health (same source — file into the sibling rulesets when opened):**

| Game | Player Health formula | Worked example |
|---|---|---|
| FO3 | `90 + END·20 + Level·10` | END 5, L1 → 200 |
| FNV | `100 + END·20 + (Level−1)·5` | END 5, L1 → 200; END 10, L30 → 445 |
| FO4 | `77.5 + END·4.5 + Level·2.5 + Lvl·END/2` | END 2, L2 → 93.5 |
| FO76 | `250 + 5·END` | END 15 → 325 (base cap; END starts at 1) |

This is direct evidence for CHARAL §3 (**ruleset is AUTHORED via GMST**): the page
names `fAVDHealthLevelMult` changing **10 → 5** between FO3 and FNV, and the base
changing **90 → 100** — i.e. the Health constants are per-game `GMST` values, not a
hardcoded curve. FNV also re-anchors the level term to `(Level − 1)`. The FO4
constant follows the same shape (see §below); **FO76's is now directly sourced**
(corrected from an earlier `245 + 5·END` guess — the *Fallout 76 SPECIAL* page's own
math block gives `250 + 5·Endurance`, no level term at all: FO76 Health is
**not** level-scaled, unlike FO3/FNV/FO4). FO76 also introduces a distinct
**level-XP curve**, `XP_to_next(L) = 160·L − 120` (same `a·L+b` shape as the other
three, own constants) — see `docs/engine/charal-fo76-ruleset.md`.

> Out of scope: the page's "behind the scenes" quote (`5 + 2·INT` skill points/level,
> `3 + END/2` HP/level) is **Fallout 1/2** (pre-Gamebryo) — it does **not** resolve
> the FO3/NV per-level tag-skill deferral, which is a different engine.

### Action Points — LOCKED

```
AP = 60 + 10·Agility
```

GMSTs named on the Agility page: `fAVDActionPointsBase = 60`,
`fAVDActionPointsMult = 10` — like Carry Weight, the constants are AUTHORED GMSTs
(CHARAL §3), read not hardcoded. Verified: AGI 5 → 60 + 50 = **110**.

Regeneration is **6 %/sec of max AP**, which the Agility page expresses exactly:

```
AP/second = (18 + 3·Agility) / 5 = 3 + 3·(Agility + 1)/5
```

(AGI 0 → 3.6/s = 6 % of 60; AGI 5 → 6.6/s = 6 % of 110 ✓.) Sprinting depletes AP at
an **Endurance**-dependent rate — now **LOCKED** (source: the Endurance (FO4) page):

```
ActionPointsPerSecond = (1.05 − 0.05·Endurance) × 12
```

Named GMSTs: `fSprintActionPointsDrainMult = 12.0`, `fSprintActionPointsEndBase =
1.05`, `fSprintActionPointsEndMult = −0.05` — another AUTHORED-GMST confirmation
(CHARAL §3/rollout item 6). Worked: END 1 → 12 AP/s (the "default" drain); END 10 →
6.6 AP/s; **END 21 zeroes the drain** (no sprint AP cost, no duration limit) — the
formula is unbounded below zero, so the consumer must clamp at 0, not the formula
itself. Rank 3 of the Moving Target perk changes the ×12 multiplier to ×6 (perk
modifies the GMST-equivalent constant, not the Endurance term) — that 0-drain
threshold stays 21 regardless. This is a **multiplier-rate** output (AP/sec, not an
absolute AV), so it routes with XP-multiplier/Melee-Damage as an applied-at-use-time
value, not a stored derived AV.

- **Cross-game:** FO4 multiplies the Agility bonus by **×10**; FO3/FNV use **×2 or
  ×3** (the page notes the multiplier difference but not the FO3/FNV base) — PENDING
  for their rulesets.
- **Application caveat:** AP (and its sprint-drain rate) is a player / V.A.T.S.
  resource, so the Health §'s "no player-actor entity yet" gate applies equally —
  locked data, application deferred.

**Independently re-confirmed 2026-07-03** on the Agility (FO4) page itself — same
`AP = 60 + 10·AGI` and same `AP/second = (18 + 3·AGI)/5` formulas, same two GMST
names — this is the page CHARAL cites as *the* Agility source (§ table above), now
directly read rather than inferred.

### Pickpocket chance — LOCKED (source: Agility (FO4) page, 2026-07-03)

```
PickpocketChance = base + 1 pp × Agility
```

"Each point adds 1% directly to the player's pickpocketing chance" — a flat additive
per-point bonus, distinct from the (Luck-governed) **Pickpocket** perk's `+25%→×2`
multiplier already in the perk chart (row 274). No GMST name given, no base value
given (base pickpocket chance lives elsewhere — target-guard-perception-driven, not
SPECIAL-derived). Routes as a gameplay-system input like persuasion/barter, not the
`derived` table (it's a percentage bonus into an existing system, not a standalone
AV). **Sneak detection likelihood**, by contrast, is a genuine dead end: the same
page states Agility is "used in an unknown formula" for it — the wiki itself doesn't
know, so this isn't worth re-querying (§9 no-guessing scope: nothing to cite).

### Max Settlement Population — LOCKED (source: Charisma (FO4) page, 2026-07-03)

```
MaxPopulation = 10 + Charisma
```

Clean affine, `derived`-table-shaped — but the output feeds the settlement system,
not a per-actor AV, so it stays a gameplay-system input rather than joining
Health/AP/CarryWeight in the actual `derived` table (§6 scopes `derived` to
per-actor `ActorValues`).

### Dialogue Persuasion — LOCKED (source: Charisma (FO4) page, 2026-07-03)

```
SuccessChance = Charisma × 15% − DifficultyModifier      # clamped [5%, 100%]
```

Difficulty modifiers are fixed per check tier: Yellow `−35%`, Orange `−50%`, Red
`−65%` (CHR 11+ = guaranteed pass on a Red check with no other modifiers). Each rank
of Lady Killer/Black Widow adds a flat `+10%` if the dialogue target is the opposite
gender. The Charisma term itself is a clean affine (`CHR × 15`), but the **check
difficulty offset is supplied at the call site**, not part of the SPECIAL-derived
formula — same shape as V.A.T.S. accuracy (a per-point coefficient consumed by a
system that adds its own situational modifier). Routes as a gameplay-system input,
not the `derived` table.

### Barter Prices — LOCKED (source: Charisma (FO4) page, 2026-07-03)

```
BuyingPriceModifier  = 3.50 − Charisma × 0.15     # floored at 1.2
SellingPriceModifier = 1 / BuyingPriceModifier     # ceiled at 0.8
```

Item price = innate value × the applicable modifier (before perk/discount
multipliers, which stack multiplicatively on top — Cap Collector, Junktown Vendor,
Barter bobblehead, friend discounts). **Selling is the reciprocal of Buying**, not an
independent affine formula — a genuinely new *shape* `DerivedStatFormula` doesn't
have (affine/bilinear only); modeling both as one reciprocal pair (compute Buying,
derive Selling) rather than two independent rows is the fitting representation if
this ever enters the canonical table. Both clamp independently (Buying ≥ 1.2, Selling
≤ 0.8) — CHR 16+ hits both clamps simultaneously (Buying 1.2 flat, Selling 0.8 flat).

### Carry Weight — LOCKED (actor-general)

```
CarryWeight = fAVDCarryWeightBase + Strength × fAVDCarryWeightMult
            = 200 + 10 × Strength
```

The page names the GMSTs directly — `fAVDCarryWeightBase = 200.0`,
`fAVDCarryWeightMult = 10.0`. **Strongest CHARAL §3 confirmation yet:** the CHARAL
GMST-sourcing step (rollout item 6) reads these two values rather than hardcoding
200 / 10.

- **Actor-general (not player-only).** The `fAVD…` (Actor Value Derived) prefix
  means this derives the `CarryWeight` AV for **any** actor — NPCs and companions
  included (companion-specific carry weights are overrides layered on top). So
  unlike the player-only Health curve, this row **is applicable to the NPC
  population path**, the first FO4 derived stat that is.
- Survival mode overrides the base to **75** (patch 1.5) — a mode toggle on
  `fAVDCarryWeightBase`, not a separate formula.

### Melee Damage — LOCKED (multiplier, actor-general)

```
MeleeDamageMultiplier = 1 + Strength × 0.1 = 1 + Strength/10
```

A **multiplier** on melee + unarmed weapon damage (STR 0 → ×1.0, STR 5 → ×1.5,
STR 10 → ×2.0) — not an additive bonus, and not a standalone resource AV. Melee and
unarmed are affected identically; applies to any actor with Strength (NPC melee
scales too) — actor-general.

**Design note — `DerivedStatFormula` needs an output *kind*.** This is the first
derived stat that is a **multiplier**, where Health / AP / Carry Weight produce
**absolute** AV values. So the canonical formula type carries a kind:

```rust
enum DerivedOutput { Absolute, Multiplier }  // absolute → an AV value; multiplier → applied at use against a base
```

Multiplier-kind formulas apply at combat/use time against a base (weapon damage);
absolute-kind formulas produce the AV the runtime reads. The `0.1` coefficient is
presumably a `GMST` (not named on this page).

### Derived table — core complete ✅

All four AV-backed derived stats are locked (Health, Action Points, Carry Weight,
Melee Damage). The remaining governance rows (VATS, persuasion, barter, settler cap,
hacking, sneak, XP-mult, crit, sprint-AP-drain) are gameplay-system inputs or
storage-TBD modifiers, not blockers for the `derived` table.

### Radiation Resistance — NOT a SPECIAL-derived stat (FO4 re-architecture)

Source: fandom *Radiation Resistance*. **FO4 dropped the Endurance derivation** — its
initial value is **0** (FO3/FNV used `(END−1)·2`). Rad Resistance is now a **flat
additive `RadResist` AV** fed entirely by armor / chems / perks (Hazmat +1000, full
power armor +1050, Rad Resistant perk +10/level), and damage is reduced by the **same
non-linear curve as FO4 Damage Resistance & Energy Resistance** — *not* a
`DerivedStatFormula`. The rule of thumb on the page: when `rads/s == RadResist`, you take
half damage; higher/lower resistance scales damage non-linearly (two empirical sample
tables given, not a closed form). So FO4 RadResist is **not** a CHARAL derived formula —
it's a plain resistance AV (base + mods) consumed by FO4's shared resistance function,
whose closed form is the FO4 damage-resist GMST formula (source later, alongside DR/ER).
This is the FO4 face of the *affliction family*'s resistance half; FO3/FNV keep the
END-derived percentage (`charal-fnv-fo3-ruleset.md`). The Rad-X / armor / perk additions
are the actor-value mod layers, identical in spirit to FO3/FNV but on a flat (not %)
scale.

**Poison Resistance — same re-architecture** (source: fandom *Poison Resistance*). FO4
dropped its `(END−1)·5` FO3/FNV derivation too; `PoisonResist` is now a flat additive AV
(Med-X +250, Poisoner's-mod armor +25/piece, max +125). Crucially the page **confirms
the affliction shape explicitly**: poison damage "stacks and it is usually accompanied by
a debuff to SPECIAL attributes" — i.e. the affliction's effect is a **temporary SPECIAL
penalty** (→ `temporary_mod`), exactly the `{pool damage + resistance AV + SPECIAL-
penalty}` model. Radiation + Poison are now **two members** of the affliction family, so
it's a reusable pattern (not a radiation one-off); both go flat-additive-AV in FO4 and
END-derived-% in FO3/FNV. **The pool/threshold mechanism itself is now BUILT and
game-agnostic** (`crates/core/src/character/affliction.rs`, 2026-07-03) — one
`AfflictionTable`/`AfflictionStatus` pair serves Radiation, Poison, Disease (FO76), or
any future member identically; only the per-game threshold *numbers* differ, and
those are still unsourced for all three (no shipped table yet).

**Addiction — confirms the pattern, but stays OUT of CHARAL's derived-stat scope**
(source: fandom *Addiction*, 2026-07-03). Every game (FO1→FO76) has a per-item
addiction-chance roll (e.g. FO4 Jet 25%, Buffout 25–35%, X-cell 35%) that, once
triggered, applies a **fixed SPECIAL penalty table** until cured (FO4: Alcohol
`CHR−1,AGI−1`; Buffout `STR−1,END−1`; X-cell `ALL SPECIAL −1`; full per-chem table on
the page) — the same `temporary_mod` mechanism as the Radiation/Poison affliction
family, so it's a **4th confirmation of the pattern**, not a new one. It does **not**
add a CHARAL derived-stat formula, though: addiction chance and its penalty are
**per-item constants**, not a function of any SPECIAL attribute — there is no
`Endurance → AddictionResist` formula the way there is for Rad/Poison/Disease
Resistance. This routes entirely to the consumable/status-effect (scripting) layer,
same out-of-scope bucket as perk effects (CHARAL §7) — CHARAL's only touchpoint is
that `temporary_mod` is the general mechanism, already established. Chem Resistant
(FO4, Endurance-gated perk) reduces addiction *chance* by a flat 50%/immune per rank
— confirms the existing FO4 Endurance perk-chart row (Chem Resistant, `charal-fo4-
ruleset.md` perk table) rather than adding new data.

## Perk chart — COMPLETE (7 / 7 SPECIAL columns, 70 perks)

**Structure** (confirmed by the Strength column): each SPECIAL has **10 perks**,
gated at SPECIAL value **1–10** — that's the 7 × 10 = **70-cell** grid. Each perk has
**1–5 ranks**; rank 1 needs only the SPECIAL value, higher ranks add an escalating
**level gate** and require the previous rank. Some ranks are **DLC-gated**.

For the **ruleset** (gating) the load-bearing data per perk is: SPECIAL requirement,
rank count, per-rank level gate, prerequisite. The rank **effects** feed the
[[perk_entry_points]] modifier pipeline — a separate layer (CHARAL §7, out of CHARAL
scope); summarised here, full per-rank text deferred to the entry-point work.

All 7 columns fetched from the per-SPECIAL fandom pages via the `action=parse` API
(2026-06-29). Convention: **Val** = the SPECIAL requirement (constant down a column);
**R** = rank count; **Level gates** = the per-rank level requirement, `—` = no extra
gate (available at the SPECIAL value); *italic* = DLC-gated rank. Effect column is a
one-line gist — full per-rank text deferred to the [[perk_entry_points]] layer.

#### Strength

| Val | Perk | R | Level gates | Gist |
|---|---|---|---|---|
| 1 | Iron Fist | 5 | —, 9, 18, 31, 46 | unarmed dmg +20→80 %, ×2; disarm/cripple/paralyze |
| 2 | Big Leagues | 5 | —, 7, 15, 27, 42 | melee dmg +20→80 %, ×2; disarm/cripple/decap |
| 3 | Armorer | 4 | —, 13, 25, 39 | craft armor mods 1–4 |
| 4 | Blacksmith | 3 | —, 16, 29 | craft melee weapon mods 1–3 |
| 5 | Heavy Gunner | 5 | —, 11, 21, 35, 47 | heavy-weapon dmg +20→80 %, ×2; stagger |
| 6 | Strong Back | 5 | —, 10, 20, 30, 40 | +25/+50 carry weight; run/fast-travel overenc. (*R5 FH*) |
| 7 | Steady Aim | 3 | —, 28, 49 | hip-fire accuracy (*R3 NW*) |
| 8 | Basher | 4 | —, 5, 14, 26 | gun-bash dmg +25 %→×2; cripple/crit |
| 9 | Rooted | 3 | —, 22, 43 | standing still: +DR + melee dmg; auto-disarm |
| 10 | Pain Train | 3 | —, 24, 50 | power-armor sprint damage + stagger |

#### Perception

| Val | Perk | R | Level gates | Gist |
|---|---|---|---|---|
| 1 | Pickpocket | 4 | —, 6, 17, 30 | pickpocket +25→×2; plant grenade, steal equipped |
| 2 | Rifleman | 5 | —, 9, 18, 31, 46 | non-auto rifle dmg +20→80 %, ×2; ignore DR/ER |
| 3 | Awareness | 2 | —, 14 | reveal target resists in VATS (*R2 NW*) |
| 4 | Locksmith | 4 | —, 7, 18, 41 | Adv/Expert/Master locks; pins never break |
| 5 | Demolition Expert | 4 | —, 10, 22, 34 | explosives dmg +25→×2; craft, radius |
| 6 | Night Person | 3 | —, 25, 37 | +INT/PER at night; night vision; +30 HP (*R3 FH*) |
| 7 | Refractor | 5 | —, 11, 21, 35, 42 | +10→50 Energy Resistance |
| 8 | Sniper | 3 | —, 13, 26 | scoped stability/AP; knockdown; VATS head acc |
| 9 | Penetrator | 2 | —, 28 | VATS targets behind cover; no acc penalty |
| 10 | Concentrated Fire | 3 | —, 26, 50 | VATS consecutive-limb acc +10→20 %, +dmg |

#### Endurance

| Val | Perk | R | Level gates | Gist |
|---|---|---|---|---|
| 1 | Toughness | 5 | —, 9, 18, 31, 46 | +10→50 Damage Resistance |
| 2 | Lead Belly | 3 | —, 6, 17 | fewer/no rads from raw food/water |
| 3 | Life Giver | 3 | —, 8, 20 | +20/+40/+60 max HP; HP regen at R3 |
| 4 | Chem Resistant | 2 | —, 22 | −50 %/immune chem addiction |
| 5 | Aquaboy/Aquagirl | 2 | —, 21 | rad-immune underwater; breathe; undetectable |
| 6 | Rad Resistant | 4 | —, 13, 26, 35 | +10→40 Radiation Resistance (*R4 FH*) |
| 7 | Adamantium Skeleton | 3 | —, 13, 26 | −30/−60 %/immune limb damage |
| 8 | Cannibal | 3 | —, 19, 38 | eat corpses to heal |
| 9 | Ghoulish | 4 | —, 24, 48, 50 | rads heal HP; rad reduction (*R4 NW*) |
| 10 | Solar Powered | 3 | —, 27, 50 | +STR/END daytime; rad reduction; HP regen |

#### Charisma  *(perk req shown as "CHR")*

| Val | Perk | R | Level gates | Gist |
|---|---|---|---|---|
| 1 | Cap Collector | 3 | —, 20, 41 | 10/20 % buy/sell price; invest in stores |
| 2 | Lady Killer / Black Widow | 3 | —, 7, 16 | +dmg & +persuade vs opposite sex |
| 3 | Lone Wanderer | 4 | —, 17, 40, 50 | no-companion: −dmg, +carry, +dmg, +AP (*R4 FH*) |
| 4 | Attack Dog | 4 | —, 9, 25, 31 | Dogmeat VATS hold/cripple/bleed (*R4 NW*) |
| 5 | Animal Friend | 3 | —, 12, 28 | pacify/command/frenzy animals |
| 6 | Local Leader | 2 | —, 14 | supply lines; build stores |
| 7 | Party Boy / Party Girl | 3 | —, 15, 37 | alcohol immunity/doubled; +3 Luck |
| 8 | Inspirational | 3 | —, 19, 43 | companion +dmg, −dmg taken, +carry |
| 9 | Wasteland Whisperer | 3 | —, 21, 49 | pacify/command/frenzy creatures |
| 10 | Intimidation | 3 | —, 23, 50 | pacify/command/frenzy humans |

#### Intelligence

| Val | Perk | R | Level gates | Gist |
|---|---|---|---|---|
| 1 | V.A.N.S. | 2 | —, 36 | path-to-objective; +2 PER at R2 (*R2 NW*) |
| 2 | Medic | 4 | —, 18, 30, 49 | stimpak/RadAway +40→100 % |
| 3 | Gun Nut | 4 | —, 13, 25, 39 | craft gun mods 1–4 |
| 4 | Hacker | 4 | —, 9, 21, 33 | Adv/Expert/Master terminals; no lockout |
| 5 | Scrapper | 3 | —, 25, 40 | salvage uncommon/rare components (*R3 FH*) |
| 6 | Science! | 4 | —, 17, 28, 41 | craft energy-weapon/high-tech mods 1–4 |
| 7 | Chemist | 4 | —, 16, 32, 45 | chem duration +50 %→… |
| 8 | Robotics Expert | 3 | —, 19, 44 | hack/shut-down/frenzy/command robots |
| 9 | Nuclear Physicist | 3 | —, 14, 26 | radiation weapons +dmg; fusion-core duration |
| 10 | Nerd Rage! | 3 | —, 31, 50 | <20 % HP: slow-time, +dmg, +DR |

#### Agility

| Val | Perk | R | Level gates | Gist |
|---|---|---|---|---|
| 1 | Gunslinger | 5 | —, 7, 15, 27, 42 | non-auto pistol dmg +20→80 %, ×2; range/disarm |
| 2 | Commando | 5 | —, 11, 21, 35, 49 | auto-weapon dmg +20→80 %, ×2; hip-fire/stagger |
| 3 | Sneak | 5 | —, 5, 12, 23, 38 | −20→80 % detection; no traps/mines |
| 4 | Mister Sandman | 3 | —, 17, 30 | sleeping instakill; silenced sneak dmg |
| 5 | Action Boy/Girl | 3 | —, 18, 38 | +25/50/75 % AP regen (*R3 FH*) |
| 6 | Moving Target | 3 | —, 24, 44 | sprint +DR/ER; −AP sprint cost |
| 7 | Ninja | 3 | —, 16, 33 | ranged/melee sneak-attack multipliers |
| 8 | Quick Hands | 3 | —, 28, 40 | faster reload; free VATS reload; +10 AP (*R3 NW*) |
| 9 | Blitz | 2 | —, 29 | melee VATS distance ×2/×3 |
| 10 | Gun Fu | 3 | —, 26, 50 | +dmg to 2nd/3rd/4th VATS targets |

#### Luck  *(perk req shown as "LCK")*

| Val | Perk | R | Level gates | Gist |
|---|---|---|---|---|
| 1 | Fortune Finder | 4 | —, 5, 25, 40 | containers spawn caps; "money shot" |
| 2 | Scrounger | 4 | —, 7, 24, 37 | containers spawn ammo |
| 3 | Bloody Mess | 4 | —, 9, 31, 47 | +5→15 % all damage; gib chain |
| 4 | Mysterious Stranger | 4 | —, 22, 41, 49 | VATS stranger appears (*R4 NW*) |
| 5 | Idiot Savant | 3 | —, 11, 34 | random 3×/5× XP (more at low INT) |
| 6 | Better Criticals | 3 | —, 15, 40 | crits +50 %/×2/×2.5 |
| 7 | Critical Banker | 4 | —, 17, 43, 50 | bank 2→5 crits (*R4 FH*) |
| 8 | Grim Reaper's Sprint | 3 | —, 19, 46 | VATS kill restores AP/crit |
| 9 | Four Leaf Clover | 4 | —, 13, 32, 48 | VATS hits fill crit meter 8→14 % |
| 10 | Ricochet | 3 | —, 29, 50 | enemy shots ricochet-kill at low HP |

**Gating shape (uniform across all 70):** a perk is takeable iff
`SPECIAL ≥ Val ∧ character_level ≥ rank_gate ∧ owns(prev_rank)` (+ a DLC flag for
the italicised ranks). Rank counts range 2–5; max level gate is **50** (the soft
cap where escalation stops). This is pure static data → a `[[u8; ?]]`-style table the
`Perks` component validates against; effects are the separate entry-point layer.

## XP / level curve — LOCKED

**XP to advance level L → L+1** (Level (FO4) table):

```
XP_to_next(L) = 75·L + 125          # = 200 at L1, then +75 each level
```

Verified: L1 200, L2 275, L3 350, L10 875, L21 1700, L22 1775. Cumulative-to-reach
has a +1 quirk — L1 total is **1 XP**, so reaching L2 needs 201
(`cum(N) = 1 + Σ_{L=1}^{N-1}(75L+125)`).

**Level-up reward — the FO4 progression model, now definitive:** each level grants
**one point**, spent on either **+1 SPECIAL** (level-up training caps SPECIAL at 10;
bobbleheads / items exceed it) **or one perk rank** (subject to the perk chart's
SPECIAL + level gates). No skills, no separate perk-point pool — SPECIAL and perks
draw from the same per-level point.

**No level cap** (hard limit 65535 = `0xFFFF`; overflow crashes). Level **272**
unlocks every perk + all SPECIAL at 10 (**286** with Far Harbor + Nuka-World).

**XP multiplier** `×(1 + 0.03·INT)` (Intelligence §) scales XP from all sources
before it accumulates against this curve. Survival mode doubles kill XP.

**Cross-game (Level page — for the sibling rulesets):** FO3 & FNV both use
`XP_to_next(L) = 150·L + 50` (also 200 at L1, but **+150**/level — steeper than FO4's
+75, fitting their hard caps of 20 / 30). So the Fallout family shares the *shape*
`XP_to_next = a·L + b` and differs only in (a, b) — exactly a `LevelingModel::XpCurve`
parameterisation for CHARAL.

→ **The FO4 *player-facing* ruleset is now complete.** Only NPC SPECIAL storage
(below) remains before NPC population code.

## NPC SPECIAL storage — RESOLVED (xEdit `Core/wbDefinitionsFO4.pas`, dev-4.1.6)

**Answer: the `PRPS` (Properties) subrecord — an array of `(AVIF FormID, value)`
pairs.** That is the exact shape `ActorValues` is keyed on, so FO4 NPC population is a
direct `ActorValues::from_pairs` after the usual source→global FormID remap. Both the
"PRPS pairs" and "RACE inheritance" hypotheses were right — they're the same format at
two levels.

### Format (authoritative — xEdit NPC_ definition)

```
PRPS      = array of Property                            # "Properties"
Property  = { avif: u32 (FormID → AVIF),  value: f32 }   # 8 bytes per entry
```

`wbObjectProperty := wbStructSK([0], 'Property', [wbActorValue, wbFloat('Value')])`,
and `wbActorValue := wbFormIDCkNoReach('Actor Value', [AVIF, NULL])` — a **4-byte AVIF
FormID**, not an integer index (the int-enum form is commented out for FO4). SPECIAL
is stored as the Strength…Luck AVIF FormID + its float value.

### Inheritance chain (where a given NPC's SPECIAL comes from)

1. **`RACE.PRPS`** — the race's base actor values (RACE carries the *same* `PRPS`
   array; xEdit line 11145). The default SPECIAL.
2. **`TPLT` + ACBS Template Flags** — if "Use Stats" is set, inherit SPECIAL / level /
   etc. from the template `NPC_`/`LVLN` (FO4 keeps the FO3/FNV template model; the new
   `TPTA` "Template Actors" lets each data-type pick its own template source).
3. **`NPC_.PRPS`** — the NPC's own actor-value overrides (xEdit line 10476).
4. **ACBS "Auto-calc stats"** flag (bit 4) — as in FO3/FNV.

### Derived stats are PRECOMPUTED in `DNAM` (not the player formula)

```
DNAM (8 bytes) = { Calculated Health: u16, Calculated Action Points: u16,
                   Far Away Model Distance: u16, Geared Up Weapons: u8, _unused: u8 }
```

An NPC's Health/AP are read **straight from `DNAM`** (baked at save time) — which is
exactly why the wiki Health/AP formulas are flagged *player-only*: the END/AGI curves
produce the *player's* live HP/AP, while NPCs ship a precomputed `Calculated Health`.
This **retires the "NPC health uses a different path" caveat** — the path is *read
DNAM*, no formula.

### Implementation path (unblocks FO4 NPC population)

`parse_npc` today reads RNAM/CNAM/TPLT/ACBS + facegen but **not `PRPS`/`DNAM`**. FO4
NPC population:

1. add a `PRPS` arm — read `N × 8` bytes as `(avif_formid: u32, f32)`, remap each
   FormID source→global, → `ActorValues::from_pairs`;
2. add an FO4 `DNAM` arm — capture `Calculated Health` / `Calculated Action Points`
   (the baked derived AVs);
3. optionally resolve `RACE.PRPS` base + template inheritance for NPCs whose own
   `PRPS` omits SPECIAL.

Shares the source→global remap gap with FactionRanks / class lookups
([[actor_value_population]]). Validation when implementing: byte-decode a real FO4
NPC's `PRPS` via the extract→trace method ([[nif_v10x_stride_drift_resolved]]) — the
*format* is locked by the xEdit definition, this just confirms our offsets.
