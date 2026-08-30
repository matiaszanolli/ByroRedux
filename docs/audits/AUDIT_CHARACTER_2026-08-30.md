# Character / CHARAL Audit — 2026-08-30

**Scope**: `/audit-character`, all 6 dimensions, `--depth deep`, every implemented
family. Run **solo, in-process** (no sub-agent fan-out) as one audit of a
`--preset comprehensive` suite run. Owner slice `crates/core/src/character/` plus
the CHARAL-adjacent siblings `crates/core/src/combat.rs` and
`crates/core/src/stealth.rs`, plus the population boundary
(`crates/plugin/src/esm/records/actor_value_derive.rs`,
`crates/plugin/src/esm/records/actor/mod.rs`, `byroredux/src/npc_spawn.rs`) and
the console/runtime consumers (`byroredux/src/commands/actor_value.rs`,
`crates/scripting/src/condition.rs`, `byroredux/src/combat.rs`).

**Repo state**: HEAD `64f64480`, branch `main`.

Delta since the last full sweep (`docs/audits/AUDIT_CHARACTER_2026-08-27b.md`, HEAD
`969d81c8`) inside the audited slice:

```
byroredux/src/npc_spawn.rs                          |  79 ++-
crates/core/src/character/profile.rs                |  32 ++
crates/plugin/src/esm/records/actor/mod.rs          | 406 ++++++++++++++--
crates/plugin/src/esm/records/actor/tests.rs        | 452 +++++++++++++++++-
crates/plugin/src/esm/records/actor_value_derive.rs | 267 ++++++++++-
```

Every other file in `crates/core/src/character/` — `derived.rs`, `fallout.rs`,
`tes.rs`, `skyrim.rs`, `leveling.rs`, `regen.rs`, `affliction.rs`,
`resistance.rs`, `reputation.rs`, `components.rs`, `attribute.rs`, `skill.rs`,
`ruleset.rs`, `mod.rs` — plus `combat.rs` and `stealth.rs` is **byte-unchanged**
since that sweep. The delta is almost entirely **#3390 (the `CREA` stat model)**,
which had never been audited; the leading finding of this pass is in it.

**Tests recorded** (read-only; nothing launched, no `byroredux` process started):

| Command | Result |
|---|---|
| `cargo test -p byroredux-core character` | **113 passed**, 0 failed, 592 filtered out |

**Verification method**: capture documents read first (Phase 1 item 6), then the
code, then static analysis — plus **three purpose-written census probes**
compiled against the live `byroredux-plugin` parser and run over the shipped
masters. Every count below is measured, not estimated. The probes were temporary
*crates/plugin/examples/_tmp_char_\*.rs* files, run and then deleted; the working
tree is unchanged. `CARGO_BUILD_JOBS=4` throughout; no two cargo commands ran
concurrently.

| Dimension | Area | New findings |
|---|---|---|
| 1 | Ruleset Seam & CHARAL Doctrine | **0** |
| 2 | Derived-Stat Formulas (+ CHARAL-adjacent siblings) | **1 LOW** |
| 3 | Leveling & Progression | **0** |
| 4 | Pools, Afflictions, Resistances & Reputation | **1 LOW** |
| 5 | Population Boundary | **1 MEDIUM** |
| 6 | Coverage, Documentation & Doctrine Drift | **4 LOW** |
| **Total** | | **0 CRITICAL · 0 HIGH · 1 MEDIUM · 6 LOW** |

## Executive summary

**7 findings — 0 CRITICAL · 0 HIGH · 1 MEDIUM · 6 LOW.** All NEW; none duplicates
an OPEN issue or a finding in `docs/audits/AUDIT_CHARACTER_2026-08-27b.md`.

The one substantive finding is at the population boundary and is a direct
consequence of the delta: **#3390 gave FO3/FNV creatures a stat model and made
them full combat participants, but their authored `DATA.Damage` — the single
number that defines a creature's attack — is parsed, parked, and read by
nothing.** 692 FNV and 186 FO3 creatures that author non-zero damage and carry no
inventory weapon therefore attack for the flat `UNARMED_DAMAGE = 8.0`; a
Deathclaw hits for 8 instead of its authored 125. The gap *widened* with #3390:
before it, creatures had no `ActorValues`, so they were not melee-eligible at all.

**Constants are in good shape.** 34 derived-stat rows, 23 pool/affliction/
reputation rows and 15 leveling rows were re-derived from the capture documents
this session — **72 rows checked, 71 PASS**. The single non-PASS is
`stealth.rs`'s six unsourced sub-coefficient groups, already tracked as OPEN
issue #3482 and not re-filed. Notably clean: the 4×4 Fame/Infamy grid is **not**
transposed (confirmed via the asymmetric off-diagonal, not just the corners);
Karma's asymmetric −249/−250 boundary is exact; all 39 FNV faction threshold
numbers match; the FO4 Health cross term is the only non-zero `cross` anywhere;
and the Skyrim GMST overlay requests exactly the two authored curve settings with
no third having crept back in.

**Doctrine holds.** Zero game-identity branches in any `CharacterRuleset`
consumer — including #3390's new arm, which selects on *record kind*
(`npc.is_creature`) through a profile row rather than on game.
`build_character_ruleset` remains the single construction site with exactly one
caller. `effective_actor_level` still has exactly one implementation with the
required `.max(0)` — **not regressed** after two prior duplications.

The remaining six findings are LOW: one latent-contract gap
(`AfflictionTable::band_for` silently requires a sorted `bands` vector that
nothing sorts, asserts or documents) and five documentation defects, of which the
sharpest two are `charal.md` declaring Oblivion "CHARAL-complete end-to-end" for a
ruleset that has no construction site *and* could not resolve a single AVIF if it
did, and this skill's own Dimension-5 checklist instructing auditors to verify a
premise ("`GameKind::Fallout3NV` resolves to the FNV ruleset for both") that was
removed months ago and whose stated conclusion would be a false CRITICAL.

**Verified and dropped: 8 stale candidates.** Listed per dimension. Two were
falsified by measurement rather than reading — the `Perks` rank-0/duplicate
concern (every one of 10,764 authored `PRKR` entries across both masters is rank
1, zero duplicates) and the FO3/FNV ruleset-collapse premise the skill file
itself asserts.

## Constant verification summary

Full per-row tables live in the Dimension 2, 3 and 4 sections below.

| Family | Rows verified against a capture document | Verdict |
|---|---|---|
| FO4 (Health / AP / Carry Weight / leveling) | 6 | all PASS |
| FO3 (Health / AP / shared six / leveling / rewards) | 13 | all PASS |
| FNV (Health / AP / shared six / leveling / rewards) | 13 | all PASS |
| Oblivion (3 pools / armour / attribute bonus / health gain / regen / combat math) | 16 | all PASS |
| Skyrim (Light Armor / Carry Weight / pools / XP curve / skill-XP) | 10 | all PASS |
| Reputation family (Karma / FNV 13 factions / FO4 Affinity) | 13 | all PASS |
| `stealth.rs` (FO3/FNV detection) | 6 top-level + 6 sub-coefficient groups | top-level PASS; sub-coefficients **UNSOURCED**, Existing #3482 |
| **Total** | **72** | **71 PASS · 1 UNSOURCED (already filed)** |

**Not verified**: FO76 and Starfield numbers (no ruleset builder — both are
`NpcStatModel::Stored` + `RulesetBuilder::None`); Oblivion's pre-`AVIF` legacy
actor-value index resolution (`Oblivion.esm` authors no `AVIF` group and no
resolver exists); the `AfflictionTable` threshold numbers (none sourced for any
game, every table ships empty). **Not re-derived**: the deferred FNV/FO3
tag-skill per-level formula.

---
## Dimension 1 — Ruleset Seam & CHARAL Doctrine

### Checks performed (all PASS unless noted)

1. **Doctrine check — no per-game branch in any consumer.** `grep GameKind|game ==|master_name`
   over `crates/core/src/character/`, `crates/core/src/combat.rs`,
   `crates/core/src/stealth.rs`: **zero hits outside doc comments**
   (`mod.rs:53-54`, `profile.rs:3` both describe the seam, do not branch on it).
   `crates/plugin/src/esm/records/actor_value_derive.rs` selects its arm from
   `index.character_rules.{npc_stat_model,creature_stat_model}()` — a data row,
   not a game id. PASS.
2. **The #3390 CREA arm preserved the doctrine.** New `NpcStatModel::CreatureData`
   is reached through `CharacterRulesProfile::creature_stat_model()`, and the
   only predicate in `derive_npc_actor_values` is `npc.is_creature` (record kind,
   not game). `profile.rs` diff since the last sweep is purely additive rows.
   PASS.
3. **Single sink.** `build_character_ruleset` (`byroredux/src/npc_spawn.rs:197`)
   has exactly one caller: `byroredux/src/cell_loader/references/mod.rs:305`,
   guarded by a `try_resource::<CharacterRuleset>().is_none()` idempotence gate.
   No second `insert_resource(CharacterRuleset)` anywhere in non-test code.
   `build_melee_damage_config` (#3092) is a *second* resource built beside it at
   the same site, but it carries no formulas — only a resolved AVIF id — so it is
   not a second ruleset. No code path writes a derived output into `ActorValues`
   directly. PASS.
4. **Derived-table N still justifies the flat `Vec`.** Measured from the builders
   and pinned by `ROSTER_CASES`: FNV 8 rows, FO3 8, FO4 3, Oblivion 8 rows / 5
   stats, Skyrim 2. All within the documented 6–10 rationale. PASS.
5. **Output keys are global-space AVIF FormIDs.** Every builder resolves through
   `EsmIndex::actor_value_form_id` (`index.rs:661`), documented and implemented as
   the index's load-order space — "the same space a remapped CTDA `param_1` …
   compares against". `actor_values` is in the `categories()` merge table
   (`index.rs:550`), so multi-plugin merges go through the same remap as every
   other record map. PASS.
6. **`DerivedInput` sentinel guard is enforced at the *caller*, not just
   documented.** `actor_value_form_id`'s `usable` closure rejects
   `form_id == 0` **and** `form_id == u32::MAX` before returning — exactly the two
   `DerivedInput` sentinels (`UNUSED` / `LEVEL`). Since every production
   `DerivedInput::actor_value(...)` argument originates from this one function,
   a real FormID can never collide with a sentinel. PASS (this is the strongest
   form of the caller-guarantee the constructor's docstring asks for).
7. **Roster split (membership ENGINE-SUPPLIED / FormIDs AUTHORED).** No hardcoded
   FormID in `attribute.rs` / `skill.rs` rosters; no count derived from parsed
   data. PASS.
8. **`effective_actor_level` — regression guard (#3171).** Exactly ONE definition,
   `crates/plugin/src/esm/records/actor/mod.rs:96`, re-exported through
   `records/mod.rs:45` and `npc_spawn.rs:123`. Branch bodies are
   `npc.calc_min.max(1)` (multiplier) / `npc.level.max(0)` (plain) — the required
   `.max(0)`, **not** `.max(1)`. Four call sites, all importing the one copy
   (`inventory.rs:191`, `npc_spawn.rs:150/165/792`, `actor_value_derive.rs:187/323`).
   No fourth copy, no re-added `.max(1)`. PASS — not regressed.
9. **#3172 roster falsification against real masters.** `ROSTER_CASES`
   (`crates/plugin/tests/parse_real_esm.rs:243`) carries all five entries
   (FNV/FO3/FO4/Skyrim/Oblivion) and `assert_rosters_resolve` walks attribute
   roster + skill roster + derived-row count against each shipped master. Oblivion
   is covered by the inverse assertion (`authors_actor_values: false` →
   `index.actor_values.is_empty()`), which is the right shape for a pre-`AVIF`
   game. PASS.

### Findings

**None.** Dimension 1 produced 0 findings.

### Coverage note (carried to Dimension 6, not a finding)

`ROSTER_CASES` asserts `derived_rows: None` for Skyrim and Oblivion, i.e. it
asserts that `build_ruleset` returns `None` for them. That is correct against
`RulesetBuilder::None`, but it means the **derived output keys of
`skyrim_ruleset()` and `oblivion_ruleset()` are unfalsifiable by construction** —
they are exercised only by synthetic in-crate resolvers. For Oblivion this is
structural, not merely pending: `Oblivion.esm` authors no `AVIF` group at all, so
every resolve-or-skip row in `oblivion_ruleset()` would skip and the ruleset would
build empty and silent. Recorded in the coverage matrix (as it was in
`docs/audits/AUDIT_CHARACTER_2026-08-20.md`); it drives one Dimension-6 doc finding.
## Dimension 2 — Derived-Stat Formulas (+ CHARAL-adjacent siblings)

Every shipped coefficient re-derived from the capture documents **before** the
code was opened (Phase 1 item 6). `derived.rs`, `fallout.rs`, `tes.rs`,
`skyrim.rs`, `resistance.rs`, `combat.rs`, `stealth.rs` are **byte-unchanged**
since `docs/audits/AUDIT_CHARACTER_2026-08-27b.md`; the table below is re-derived, not
carried forward.

### Constant verification

| # | Formula / constant | Code value | Document value | Source | Verdict |
|---|---|---|---|---|---|
| 1 | FO4 Health | `bilinear(END,4.5, LEVEL,2.5, cross 0.5, bias 77.5).floored().player_only()` | `floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END)` | `charal-fo4-ruleset.md` § Health | PASS |
| 2 | FO4 Action Points | `affine(AGI,10.0,60.0).player_only()` | `60 + 10·AGI` | `charal-fo4-ruleset.md` § Action Points | PASS |
| 3 | FO4 Carry Weight | `affine(STR,10.0,200.0)` actor-general | `200 + 10·STR`, `fAVD…` = actor-general | `charal-fo4-ruleset.md` § Carry Weight | PASS |
| 4 | FO3 Health | `bilinear(END,20.0, LEVEL,10.0, cross 0.0, bias 90.0).player_only()` | `90 + END·20 + Level·10` | `charal-fnv-fo3-ruleset.md:93` | PASS |
| 5 | FNV Health | `bilinear(END,20.0, LEVEL,5.0, cross 0.0, bias 95.0).player_only()` | `100 + END·20 + (Level−1)·5` ≡ `95 + 20·END + 5·L` | `charal-fnv-fo3-ruleset.md:93` | PASS |
| 6 | FO3 Action Points | `affine(AGI,2.0,65.0).capped(85.0).player_only()` | `65 + 2·AGI` cap 85 | `charal-fnv-fo3-ruleset.md:94` | PASS (scope unsourced — #2937, already documented at the call site) |
| 7 | FNV Action Points | `affine(AGI,3.0,65.0).capped(95.0).player_only()` | `65 + 3·AGI` cap 95 | `charal-fnv-fo3-ruleset.md:94` | PASS (same scope caveat) |
| 8 | FO3/FNV Carry Weight | `affine(STR,10.0,150.0)` | `150 + 10·STR` | `charal-fnv-fo3-ruleset.md:95` | PASS |
| 9 | FO3/FNV Melee Damage | `affine(STR,0.5,0.0)` additive | `STR × 0.5` additive | `charal-fnv-fo3-ruleset.md:97` | PASS |
| 10 | FO3/FNV Critical Chance | `affine(LUCK,1.0,0.0).capped(10.0)` (0–100 scale) | `Luck × 1%` cap 10% | `charal-fnv-fo3-ruleset.md:96` | PASS (#2936 convention correctly applied) |
| 11 | FO3/FNV Unarmed Damage | `affine(Unarmed,0.05,0.5).ceiled()` | `ceil((10 + Unarmed)/20)` ≡ `ceil(0.5 + 0.05·U)` | `charal-fnv-fo3-ruleset.md:98` | PASS |
| 12 | Radiation Resistance | `derive_coeff 2.0`, `resist_cap 85.0`, bias `−2.0`, `clamped_below(0.0)` | `(END−1)·2`, cap 85 % | `charal-fnv-fo3-ruleset.md:99` | PASS |
| 13 | Poison Resistance | `derive_coeff 5.0`, `resist_cap INFINITY`, bias `−5.0` | `(END−1)·5`, uncapped | `charal-fnv-fo3-ruleset.md:100` | PASS |
| 14 | `damage_multiplier` | `(1 − clamp(r,0,cap)/100).max(0.0)` | "damage reduced by this percentage"; ≥100 % = immunity | `resistance.rs` docs + FNV capture | PASS — cannot exceed 1.0 or go negative, so no heal-on-overresist |
| 15 | Oblivion Health | `affine(END,2.0,0.0).player_only()` | `2 × Endurance` | `charal-oblivion-ruleset.md` § Health | PASS |
| 16 | Oblivion Magicka | `affine(INT,2.0,0.0).player_only()` | `Intelligence + INT×fPCBaseMagickaMult(1.0)` = `2×INT` | `charal-oblivion-ruleset.md` § Magicka | PASS |
| 17 | Oblivion Fatigue | four `affine(av,1.0,0.0)` rows summed | `Strength + Willpower + Agility + Endurance` | `charal-oblivion-ruleset.md` § Fatigue | PASS (multi-row rule honoured: uncapped, unrounded, absolute) |
| 18 | Oblivion Armor Rating mult | `ARMOR_RATING_SKILL_BIAS 0.35`, `_COEFF 0.0065`, `.as_multiplier()`, actor-general | `(0.35 + 0.0065 × ArmorSkill)` | `charal-oblivion-ruleset.md` § The Complete Damage Formula ¶3 | PASS |
| 19 | Skyrim Light Armor rating | `LIGHT_ARMOR_RATING_COEFF 0.004`, bias 1.0, `.as_multiplier().player_only()` | `1 + 0.004 × LightArmorSkill` (player); NPC 0.015 not modelled | `charal-skyrim-ruleset.md` § Light Armor Rating Bonus | PASS |
| 20 | Skyrim Carry Weight | `CARRY_WEIGHT_BIAS 250.0`, `_STAMINA_COEFF 0.5`, `.a_from_base()` | `250 + 0.5 × BaseStamina`, base-only (Fortify Stamina excluded) | `charal-skyrim-ruleset.md` § Carry Weight | PASS — `a_from_base` correctly set, the only formula that needs it |
| 21 | `modified_skill` | `skill + 0.4·(luck − 50.0)` | `ModifiedSkill = Skill + 0.4×(Luck−50)` | `charal-oblivion-ruleset.md` § Complete Damage Formula ¶1 | PASS |
| 22 | `oblivion_weapon_damage_multiplier` | `0.5 × (0.75 + 0.005·A) × (0.2 + 0.015·MS)`, both inputs `clamp(0,100)` | identical, and UESP's explicit `[0,100]` clamp | `charal-oblivion-ruleset.md` § Melee weapon damage | PASS (all four coefficients) |
| 23 | `oblivion_hand_to_hand_damage` | `1 + 10.5·(STR/100)·(MS/100)`; `fatigue = 1 + 0.5·health`; **no** clamp | identical; clamp deliberately not applied (UESP states it only for weapon damage) | `charal-oblivion-ruleset.md` § Complete Damage Formula ¶2 | PASS |
| 24 | `stealth::detection_score` top level | `att·(sound + visual + detskill/2) − targetskill/2 − 35` | identical | `charal-fnv-fo3-ruleset.md` § Sneak Detection | PASS |
| 25 | `stealth` DetectorSkill | `(10 + 8·Perception) × state` | identical | same | PASS |
| 26 | `stealth` TargetSkill | `Sneak + 5·(TL−DL) + max(50 − 10·TL, 0) − Armor`, `0` when not sneaking | identical | same | PASS |
| 27 | `stealth` Attenuation | `((max − d)/max)²`, MaxDist 2500 in / 5000 out | identical | same | PASS |
| 28 | `stealth::classify` bands | `< −20` Undetected, `≤ 0` Suspicious, else Detected | `< −20` undetected, `−20..0` suspicious, `> 0` detected | same | PASS — half-open, every value lands in exactly one band |
| 29 | `stealth` sub-coefficients (1.6/0.16 LOS, 12.0 + weight/2, 1.5/1.0/0.0 movement, 2.0 action, 3.0 night-eye, 1.4 light, min 100, 0.21/0.01 visual-movement) | shipped | **not in any capture document** | — | **UNSOURCED — Existing: #3482 (OPEN)**, not re-filed |
| 30 | `eval` structure | `bias + cₐA + c_bB + cross·A·B`, round, then `max(floor).min(cap)`; no allocation, no game branch, `Copy` | as documented | `derived.rs` module docs | PASS |
| 31 | Cap sentinel | uncapped = `f32::INFINITY` (constructors), never `0` | — | `derived.rs` | PASS — the "cap `0` = clamped to zero" trap does not exist; no `capped(0.0)` call site |
| 32 | `DerivedInput` sentinels | `0` = unused, `u32::MAX` = level; `EsmIndex::actor_value_form_id` rejects **both** before returning | — | `index.rs:662` | PASS — enforced at the sole caller, not just documented |
| 33 | `DerivedScope` tagging | Health + AP `PlayerOnly` on every Fallout game; Carry Weight / Melee / Crit / Unarmed / resistances `ActorGeneral`; Skyrim Light Armor `PlayerOnly`, Carry Weight `ActorGeneral`; Oblivion pools `PlayerOnly`, armour `ActorGeneral` | matches each document's scope annotations | all four captures | PASS |
| 34 | Cross term used only where sourced | non-zero `cross` appears in exactly one shipped row (FO4 Health `0.5`) | only FO4 Health needs it | `charal-fo4-ruleset.md` | PASS — no unexplained cross term anywhere |

**34 rows checked; 33 PASS, 1 pre-existing UNSOURCED already tracked as #3482.**

### Chaining ordering (checklist item)

`add_fnv_fo3_shared` registers Unarmed Damage keyed on the **Unarmed skill AV**,
so the population path must write skills before any consumer evaluates it.
Verified: `derive_autocalc_actor_values` emits SPECIAL **and** the full governed
skill roster in one `Vec<(u32,f32)>` that becomes the actor's `ActorValues` via
`from_pairs` at spawn; `derived_value` is only ever called later, from
`condition.rs` (`GetActorValue` / `GetXPForNextLevel`) and
`byroredux/src/combat.rs`. The order invariant holds because population is a
single atomic build, not an incremental sequence. An absent input reads `0.0`,
which `derived.rs` documents as "the Bethesda absent-AV default" — a documented
default, not an accidental zero. PASS.

### Findings

#### CHAR-2026-08-30-D2-01: `DerivedStatFormula::cap`'s field docstring still names the pre-#2936 fractional Critical Chance cap, and a second cap that has never shipped
- **Severity**: LOW
- **Dimension**: Derived Formulas
- **Game**: all (FO3/FNV specifically)
- **Location**: `crates/core/src/character/derived.rs:167-168`
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md:96` (`Luck × 1%` cap **10 %**); `docs/engine/charal-fo4-ruleset.md` § derived table (V.A.T.S. accuracy is routed as a *gameplay-system input*, never a `derived` row).
- **Description**: The `cap` field doc reads *"Upper clamp (`f32::INFINITY` = uncapped). FO3 AP 85, FNV AP 95, **Critical Chance 0.10**, FO4 VATS 0.95."* Two of those four are wrong against the shipped tables. Critical Chance has shipped `capped(10.0)` since #2936 moved every percentage row onto the 0–100 convention — `0.10` is precisely the fractional value that fix removed. `FO4 VATS 0.95` names a cap that has never existed in any builder: `fallout4_ruleset` registers exactly three rows (Health / AP / Carry Weight), pinned by `ROSTER_CASES`' `derived_rows: Some(3)`.
- **Evidence**: `derived.rs:168` `/// Critical Chance 0.10, FO4 VATS 0.95.` versus `fallout.rs:63-66` `DerivedStatFormula::affine(av(l), 1.0, 0.0).capped(10.0)` and the module docstring 100 lines above it: *"`Luck·1` capped `10`, **not** `Luck·0.01` capped `0.10`"*. `grep -rn 'capped(' crates/core/src/character/` returns no `0.95` and no `0.10`.
- **Impact**: The module docstring's percentage-convention paragraph explicitly says there is **no type-level enforcement** and that a new percentage row "must be written on the 0–100 scale by hand". The `cap` field doc is the nearest reference an implementer of such a row reads, and it demonstrates the wrong convention with a concrete number — the exact 100×-off failure mode #2936 was filed for. `AUDIT_CHARACTER_2026-08-15.md` row 22 already quoted this line approvingly ("Crit 0.10") while marking it PASS, so the stale text has survived one audit that read it.
- **Related**: #2936 (the fix that changed the value); #3485 (the sibling 32 B → 36 B pin rot in the same struct, OPEN)
- **Suggested Fix**: Replace the examples with the shipped caps: FO3 AP `85`, FNV AP `95`, Critical Chance `10`, Radiation Resistance `85`. Drop the FO4 VATS example entirely — it names no shipped row.

### Candidates verified and dropped

- **"`Perks` population bypasses `try_set_rank`, so a rank-0 or duplicate `PRKR` entry lands verbatim and `HasPerk` silently reads false."** Structurally true — `stamp_character_components` (`npc_spawn.rs:181-188`) builds `Perks { entries: … .collect() }` directly and `parse_npc_perks` (`crates/plugin/src/esm/records/actor/mod.rs:1406`) pushes without dedup — but **falsified against shipped data**. A purpose-written census over both masters that author `PRKR` measured: `Skyrim.esm` 1,620/5,118 `NPC_`, 7,993 entries, rank histogram `[(1, 7993)]`, **0** records with a duplicate perk FormID; `Fallout4.esm` 1,361/3,015, 2,771 entries, `[(1, 2771)]`, **0** duplicates. Every authored rank is exactly 1 and no perk repeats, so neither the rank-0 ghost nor the shadowed-duplicate case exists in vanilla content, and `num_ranks` can never be exceeded by a rank of 1. Not reportable. (Probe was a a temporary *crates/plugin/examples/_tmp_char_prkr_rank.rs*, run and deleted; tree unchanged.)
- **"`stealth::classify` boundary is ambiguous at −20 / 0."** Re-checked: `< -20.0` → Undetected, `<= 0.0` → Suspicious, else Detected. Half-open, total, matches the document's `−20..0` band exactly. Pinned by `classify_matches_the_documented_bands`. Not a finding.
- **"A cap of `0` is read as clamp-to-zero."** The sentinel is `f32::INFINITY`; no builder passes `capped(0.0)`; `DerivedStatFormula` has no `Default` derive. Trap does not exist.
## Dimension 3 — Leveling & Progression Models

`leveling.rs`, `skyrim.rs`, `tes.rs`, `components.rs` are **byte-unchanged**
since `docs/audits/AUDIT_CHARACTER_2026-08-27b.md`. `profile.rs` gained only the additive
`creature_stats` row (#3390).

### Constant verification

| # | Constant | Code | Document | Source | Verdict |
|---|---|---|---|---|---|
| 1 | FO4 XP curve | `XpCurve { xp_a 75.0, xp_b 125.0, level_cap 0, SpecialOrPerk }` | `75·L + 125`; no hard cap; level grants +1 SPECIAL **or** a perk | `charal-fo4-ruleset.md`; `charal-fnv-fo3-ruleset.md` § XP/level curve | PASS |
| 2 | FO3 XP curve | `xp_a 150.0, xp_b 50.0, level_cap 20` | `150·L + 50`; cap 20 (30 w/ Broken Steel — DLC raises unwired) | `charal-fnv-fo3-ruleset.md` § XP/level curve | PASS |
| 3 | FNV XP curve | `xp_a 150.0, xp_b 50.0, level_cap 30` | same curve; cap 30 (50 w/ add-ons) | same | PASS |
| 4 | FO3 level reward | `SkillPoints { base 10.0, int_mult 1.0, perk_cadence 1 }` | `base: 10, int_mult: 1.0 (FO3), perk_cadence: 1 (FO3)` | same | PASS |
| 5 | FNV level reward | `SkillPoints { base 10.0, int_mult 0.5, perk_cadence 2 }` | `int_mult: 0.5 (FNV), perk_cadence: 2 (FNV)` | same | PASS |
| 6 | SPECIAL immutable at level-up (FO3/FNV) | modelled as `SkillPoints` (no SPECIAL arm) vs FO4's `SpecialOrPerk` | "FO3/FNV grant **no** SPECIAL point at level-up" | same | PASS |
| 7 | Oblivion leveling | `SkillUse { major_skill_ups_per_level 10, level_cap 0 }` | "a level becomes available after 10 increases in major skills"; no hard cap | `charal-oblivion-ruleset.md` § Leveling | PASS |
| 8 | Oblivion attribute bonus bands | `0→1, 1..=4→2, 5..=7→3, 8..=9→4, _→5` | "+1 to +5 … based on governed major-skill increases (0, 1–4, 5–7, 8–9, and 10+)" | same | PASS — all five bands, capped at +5, no roll-over |
| 9 | `oblivion_health_gain_per_level` | `0.1 × endurance` | "10 % of Endurance accrued each level"; UESP worked case END 98 → +9 = `floor(9.8)` | `charal-oblivion-ruleset.md` § Health | PASS (returns the exact `f32`; the `floor` is the caller's, matching the doc) |
| 10 | `SKYRIM_POOL_BASE` | `100.0` | Skyrim base 100 Health/Magicka/Stamina | `charal-skyrim-ruleset.md` § Magicka; project memory *tes_character_rules* | PASS |
| 11 | Skyrim `pool_pick_gain` | `10.0` | "you may add ten points of magicka" per level | same | PASS |
| 12 | Skyrim XP curve | `SkillXp { xp_base 75.0, xp_mult 25.0, level_cap 0 }` | `fXPLevelUpBase` 75, `fXPLevelUpMult` 25 → `25·L + 75` | `charal-skyrim-ruleset.md` § XP/level curve | PASS |
| 13 | `xp_per_skill_rank` | `1.0`, engine-owned, **not** GMST-read | "A skill raised to rank `R` awards `R` character XP; that coefficient is an engine rule, not a `fXPPerSkillRank` GMST" | same | PASS — the settled 2026-08-24 design, correctly implemented |
| 14 | `SKYRIM_SKILL_USE_CURVE` | `1.95` | `fSkillUseCurve` 1.95 | `charal-oblivion-ruleset.md` § Leveling; `charal-skyrim-ruleset.md` | PASS |
| 15 | `skyrim_skill_xp_to_next` | `improve_mult · L^use_curve + improve_offset` | UESP worked case: Lockpicking 15→16 = `0.25·15^1.95 + 300 ≈ 349.13` | `charal-skyrim-ruleset.md` | PASS — pinned by `skill_xp_cost_matches_uesp_lockpicking` |

**15 rows checked, 15 PASS.**

### Mechanism checks

- **Three data variants, one consumer match.** `LevelingModel` is a three-arm
  enum; every accessor (`xp_to_next`, `xp_from_skill_rank`, `pool_pick_gain`,
  `skill_points`, `grants_perk_at`, `level_cap`) does its own single match
  **inside** `leveling.rs`. No consumer matches on the variant: the sole
  production consumer, `condition.rs:663` (`GetXPForNextLevel`, fn 533), calls
  `rs.leveling.xp_to_next(level)` and never inspects the shape. PASS — the seam
  is data, resolved at one place.
- **GMST overlay requests exactly two settings.** `LevelingModel::with_gmst`
  reads `"fXPLevelUpBase"` and `"fXPLevelUpMult"` and nothing else; the
  withdrawn *fXPPerSkillRank* read is gone and `xp_per_skill_rank` /
  `pool_pick_gain` / `level_cap` are carried through unchanged. The `XpCurve`
  and `SkillUse` arms fall through as `other => other` (no GMST source captured
  for them). PASS — no third GMST has crept back in, per the skill's explicit
  check.
- **`level_cap == 0` = uncapped, handled identically in all three variants.**
  `level_cap()` is one or-pattern arm over all three, so there is no per-variant
  divergence to be off-by-one in. It has **no production consumer** (only the
  `profile.rs` regression test) — nothing applies a cap today, so the sentinel
  cannot be mishandled at runtime. Same for `grants_perk_at`, `skill_points`
  and `pool_pick_gain`: test-only. Coverage information, consistent with "no
  leveling runtime exists yet"; not a defect.
- **`grants_perk_at` cadence.** FO4 `SpecialOrPerk` → every level; FO3/FNV →
  `level.is_multiple_of(perk_cadence)` with a `perk_cadence != 0` guard (no
  division by zero); Skyrim → every level; classic TES → never. Matches each
  document. The cadence *phase* is documented in-code as an unrefined modulo
  pending a citing pass — an honest, labelled approximation.
- **Perk rank stacking and the declared max.** `Perks::set_rank` is idempotent
  (raises in place, never stacks a duplicate) and treats `rank == 0` as a no-op;
  `try_set_rank` **rejects** `rank > num_ranks` rather than clamping, per #2944
  and the FO4 capture document's perk-chart gating. PASS. (The population path
  bypasses both — investigated, measured, and dropped; see Dimension 2's
  "Candidates verified and dropped".)

### Reachability (coverage, not a bug)

Only **FO4**, **FO3** and **FNV** leveling models are constructed by a live
path (`CharacterRulesProfile::build_ruleset`'s three `RulesetBuilder` arms).
`LevelingModel::OBLIVION` and `::SKYRIM` — and the whole of `oblivion_ruleset`,
`skyrim_ruleset`, `oblivion_attribute_bonus`, `oblivion_health_gain_per_level`,
`skyrim_skill_xp_to_next`/`_between` — are **unwired**: their profiles carry
`RulesetBuilder::None`. Their constants are correct (rows 7–15 above); nothing
constructs them outside tests. This is the known, documented state.

### Findings

**None.** Dimension 3 produced 0 findings.
## Dimension 4 — Pools, Afflictions, Resistances & Reputation

`regen.rs`, `affliction.rs`, `resistance.rs`, `reputation.rs` are all
**byte-unchanged** since `docs/audits/AUDIT_CHARACTER_2026-08-27b.md`. Constants re-derived
from the capture documents, not carried forward.

### Constant verification

| # | Constant | Code | Document | Source | Verdict |
|---|---|---|---|---|---|
| 1 | `POOL_REGEN_DT` | `1.0/60.0` | "per real-time second" rates → fixed 60 Hz | `charal.md` §4.7 | PASS |
| 2 | `FATIGUE_REGEN_PER_SEC` | `10.0` | `fFatigueReturnBase = 10.0`, `fFatigureReturnMult = 0` | `charal-oblivion-ruleset.md` § Fatigue ¶1 | PASS |
| 3 | `MAGICKA_REGEN_WILLPOWER_COEFF` | `0.02` | `(Willpower × 0.02 + 0.75) × (MaxMagicka/100)` | `charal-oblivion-ruleset.md` § Magicka | PASS |
| 4 | `MAGICKA_REGEN_BASE` | `0.75` | same | same | PASS |
| 5 | `magicka_regen_per_sec` shape | `(wp·0.02 + 0.75)·(max/100)`, `0.0` when stunted or `max ≤ 0` | identical; Stunted zeroes regen entirely | same | PASS |
| 6 | Oblivion Health passive regen | **absent by design** | "Vanilla Oblivion Health has NO passive regeneration at all" | `charal-oblivion-ruleset.md` § Health | PASS — correctly unmodelled |
| 7 | `MAX_REGEN_SUBSTEPS` | `8` | none — engine tuning, not a game rule | — | PASS (labelled unsourced in-code and explicitly *not* "corrected" to physics' `5` — the right no-guessing posture) |
| 8 | Radiation / Poison resistance | see Dim 2 rows 12–13 | — | — | PASS |
| 9 | `damage_multiplier` overresist | floored at `0.0` | ≥100 % = immunity | `resistance.rs` docs | PASS — cannot invert into healing |
| 10 | `KARMA_MIN` / `KARMA_MAX` | `-1000` / `1000` | "clamped to [-1000, +1000]" | `charal-fnv-fo3-ruleset.md` § Karma | PASS |
| 11 | Karma bands | ≥750 VeryGood, ≥250 Good, ≥−249 Neutral, ≥−749 Evil, else VeryEvil | +750…+1000 / +250…+749 / −249…+249 / −250…−749 / −1000…−750 | same | PASS — including the asymmetric −249 vs −250 boundary |
| 12 | `clamp_karma` | bounds **both** ends | — | — | PASS |
| 13 | `REPUTATION_BUMP_POINTS` | `[0, 1, 2, 4, 7, 12]` | editor int 1–5 → 1 / 2 / 4 / 7 / 12 | `charal-fnv-fo3-ruleset.md` § bump table | PASS |
| 14 | `REPUTATION_AXIS_MAX` | `100` | "maxes out at … 100" | same | PASS |
| 15 | 13 FNV faction threshold triples | Boomers 8/25/50 · BoS 3/10/20 · Legion 15/50/100 · Followers 8/25/50 · Khans 5/15/30 · Powder Gangers 5/15/50 · NCR 12/40/80 · WGS 2/5/10 · Freeside 11/35/70 · Goodsprings 3/8/15 · Novac 3/10/20 · Primm 5/15/30 · The Strip 6/20/40 | identical to the document's 13-row table | `charal-fnv-fo3-ruleset.md` § per-faction thresholds | PASS — all 39 numbers |
| 16 | **4×4 standing grid axes** | `STANDING_GRID[infamy][fame]`; `from_ranges(fame, infamy)` indexes `[i][f]` | doc table is `Infamy ↓ \ Fame →` | same | **PASS — not transposed.** Confirmed by the asymmetric off-diagonal: `from_ranges(2,1) = SmilingTroublemaker` (Infamy 1 / Fame 2) and `from_ranges(1,2) = SneeringPunk` (Infamy 2 / Fame 1), which is the pair a transposition would swap |
| 17 | `AFFINITY_MIN` / `MAX` | `-1000` / `1100` | "clamps to [-1000, +1100]" (asymmetric) | `charal.md` §7.1 FO4 affinity | PASS |
| 18 | Affinity 7 bands | ≥1000 / ≥750 / ≥500 / ≥250 / ≥0 / ≥−500 / else | thresholds `-500/0/250/500/750/1000`, 7 bands | same | PASS |
| 19 | `affinity_reaction_delta` | ±15 like/dislike, ±35 love/hate | identical (`TryToModAffinity`) | same | PASS |
| 20 | `AffinityReactionSize` scalars | `0.5 / 1.0 / 1.5` | `CA_Size_{Small,Normal,Large}` = `0.5/1/1.5` | same | PASS |
| 21 | `affinity_passive_gain` | `40.0 − 0.033·current` | `40 − 0.033·current_affinity`; worked example 500 → +23.5 | same | PASS (`40 − 16.5 = 23.5` ✓) |
| 22 | `clamp_affinity` | bounds both ends | — | — | PASS |
| 23 | `ReputationStanding::sentiment` | green/black/red bucketing | **not captured** — the document gives titles + a colour legend, never per-cell colour | — | UNSOURCED, but **already labelled in-code as unsourced (#2949)** with a test that asserts self-consistency only. Correct disclosure; not re-filed |

### Mechanism checks

- **Fixed-60 Hz tick.** `PoolRegenAccumulator::advance` clamps the backlog to
  `MAX_REGEN_SUBSTEPS × POOL_REGEN_DT` **before** extracting ticks, so a long
  stall cannot dump unbounded regen. `frame_dt.max(0.0)` rejects a negative dt,
  and `ticks == 0` returns early, so a paused / zero-dt frame cannot spin. Regen
  is applied as `rate × (ticks × POOL_REGEN_DT)`, i.e. per fixed tick, never per
  frame. PASS.
- **Scheduler access declaration matches what the system touches.**
  `boot.rs:993-1001` declares `reads_resource::<PoolRegenConfig>` +
  `writes_resource::<PoolRegenAccumulator>` + `reads_resource::<CharacterRuleset>`
  + `writes::<ActorValues>` — exactly the four the body uses, in that order.
  PASS. (`/audit-concurrency` Dim 4 owns the general rule; the specific
  declaration is verified here.)
- **`PoolRegenConfig` insertion.** Still inserted by no production path
  (`oblivion_pool_regen_config` has no caller), so the tick is armed and inert.
  Known/documented state, not a finding.
- **Affliction diff-and-reapply.** `reevaluate_affliction` reverses the previous
  band's penalties (`mod_temporary(-delta)`) **before** applying the new band's,
  and no-ops when the band is unchanged — so repeated ticks cannot compound.
  PASS.
- **Band boundaries are half-open and total.** `band_for` =
  `bands.rposition(|b| pool >= b.min_pool)`; a value on a boundary lands in
  exactly one band; below every `min_pool` gives `None` (healthy). PASS —
  subject to the ordering precondition below.
- **Affliction wiring.** No `AfflictionTable` is constructed anywhere in
  production and `affliction_tick_system` has no scheduler registration
  (confirmed by `save_io/registry_completeness_tests.rs:104`'s own allowlist
  reason). Mechanism-ahead-of-data, as documented in `charal.md` §4.6.

### Findings

#### CHAR-2026-08-30-D4-01: `AfflictionTable::band_for` requires `bands` to be sorted ascending, and nothing states, enforces or tests that
- **Severity**: LOW
- **Dimension**: Pools, Afflictions & Reputation
- **Game**: all
- **Location**: `crates/core/src/character/affliction.rs:76-98`
- **Status**: NEW
- **Source**: n/a — this is a structural precondition, not a numeric claim, so no capture-document value is required.
- **Description**: `band_for` returns `self.bands.iter().rposition(|b| pool_value >= b.min_pool)` and its docstring calls that "the highest `min_pool` reached". `rposition` returns the **last index** satisfying the predicate, which equals the highest threshold only when `bands` is sorted ascending by `min_pool`. `AfflictionTable { pool_avif, bands }` has a fully `pub` `bands: Vec<AfflictionBand>` with no constructor, no sort, no `debug_assert`, and no documented ordering contract.
- **Evidence**: `affliction.rs:97` `self.bands.iter().rposition(|b| pool_value >= b.min_pool)`. With `bands = [{min 600, …}, {min 200, …}]` and `pool = 700`, both predicates hold and `rposition` yields index `1` — the *200* band — so a heavily-irradiated actor gets the mild penalty. The only test, `band_for_picks_the_highest_threshold_reached`, builds `stand_in_radiation_table()` already sorted, so it cannot detect this.
- **Impact**: Latent, not live: no `AfflictionTable` is constructed in production today (thresholds are PENDING for every game per `charal.md` §4.6, and `affliction_tick_system` has no scheduler registration). The blast radius is the moment real per-game tables *are* authored — a threshold list transcribed in the natural "worst first" reading order from a wiki table silently inverts every band, and `reevaluate_affliction`'s diff logic stays perfectly consistent while doing it, so there is no crash and no assertion to catch it. This is the same class as the transposed-grid trap Dimension 4 checks for on `ReputationStanding`, which *is* pinned by an asymmetric test.
- **Related**: `charal.md` §4.6 (thresholds PENDING); CHAR-2026-08-30-D2-01 (sibling latent-contract doc gap in `derived.rs`)
- **Suggested Fix**: State the ascending-`min_pool` contract on the `bands` field, add a `debug_assert!(bands.is_sorted_by_key(|b| b.min_pool))` in a constructor (or sort on construction), and extend `band_for_picks_the_highest_threshold_reached` with a deliberately unsorted table asserting the intended answer.

### Candidates verified and dropped

- **"The 4×4 Fame/Infamy grid is transposed."** Checked directly against the
  asymmetric off-diagonal cells; `[infamy][fame]` indexing is correct and
  pinned by `standing_grid_corners_and_diagonal`. Not a finding.
- **"Karma's Neutral/Evil boundary is off by one."** `≥ −249` → Neutral,
  `≥ −749` → Evil reproduces the document's `−249…+249` / `−250…−749` split
  exactly, asymmetry included. Not a finding.
- **"Flat Fatigue regen is blocked by the `CharacterRuleset` gate it doesn't
  need."** Confirmed still present (`regen.rs:176`) — **Existing: #3483 (OPEN)**,
  not re-filed.
- **"#2153's hold-stack reduction: `let config = *config;` shadows the guard
  rather than dropping it."** Confirmed still present —
  **Existing: #3444 (OPEN)**, not re-filed.
## Dimension 5 — Population Boundary (parse → ruleset → actor)

This is where the delta since `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` (HEAD `969d81c8`)
lives: `crates/plugin/src/esm/records/actor_value_derive.rs` +267,
`crates/plugin/src/esm/records/actor/mod.rs` +406, `byroredux/src/npc_spawn.rs` +79,
`profile.rs` +32 — almost all of it #3390 (the `CREA` stat model).

### Checks performed

1. **`build_ruleset` returning `None` degrades to "no CHARAL", never to a
   default ruleset.** `CharacterRulesProfile::build_ruleset` returns `None` on
   `RulesetBuilder::None` (Oblivion / Skyrim / FO76 / Starfield / NONE). Its one
   production caller chain is
   `references/mod.rs:305 → npc_spawn::build_character_ruleset →
   profile.build_ruleset`, wrapped in `if let Some(rs) = … { insert_resource }`.
   No `unwrap_or_default`, and `CharacterRuleset` has no `Default` impl to fall
   back to. Consumers (`condition.rs:500`, `condition.rs:659`, `regen.rs:176`,
   `combat.rs:353`) all use `try_resource` and return the absent-value default.
   **PASS** — Fallout formulas cannot reach a TES actor.
2. **The FO3-vs-FNV collapse the checklist warns about no longer exists.**
   `character_rules_profile` (`records/mod.rs:149`) splits `GameKind::Fallout3NV`
   on `hedr_version < 1.0` into `FALLOUT3` / `FALLOUT_NEW_VEGAS`, each with its
   own `SkillSet`, `NpcHealthCurve` and `RulesetBuilder`. Pinned by
   `fo3_and_fnv_profiles_build_their_own_distinct_leveling_model` and, on real
   data, by `ROSTER_CASES`' per-master profile assertion. **PASS** — this
   checklist item is stale (carried to Dimension 6).
3. **Multi-plugin profile arbitration is first-non-`NONE`-wins, not
   last-write-wins** (`index.rs:883-897`, #3384), with a `log::warn!` when a
   later plugin disagrees. This is the right direction: a third-party plugin
   with a low HEDR cannot re-point an FNV load order at the FO3 roster and
   Health curve. **PASS.**
4. **Resolve-or-skip, never a formula keyed on `0`.** Every `push_derived` in
   `fallout.rs` / `tes.rs` / `skyrim.rs` is inside an
   `if let (Some(out), Some(input)) = (resolve(…), resolve(…))`. `resolve` is
   `EsmIndex::actor_value_form_id`, which additionally rejects `0` and
   `u32::MAX` at source. **PASS.**
5. **Ordering (base AVs before dependents).** `derive_npc_actor_values` emits
   SPECIAL + the full governed skill roster + Health as one
   `Vec<(u32, f32)>` consumed by `ActorValues::from_pairs` at spawn — a single
   atomic build, so there is no window in which a dependent formula could read
   an unpopulated input. `derived_value` is only called later, from
   `condition.rs` and `byroredux/src/combat.rs`. **PASS.**
6. **FNV/FO3 class auto-calc.** `base_skill` = `SKILL_BASE 2.0 +
   SKILL_ATTR_MULT 2.0 × governing + ceil(SKILL_LUCK_MULT 0.5 × Luck)` — exactly
   the settled `2 + 2×SPECIAL + ceil(Luck/2)`. SPECIAL comes from
   `ClassRecord::base_attributes` (`ATTR`, not `DATA`), read positionally
   against `AttributeSet::FALLOUT` and pinned by `fallout_roster_matches_attr_order`.
   **PASS.** The tag-skill per-level term is **still absent** — the module
   docstring's "Deferred (intentionally, not guessed)" section states it is
   uncitable and omits it rather than approximating. **Confirmed not guessed.**
7. **`setav` / `modav` write the base layers.** `AvEdit::SetBase → set_base`,
   `AvEdit::ModPermanent → mod_permanent` (`commands/actor_value.rs:84-85`) —
   never a derived output. And `GetActorValue` (`condition.rs:483-490`) gives a
   **carried value priority** over the derived fallback, so a console edit to a
   populated AV is what the condition reads back. **PASS** — no silent revert.
8. **Templated NPCs respect the flags.** `resolve_inherited_stats` /
   `_traits` / `_inventory` all funnel through `resolve_inherited_record`, which
   follows the `TPLT` chain only while the *specific* bit is set
   (`TEMPLATE_FLAG_USE_STATS` / `_USE_TRAITS`), with a depth cap and highest-
   eligible-`LVLN`-tier pick. `derive_npc_actor_values` resolves once at the top
   for **every** stat model (#3381/#3382), and `stamp_character_components`
   resolves the same chains for `CharacterLevel` (stats source) and
   `Background.race_form_id` (traits source). **PASS.**
9. **`derive_skyrim_actor_values` per-pool independence.** The three pools are a
   `for (name, starting, offset) in [Health, Magicka, Stamina]` loop; each
   iteration does `actor_value_form_id(name).zip(starting)` and `continue`s on
   its own failure. A race with `starting_magicka: None`, or a load order missing
   the Magicka AVIF, suppresses **only** Magicka. **PASS** — the all-or-nothing
   gate is genuinely gone.
10. **#3390's `CREA` arm.** Model chosen by `npc.is_creature` (record kind) via
    `creature_stat_model()`, never by game. `CreatureStats` layout is sourced to
    xEdit `wbDefinitionsFNV.pas` and verified byte-for-byte against a real record
    (`VCrTier3GiantRadscorpionMedPers`, `00167EA7`). The three aggregate skills
    are deliberately **not** emitted because FO3/FNV publish no AVIF for them —
    the right no-guessing call. Covered by 7 unit tests plus a real-data test
    over every `FalloutNV.esm` `CREA`. **PASS** on everything it claims to do.
11. **`effective_actor_level` is the single decoder.** See Dimension 1 item 8 —
    not regressed.

### Findings

#### CHAR-2026-08-30-D5-01: every `CREA` actor's authored `DATA.Damage` is dropped at the population boundary, so 692 FNV / 186 FO3 creatures attack for the flat 8.0 unarmed baseline
- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: fnv, fo3
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:222-249` (`derive_creature_actor_values`); `crates/plugin/src/esm/records/actor/mod.rs:281` (`CreatureStats::damage`); `byroredux/src/combat.rs:318-330` (`attack_damage`)
- **Status**: NEW
- **Source**: `docs/engine/charal-fnv-fo3-ruleset.md` § Derived statistics — the Fallout family's damage model is `MeleeDamage = STR × 0.5` **additive on a weapon's own damage**, and `UnarmedDamage = ceil((10 + Unarmed)/20)`; neither is a creature's attack damage. `CreatureStats`' own docstring (sourced to xEdit `wbDefinitionsFNV.pas`) states `DATA.Damage` is "the creature's attack damage. Authored here rather than on a weapon (creatures fight unarmed)".
- **Description**: #3390 gave `CREA` records a stat model that emits their 7 SPECIAL and Health. It deliberately does not emit `DATA.Damage` — correctly, because FO3/FNV publish no `AVIF` it maps onto and inventing one would be a guess — and parks it on `CreatureStats::damage` "for a future combat consumer". That consumer already exists and shipped: `byroredux/src/combat.rs`'s `combat_damage_system` (live since 2026-08-15/16, registered in `boot.rs`). `attack_damage` reads `EquippedWeapon.damage + melee_damage_charal_bonus(...)` when a weapon is equipped and the flat `UNARMED_DAMAGE = 8.0` otherwise. Nothing anywhere reads `CreatureStats::damage` — `grep -rn 'creature_stats' --include='*.rs'` returns only the parser, `derive_creature_actor_values`, and tests.
- **Evidence**: Measured over both vanilla masters with a purpose-written probe (a temporary *crates/plugin/examples/_tmp_char_crea_dmg.rs*, run and deleted):

  | | FNV | FO3 |
  |---|---|---|
  | `CREA` records | 1,578 | 533 |
  | authoring a 17-byte `DATA` | 1,578 (100 %) | 533 (100 %) |
  | with non-zero `DATA.Damage` | 1,030 | 331 |
  | mean non-zero damage | 30.7 | 22.3 |
  | max | 500 | 1,000 |
  | **non-zero damage AND no inventory `WEAP`** | **692** | **186** |

  Samples: `QJDeathclawWanderer02` = 125, `FFEU02DeathClaw` = 100, `FFER01Radscorpion` = 60, `FFER15YaoGuai` = 75. Each of those, having no weapon to equip, resolves through `attack_damage`'s `None => UNARMED_DAMAGE` arm to **8.0** — a Deathclaw hits for 8 instead of 125, a 15.6× shortfall. The CHARAL `MeleeDamage` bonus does not compensate: `melee_damage_charal_bonus` is inside the `Some(weapon)` arm, so an unarmed creature does not even receive `STR × 0.5`.
- **Impact**: Creatures are targetable and hostile — `derive_creature_actor_values` gives them Health, and `ActorVitals` follows — so they participate fully in the shipped melee slice as aggressors, all dealing the same 8 damage regardless of species. Combat balance on FO3/FNV content is uniformly wrong for the 878 measured actors, with no crash, no log line and no failing test. The gap widened rather than narrowed with #3390: before it, creatures had no `ActorValues` at all and so were not melee-eligible; #3390 made them combat participants while leaving the one number that defines their attack unread.
- **Related**: #3390 (the `CREA` stat model); #3092 (the `MeleeDamageConfig` route this would parallel); #2962 (the unresolved "should the shipped combat consumer dispatch per-game into CHARAL math" question the `crates/core/src/combat.rs` module docs raise)
- **Suggested Fix**: Do **not** invent an AVIF for it. Follow the `MeleeDamageConfig` precedent (#3092): carry `CreatureStats::damage` onto the spawned entity as a small dedicated component (a creature-attack analogue of `EquippedWeapon`), and give `attack_damage` a third arm that prefers it over `UNARMED_DAMAGE` for an actor that carries it. If that routing is judged to belong to combat rather than CHARAL, the minimum is to file it explicitly — the current state is an unfiled, unbounded deferral inside a live system.

### Candidates verified and dropped

- **"`GameKind::Fallout3NV` collapses FO3 and FNV onto the FNV ruleset, so every
  FO3 NPC is mis-statted."** This is the checklist's own premise and it is
  **stale**: the split landed as `CharacterRulesProfile::FALLOUT3` /
  `FALLOUT_NEW_VEGAS` selected on `HEDR < 1.0`, with distinct rosters, Health
  curves and leveling models, pinned by two tests including one over real
  masters. Not a finding — reported as skill doc rot in Dimension 6.
- **"An unresolved EditorID registers a formula keyed on `0`."** Every builder
  is resolve-or-skip and `actor_value_form_id` rejects `0`/`u32::MAX` at source.
  Not possible.
- **"`setav` writes a value the next tick recomputes away."** Derived stats are
  never materialised into `ActorValues` (`charal.md` §6), and `GetActorValue`
  prefers a carried value over the derived fallback. Not a finding.
- **"The ActorValues/CharacterRuleset lock cycle."** Fixed in `b28acb0c`
  (#3441); the guard-drop-then-clone shape is present at `condition.rs:481-491`
  with its source-order regression test. **Not regressed**, per dispatch.
## Dimension 6 — Coverage, Documentation & Doctrine Drift

### Coverage matrix

Legend: **wired** = reachable from the live spawn/tick path
(`CharacterRulesProfile::build_ruleset` / `derive_npc_actor_values` / a scheduler
registration), not merely present as a buildable function.

| Game family | Ruleset built | Ruleset **wired** | Derived rows | Leveling model | NPC population | CREA population | Regen wired | Affliction wired |
|---|---|---|---|---|---|---|---|---|
| **Oblivion** | ✓ `oblivion_ruleset` | ✗ `RulesetBuilder::None` | 8 rows / 5 stats (synthetic resolver only) | ✓ `OBLIVION` (10 major-skill-ups) | ✗ `NpcStatModel::None` | ✗ (`CREA.DATA` layout unsourced) | ✗ (`oblivion_pool_regen_config` has no caller) | ✗ |
| **FO3** | ✓ `fallout3_ruleset` | ✓ | **8**, falsified against `Fallout3.esm` | ✓ `FO3` (150·L+50, cap 20) | ✓ class auto-calc | ✓ `CreatureData` (#3390) | ✗ | ✗ |
| **FNV** | ✓ `falloutnv_ruleset` | ✓ | **8**, falsified against `FalloutNV.esm` | ✓ `FNV` (150·L+50, cap 30) | ✓ class auto-calc | ✓ `CreatureData` (#3390) | ✗ | ✗ |
| **Skyrim SE** | ✓ `skyrim_ruleset` | ✗ `RulesetBuilder::None` | 2 (synthetic resolver only) | ✓ `SKYRIM` (25·L+75) | ~ `RaceBaseOffsets` — Health **+ Magicka + Stamina**, each independent | n/a (creatures folded into `NPC_`) | ✗ | ✗ |
| **FO4** | ✓ `fallout4_ruleset` | ✓ | **3**, falsified against `Fallout4.esm` | ✓ `FO4` (75·L+125) | ✓ stored `PRPS` + baked `DNAM` | n/a | ✗ | ✗ |
| **FO76** | ✗ | ✗ | — | ✗ | ~ stored (decoder shared by lineage, unverified) | n/a | ✗ | ✗ |
| **Starfield** | ✗ | ✗ | — | ✗ (curve + spend thresholds unpublished, `charal.md` §9) | ~ stored (unverified) | n/a | ✗ | ✗ |

Two structural notes the matrix alone does not carry:

- **Oblivion is not merely unwired, it is not wireable through the current
  resolver.** `Oblivion.esm` authors **no `AVIF` group at all** — asserted
  directly by `ROSTER_CASES`' `authors_actor_values: false` arm, which requires
  `index.actor_values.is_empty()`. Since every row in `oblivion_ruleset` is
  resolve-or-skip against `EsmIndex::actor_value_form_id`, flipping
  `RulesetBuilder::None` to an Oblivion arm today would build a ruleset with
  **zero** derived rows and an unresolved attribute/skill roster, silently. A
  pre-`AVIF` legacy actor-value index resolver has to land first. (Recorded as
  "n/a — pre-`AVIF`, no legacy-index resolver" in `docs/audits/AUDIT_CHARACTER_2026-08-20.md`;
  restated because it is what makes finding D6-01 below load-bearing.)
- **Skyrim's and Oblivion's derived output keys are unfalsifiable by
  construction.** `ROSTER_CASES` gives both `derived_rows: None`, which asserts
  `build_ruleset` returns `None`. Correct today, but it means the only thing
  exercising `skyrim_ruleset`'s and `oblivion_ruleset`'s output EditorIDs is a
  synthetic in-crate resolver that enumerates the same strings the builders
  pass — the exact circularity #3172 was filed to remove for the other three.

### Documentation checks

- **`mod.rs` docstring vs the live module list.** All 14 sub-modules
  (`affliction`, `attribute`, `components`, `derived`, `fallout`, `leveling`,
  `profile`, `regen`, `reputation`, `resistance`, `ruleset`, `skill`, `skyrim`,
  `tes`) are named, and `mod_docstring_indexes_every_sub_module` +
  `mod_docstring_points_at_the_charal_adjacent_siblings` enforce it mechanically.
  **PASS.**
- **Capture-document ↔ implementation coverage.** All six captures exist. FO76
  and Starfield have captures and no implementation, both explicitly noted in
  `charal.md` §8 items 8 / §9 and in `feature-matrix.md`. **No silent scope
  loss.**
- **Vocabulary.** `translate` / `canonical` / `resolve` / `derive` used
  throughout; `CharacterRulesProfile` / `NpcStatModel` / `RulesetBuilder` are
  data-row nouns, not a competing verb set. `MeleeDamageConfig` (#3092) mirrors
  `PoolRegenConfig`'s established "pre-resolve once, consume by id" shape rather
  than inventing a new one. **No drift.**

### Findings

#### CHAR-2026-08-30-D6-01: `charal.md` declares Oblivion "CHARAL-complete end-to-end" for a ruleset that has no construction site and could not resolve a single AVIF if it did
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: oblivion
- **Location**: `docs/engine/charal.md:343`; echoed at `docs/engine/charal-oblivion-ruleset.md:7`
- **Status**: NEW
- **Description**: §5 ends its Oblivion paragraph *"**Oblivion is now CHARAL-complete** end-to-end."* Two independent facts contradict it. (1) `CharacterRulesProfile::OBLIVION` carries `ruleset: RulesetBuilder::None`, so `build_ruleset` returns `None` and no Oblivion `CharacterRuleset` is ever constructed at load. (2) More fundamentally, `Oblivion.esm` authors no `AVIF` records at all — Oblivion predates the record type — so every one of `oblivion_ruleset`'s eight resolve-or-skip rows would skip and both rosters would resolve empty even if arm (1) were added.
- **Evidence**: `profile.rs:82-87` (`OBLIVION` → `RulesetBuilder::None`); `crates/plugin/tests/parse_real_esm.rs:286-295` — the Oblivion `RosterCase` carries `authors_actor_values: false`, and `assert_rosters_resolve` asserts `index.actor_values.is_empty()` with the comment *"if it now does, its rosters became falsifiable and this case should assert them"*. `docs/feature-matrix.md:250` gets it right ("~ built, unwired"), so the design doc is the outlier, not the matrix.
- **Impact**: "end-to-end" is the phrase a future contributor greps for when deciding what is left to do on Oblivion. It hides the *actual* blocker — a legacy actor-value index resolver for a pre-`AVIF` game — behind a completion claim, and the child capture repeats it, so cross-checking the two documents confirms rather than corrects it. This is `feedback_audit_findings`' stale-premise class at the doc layer: a future audit that trusts §5 would mark Oblivion done and stop looking.
- **Related**: #3170 (Skyrim's parallel unwired-ruleset issue); the Dimension-1 coverage note
- **Suggested Fix**: Reword to "the Oblivion **ruleset builder** is complete; it is unwired (`RulesetBuilder::None`) and additionally blocked on a pre-`AVIF` legacy actor-value resolver, since `Oblivion.esm` authors no `AVIF` group." Correct the echo in `charal-oblivion-ruleset.md:7` the same way.

#### CHAR-2026-08-30-D6-02: `charal.md` §8 item 6 still lists `fXPPerSkillRank` among the GMSTs the Skyrim curve overlays — a read withdrawn 2026-08-24 and absent from the code
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: skyrim
- **Location**: `docs/engine/charal.md:589-592`
- **Status**: NEW
- **Source**: `docs/engine/charal-skyrim-ruleset.md:711-720` — *"A skill raised to rank `R` awards `R` character XP; that coefficient is an engine rule, **not** a `fXPPerSkillRank` GMST."*
- **Description**: The rollout section reads *"Skyrim's XP curve now overlays the authored `fXPLevelUpBase`, `fXPLevelUpMult`, and `fXPPerSkillRank` values with sourced fallbacks."* `LevelingModel::with_gmst` requests exactly two settings and carries `xp_per_skill_rank` through untouched; the third read was removed as a settled design decision when the coefficient was reclassified engine-owned. The parent design doc is now the only place in the repository that still asserts the withdrawn behaviour, contradicting both its own child capture and the code.
- **Evidence**: `crates/core/src/character/leveling.rs:93-108` — `gmst("fXPLevelUpBase")`, `gmst("fXPLevelUpMult")`, then `xp_per_skill_rank,` passed through with no lookup; guarded by `skyrim_gmst_overlay_reads_only_authored_curve_settings`. `grep -rn 'fXPPerSkillRank' docs/engine/` returns `charal.md:591` (asserting it) and `charal-skyrim-ruleset.md:717` (denying it).
- **Impact**: `charal.md` is the layer spec and the first document `/audit-character` Phase 1 loads, ahead of the per-game captures. An auditor reading them in that order meets the withdrawn claim first. It is also live evidence that #3221 — filed against `leveling.rs:92` and `charal-skyrim-ruleset.md:711-720`, **both since corrected** — is fixed at the two locations it names while its subject survives at a third the issue never mentioned.
- **Related**: #3221 (OPEN; both locations it cites are already fixed — worth closing with this line as the remaining edit)
- **Suggested Fix**: Drop `fXPPerSkillRank` from the §8 item 6 sentence and add the half-clause the capture document uses ("only the level curve is GMST-authored; the skill-rank coefficient is engine-owned").

#### CHAR-2026-08-30-D6-03: the `/audit-character` skill's own Dimension-5 checklist asserts an FO3↔FNV ruleset collapse that was removed months ago, and asks auditors to verify a justification for it
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: fo3, fnv
- **Location**: `.claude/commands/audit-character/SKILL.md:306-309`
- **Status**: NEW
- **Description**: The Dimension-5 checklist reads: *"`GameKind::Fallout3NV` resolves to the **FNV** ruleset for both FO3 and FNV, justified because the actor-general derived stats are identical. Verify that justification against both capture documents — if any actor-general coefficient differs, the collapse is wrong and every FO3 NPC is mis-statted."* No such collapse exists. `character_rules_profile` splits the shared `GameKind::Fallout3NV` on `hedr_version < 1.0` into `CharacterRulesProfile::FALLOUT3` and `FALLOUT_NEW_VEGAS`, which carry different skill rosters (`FALLOUT3` vs `FALLOUT_NV`), different `NpcHealthCurve`s (bias 90/END 20/level 10 vs 95/20/5) and different `RulesetBuilder` arms.
- **Evidence**: `crates/plugin/src/esm/records/mod.rs:149-158`; `crates/core/src/character/profile.rs:89-118`. Pinned by `fo3_and_fnv_profiles_build_their_own_distinct_leveling_model` (`profile.rs`) and by `ROSTER_CASES`' per-master profile assertion over both real masters.
- **Impact**: A checklist item is an instruction to spend audit effort. This one directs an auditor to verify a premise that cannot be true and to reach a conclusion ("the collapse is wrong and every FO3 NPC is mis-statted") that would be a false CRITICAL if reported without checking the code — the exact failure `feedback_audit_findings` records (~5 of 30 findings in the 2026-04 sweep were stale premises). Every prior report in `docs/audits/` had to spend the same effort re-falsifying it.
- **Related**: #3485, #3486 (the two other open `audit-character` SKILL.md doc-rot items); `feedback_audit_findings`
- **Suggested Fix**: Replace with a positive regression check: "the FO3/FNV split is `HEDR < 1.0` in `character_rules_profile`; verify both profiles still carry distinct rosters, Health curves and `RulesetBuilder` arms, and that multi-plugin arbitration is still first-non-`NONE`-wins (#3384) rather than last-write-wins."

#### CHAR-2026-08-30-D6-04: `docs/feature-matrix.md`'s CHARAL section has no row or prose for the `CREA` stat model, three days after it shipped
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: fo3, fnv
- **Location**: `docs/feature-matrix.md:248-272`
- **Status**: NEW
- **Description**: #3390 added a fourth NPC stat model, `NpcStatModel::CreatureData`, populating SPECIAL + Health for 1,578 FNV and 533 FO3 `CREA` records from the record's own `DATA`. The matrix's "NPC actor-value population at spawn" row still reads `✓ class auto-calc` for FO3/FNV, and the prose paragraph beneath enumerates the mechanisms — "class auto-calc", "Health only", "stored" — without mentioning creatures at all. `grep -n 'CREA\|creature' docs/feature-matrix.md` finds only an unrelated physics row.
- **Evidence**: `crates/core/src/character/profile.rs:37-44` (`NpcStatModel::CreatureData`) + `:100`/`:115` (`creature_stats: NpcStatModel::CreatureData` on both Fallout profiles); `crates/plugin/src/esm/records/actor_value_derive.rs:222` (`derive_creature_actor_values`). Landed in `a1327227`; the matrix section is unchanged since before it.
- **Impact**: The matrix is this project's designated "what actually works per game" artifact and is documented as lagging the code, so a lag is reportable doc rot. Here it under-reports shipped coverage on the two reference titles — 2,111 actors' worth — which is the direction that causes duplicated work rather than false confidence, but is still wrong. It also leaves no place to record the gap CHAR-2026-08-30-D5-01 identifies (creature attack damage parsed but unconsumed).
- **Related**: #3484 (the same section's Skyrim population row, stale in the opposite direction — OPEN); #3390; CHAR-2026-08-30-D5-01
- **Suggested Fix**: Add a "Creature (`CREA`) actor-value population" row (FO3 ✓, FNV ✓, Oblivion ✗ — `CREA.DATA` layout unsourced, others n/a) and one prose sentence naming the model and what it deliberately omits (the three aggregate skills, and `DATA.Damage`).

### Candidates verified and dropped

- **"`mod.rs`'s docstring has drifted from the module list."** All 14 present and
  mechanically pinned by two tests. Not a finding.
- **"`DerivedStatFormula` is documented at 32 B but is 36 B."** Confirmed, but
  **Existing: #3485 (OPEN)** against the SKILL.md pin. The struct's own docstring
  and `formula_is_thirty_six_bytes_and_copy` are both already correct. Not
  re-filed.
- **"`docs/feature-matrix.md` records Skyrim NPC population as Health-only."**
  Confirmed still stale (`:266`) — **Existing: #3484 (OPEN)**. Not re-filed.

---

## Known-Open Register

Restated and confirmed **not re-filed** as findings:

| Item | Status this pass |
|---|---|
| **FNV/FO3 tag-skill per-level formula is undocumented and deliberately deferred** (*actor_value_population*) | Confirmed still absent, not approximated. `actor_value_derive.rs`'s "Deferred (intentionally, not guessed)" section states it is uncitable and omits it. The implemented half, `base_skill` = `2 + 2×governing + ceil(0.5×Luck)`, verified exact. `CLAS` SPECIAL is read from `ATTR` (`ClassRecord::base_attributes`), not `DATA`. **Not re-derived, not invented.** |
| **FO3↔FNV divergent *player* Health/AP deferred with the player actor** | Confirmed. Both rows are `.player_only()`; no player stat-bearing entity exists. The FO3/FNV **profile** split (rosters, `NpcHealthCurve`, leveling) has landed and is verified — that is a different axis from the deferred player-scoped divergence. |
| **VATS runtime does not exist; only the AP formulas are in CHARAL** (*vats_system*) | Confirmed. `fallout3_ruleset`/`falloutnv_ruleset`/`fallout4_ruleset` carry the AP formulas; no AP pool, regen, time-pause, limb health, hit-chance roll, crit or kill-cam exists. **Absence not reported as a defect.** |
| **Oblivion / Skyrim rulesets exist but are UNWIRED** | Confirmed (`RulesetBuilder::None` on both profiles). Reported **only** where a new consequence was shown: the Oblivion doc claim (D6-01) and the derived-key falsifiability gap in the coverage matrix. Not re-filed as a bug. |
| **The `ActorValues`/`CharacterRuleset` lock cycle** (broken in `b28acb0c`, #3441) | Verified **not regressed**. `condition.rs:481-491` snapshots `ActorValues` and drops the storage guard before acquiring `CharacterRuleset`, with the source-order regression test still in place. |
| **`stealth.rs` sub-coefficients unsourced** | Existing: **#3482 (OPEN)**. Confirmed unchanged; not re-filed. |
| **Flat Fatigue regen gated behind the `CharacterRuleset` lookup only Magicka needs** | Existing: **#3483 (OPEN)**. Confirmed unchanged; not re-filed. |
| **`let config = *config;` shadows rather than drops the guard (#2153's unrealised hold-stack reduction)** | Existing: **#3444 (OPEN)**. Confirmed unchanged; not re-filed. |
| **`DerivedStatFormula` pinned at 32 B in SKILL.md, actually 36 B** | Existing: **#3485 (OPEN)**. Confirmed; not re-filed. |
| **`feature-matrix.md` Skyrim NPC-population row stale** | Existing: **#3484 (OPEN)**. Confirmed; a *different* row of the same section is filed as D6-04. |

## Cross-audit routing

- Component storage/shape (`Perks`, `AfflictionStatus`, `FactionReputation`) → `/audit-ecs`.
- `AVIF` / `CLAS` / `NPC_` / `CREA` wire-format decoding → `/audit-esm` Dim 4. In particular the still-open FO3/FNV `NPC_` DNAM SPECIAL/skill block, which blocks the ~40 % of actors with auto-calc OFF.
- `CTDA` condition evaluation (`GetActorValue` 14, `HasPerk`, `GetReputation` 573/575, `GetXPForNextLevel` 533) → `/audit-scripting`.
- Scheduler access declarations → `/audit-concurrency` Dim 4 (the `pool_regen_tick_system` declaration specifically was verified here and matches).
- The melee-damage routing question raised by CHAR-2026-08-30-D5-01 touches `byroredux/src/combat.rs`, which no audit skill currently owns — the same ownership gap #2962 records for `crates/core/src/combat.rs`.

## Process notes

- Every dimension was run **in-process**; no sub-agents were dispatched, so no
  dimension could be silently lost to an unretrievable sub-agent result.
- Scratch files were written to */tmp/audit/character/dim_N.md* as each dimension
  completed and consolidated from there. The directory held one stale
  `issues.json` from a 2026-08-28 run; it was deleted and regenerated before use,
  and no stale *dim_N.md* existed.
- **Nothing was launched.** No `byroredux` process was started; no GitHub issues
  were created. Three temporary parser probes were compiled, run and deleted.

---

**7 findings — 0 CRITICAL · 0 HIGH · 1 MEDIUM · 6 LOW.**

Suggested next step:

```
/audit-publish docs/audits/AUDIT_CHARACTER_2026-08-30.md
```

Domain label `character`; add `game:fnv` + `game:fo3` (D5-01, D6-04),
`game:oblivion` (D6-01), `game:skyrim` (D6-02).
