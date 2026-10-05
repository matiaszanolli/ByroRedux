**HEAD**: `a2c24b16e` · **Baseline**: [`AUDIT_CHARACTER_2026-10-03.md`](AUDIT_CHARACTER_2026-10-03.md) (HEAD `2c36c29d8`) · **Audited**: Dim 1 (Ruleset Seam), Dim 2 (Derived Formulas, a doc-and-pin delta only), Dim 4 (Population Boundary), Dim 5 (Coverage & Doctrine) · **Unchanged since baseline (skimmed)**: Dim 3 (Progression & Pools: no commits to `leveling/regen/affliction/reputation/components/skyrim/tes.rs`)

# Character / CHARAL Audit — 2026-10-05

This run is `/audit-character` with the default scope and `--depth deep`, run as part of `/audit-suite --preset comprehensive`. I analysed every dimension myself, in order, with no sub-agents.

The delta is `2c36c29d8..a2c24b16e`: 88 commits, of which three are in scope:
- `c71eeed80` (#5238): FO3/FNV NPC Health now seeds from the authored NPC curve instead of the player curve.
- `297a0e64a` (#5238 follow-up): the bin test fixtures were re-derived.
- `3826c2792` (#5240): the GECK *Stats Tab — NPC* citations were added to the scopes of the shared derived rows.

The other commits that touch population paths (`717f39a82` #5223 corpse-trigger idiom, `7ead491d7` #5061 appearance loaders, `ccc743160` footsteps) are outside CHARAL.

## Tests recorded (read-only; no engine launched, no `--ignored`, no GPU)

| Command | Result |
|---|---|
| `cargo test -p byroredux-core --features inspect -- character combat stealth` | 164 passed, 0 failed. That is +2 vs 162: the new `fo3_fnv_npc_health_curve_reproduces_the_vanilla_samples` and `npc_health_curve_overlays_authored_gmsts`. |
| `cargo test -p byroredux-plugin --lib -- actor_value_derive consumables::tests::fn586 equip` | 103 passed, 1 ignored |
| `cargo test -p byroredux --bin byroredux -- resolve_inherited obscript_dialect player_character_template attach_to_player vitals_snapshot vital melee_damage player_npc_form_id modav setav player_derived reset_player_factions npc_spawn` (rustc 1.96) | 132 passed, 9 ignored. The working tree compiles cleanly, so this ran in place. |
| Real-master probe, outside the repo | Two read-only tools. **(a)** `/mnt/data/tmp/char_probe` `npchp`, rebuilt against the current tree, gives the seeded Health for named NPCs. **(b)** Python walkers in the session scratchpad cover: the raw `NPC_`/`CLAS` census (`DATA` SPECIAL vs `CLAS` `ATTR`, split by the ACBS auto-calc flag); the authored `fAVD*Health*` GMST values in both masters; and the exe default-Setting tables in `Fallout3.exe` and `FalloutNV.exe`. |

## Executive Summary

**3 new findings: 0 CRITICAL · 0 HIGH · 1 MEDIUM · 2 LOW. 0 regressions.**
- The two baseline findings that were fixed are verified on HEAD:
  - #5238: the NPC Health curve is now sourced, and both the plugin and bin pins are green.
  - #5240: the GECK citations are quoted exactly from the local dump.
- The other three baseline findings are still open and still present. The code is unchanged, so they are cited, not re-filed:
  - #5239: a player `setav` on Health/AP is reverted by the per-frame refresh.
  - #5241: the scheduler comment sits on the wrong registration.
  - #5242: a `feature-matrix.md` paragraph still describes FO76/Starfield as wired.
- Known-open #4137, #4232, #4415 and #5079 are re-confirmed open.

**The #5238 curve is right, but its END input is wrong for about 40% of FO3/FNV NPCs (D4-01, MEDIUM).**
- What is right: the curve `base + 5·(END−1) + 5·(L−1)` reproduces the in-game Health of two non-auto-calc named NPCs exactly, provided it is fed their **authored** `NPC_ DATA` SPECIAL.
- What is wrong: the population path still takes SPECIAL from the **class** `ATTR` for every actor, ignoring the ACBS "Auto-calc stats" bit.
  - Jason Bright: wiki SPECIAL `6,5,7,2,6,6,3`, which equals his record `DATA`. HP 135 = 80 + 5·6 + 5·5. The engine seeds **120** from class END 4.
  - Martina Groesbeck: wiki HP 80. The engine seeds **90**.
- Scale: 1,495 of 1,533 FNV and 708 of 712 FO3 non-auto-calc `NPC_` records author a SPECIAL that differs from their class.
- The deferral that covers this (#2957, closed as documented) gives "the stored values are unparsed" as its blocker. That is no longer true for SPECIAL: #5005 already parses the 11-byte `DATA` sub-record that carries it. #5238 now reads Base Health from that record while taking END from the class.

**Constants:** no coefficient moved in Dim 2/3 code, the fifth run in a row with no drift.
- The FO3/FNV NPC Health row is now graded against primary sources:
  - the authored master GMSTs (5.0 / 5.0);
  - the exe default `fAVDNPCHealthEnduranceOffset` = −1.0, read directly from `Fallout3.exe`'s Setting table (vtable, value, name-pointer triplet; value at `0xcf8a98`, name pointer at `0xcf8a9c`);
  - the GECK.
- One modelling choice inside the curve is unsourced: the endurance offset is also applied to the level term (D1-01, LOW).

**Wiring state is unchanged:**
- `PoolRegenConfig` has zero production inserts.
- `affliction_tick_system` is unregistered.
- `level_cap()` has test consumers only.
- `FactionReputation` has no production insert.

## Constant Verification Table

Doc keys: **FNV/FO3** = `charal-fnv-fo3-ruleset.md`, **FO4** = `charal-fo4-ruleset.md`, **OBL** = `charal-oblivion-ruleset.md`, **SKY** = `charal-skyrim-ruleset.md`, **GECK** = local geck.uesp dump `(main)/Stats Tab - NPC.wiki`, **ESM** = authored GMST read from the vanilla master, **EXE** = default read from the executable's Setting table.

### FO3/FNV NPC Health (re-graded this run, after #5238)

| Row | Code | Document / data | Verdict |
|---|---|---|---|
| `fAVDNPCHealthEnduranceMult` = 5 | `NpcHealthCurve.endurance_multiplier` 5.0, plus the `with_gmst` overlay | ESM: `Fallout3.esm` and `FalloutNV.esm` `DATA` = 5.0 (verified this run) | PASS |
| `fAVDNPCHealthLevelMult` = 5 | `level_multiplier` 5.0, plus the overlay | ESM: both masters = 5.0 | PASS |
| END-term offset −1 | `offset` −1.0 on the END term | EXE: `Fallout3.exe` default −1.0 (value `0xcf8a98`, name ptr `0xcf8a9c`). No FO3/FNV plugin on disk authors it. | PASS |
| Level-term offset −1 | the **same** `offset` field, applied to the level term | No `fAVDNPCHealthLevelOffset` exists in either exe. The value is right at vanilla, but its binding to the END offset is **UNSOURCED** | PASS on value · **binding UNSOURCED → D1-01** |
| `DATA` Base Health additive term | `npc.data_base_health.unwrap_or(0).max(0)` | GECK: "Health is calculated with Endurance and level. This value is then added to that result." | PASS |
| END input | **class `ATTR` END for every actor** | GECK: "Attributes may not be changed if Autocalc is checked". Authored `DATA` SPECIAL matches the wiki for non-auto-calc NPCs (Jason Bright, Martina Groesbeck). | **FAIL for auto-calc-OFF actors → D4-01** |
| Vanilla samples | profile.rs pins 8 (5 FNV + 3 FO3); the `#[ignore]` real-master test pins 3 | fandom infobox `hp` | PASS. The docs say "15/15 pinned", which is D5-01. |

### Derived-stat formulas (Dim 2): code unchanged, so the baseline verdicts carry forward

| Formula / constant | Code | Document | Verdict |
|---|---|---|---|
| FO4 Health `floor(77.5+4.5·END+2.5·L+0.5·L·END)` player-only (the only cross-term row) | `fallout4_ruleset` | FO4 | PASS |
| FO4 AP `60+10·AGI` · CW `200+10·STR` | `fallout4_ruleset` | FO4 | PASS ×2 |
| FO3 player Health `90+20·END+10·L` | `fallout3_ruleset` | FO4 "Cross-game Health" table only. **The FNV/FO3 capture no longer carries the row** (D5-01). | PASS on value; the sourcing doc has moved |
| FNV player Health `95+20·END+5·L` | `falloutnv_ruleset` | same | PASS on value; same caveat |
| FO3 AP `65+2·AGI` cap 85 · FNV `65+3·AGI` cap 95, both `.player_only()` | FO3/FNV rows | FNV/FO3. Scope is #2937, closed as documented; the GECK has no AP entry. | PASS |
| CW `150+10·STR` · Crit `1·Luck` cap 10 · Unarmed `ceil(0.5+0.05·U)` actor-general | shared rows | FNV/FO3 + GECK (#5240) | PASS. Scope is now sourced. |
| Melee `0.5·STR` actor-general | shared row | GECK "Not used." CONFLICTED, deliberately retained (#5240) | Known-open, not re-filed |
| RadResist `(END−1)·2` cap 85 · PoisonResist `(END−1)·5` | `resistance.rs` | FNV/FO3 + GECK | PASS |
| Oblivion Health/Magicka/Fatigue/Armor, Skyrim Light Armor / CW | `tes.rs` / `skyrim_ruleset` | OBL / SKY | PASS (unchanged) |
| `combat.rs` / `stealth.rs` coefficients | unchanged | OBL damage formula; FNV/FO3 Sneak Detection | PASS |

### Progression, pools and reputation (Dim 3)
The code is unchanged, so the baseline's 60 rows stand.

## Coverage Matrix

Re-derived from the `profile.rs` arms, workspace greps and scheduler registrations:

| Family | Ruleset impl | Wired | Derived rows | Leveling model | NPC seed | Player seed | Regen wired | Affliction wired |
|---|---|---|---|---|---|---|---|---|
| FO3 | ✓ `fallout3_ruleset` | ✓ `RulesetBuilder::Fallout3` | 8 | ✓ `150·L+50`, cap 20 (model only) | ✓ auto-calc NPCs. **Non-auto-calc NPCs get class SPECIAL/END (D4-01).** Health curve sourced (#5238). | ✓ HP/AP refreshed per frame; `setav` reverted (#5239) | ✗ registered-inert | ✗ unregistered |
| FNV | ✓ `falloutnv_ruleset` | ✓ `RulesetBuilder::FalloutNewVegas` | 8 | ✓ `150·L+50`, cap 30 | as FO3 | as FO3 | ✗ | ✗ |
| FO4 | ✓ `fallout4_ruleset` | ✓ `RulesetBuilder::Fallout4` | 3 | ✓ `75·L+125`, uncapped | ✓ stored `PRPS` + `DNAM` | ✓ HP/AP refreshed per frame | ✗ | ✗ |
| Skyrim SE | ✓ `skyrim_ruleset` | ✓ `RulesetBuilder::Skyrim` | 2 | ✓ `25·L+75` + skill-XP | ✓ race + offset (class/level term deferred, disclosed) | ✓ race + offset | ✗ | ✗ |
| Oblivion | ✓ `oblivion_ruleset` | ✗ `RulesetBuilder::None` (deliberate, pinned) | tests only | ✓ 10 major-skill-ups | ✗ | ✗ | ✗ | ✗ |
| FO76 | ✗ | ✗ | ✗ | ✗ curve LOCKED, uncoded | ✗ `NpcStatModel::None` (#4453) | ✗ | ✗ | ✗ |
| Starfield | ✗ | ✗ | ✗ | ✗ PENDING | ✗ `NpcStatModel::None` (#4453) | ✗ (vital roster sourced, #5044) | ✗ | ✗ |

## Findings

**MEDIUM (1)**
- D4-01: FO3/FNV non-auto-calc NPCs are seeded with class SPECIAL and END instead of their authored `DATA` SPECIAL, and the deferral's "unparsed" blocker is now stale.

**LOW (2)**
- D1-01: the NPC Health level term borrows `fAVDNPCHealthEnduranceOffset`, which is an unsourced binding.
- D5-01: #5238 replaced the capture's player Health row and left five code and doc sites describing the old state.

---

### CHAR-2026-10-05-D4-01: FO3/FNV non-auto-calc NPCs (about 40% of actors) are still seeded with their **class** SPECIAL, not their authored `NPC_ DATA` SPECIAL. #5005 already parses that sub-record and #5238 now reads Base Health from it, so the #2957 "unparsed" blocker is stale and the seeded Health is wrong for those actors (Jason Bright 120 vs 135, Martina Groesbeck 90 vs 80)
- **Severity**: MEDIUM. It is the same class as #5238 (HIGH), but it hits a disclosed deferral, and the per-actor error is mostly ±1–3 SPECIAL points.
- **Dimension**: Population Boundary
- **Game**: fo3, fnv
- **Location**:
  - `crates/plugin/src/esm/records/actor_value_derive.rs`: `derive_autocalc_actor_values`, which reads `class.base_attributes` for every actor and never consults `acbs_flags & 0x10`. Also the module doc's "Non-auto-calc NPCs" deferral, whose "unparsed … DNAM-era layout" claim is stale.
  - `crates/plugin/src/esm/records/actor/mod.rs`: the FO3/FNV `NPC_` `b"DATA"` arm in `parse_npc`, which decodes only the i32 and discards the 7 SPECIAL bytes it already holds.
- **Status**: NEW. It is related to #2957, which was closed as documented on 2026-08-21. That issue only corrected the note's stated *scale*; its blocker ("the stored values are … unparsed") was true then and stopped being true for SPECIAL when #5005 (`de6bdb381`) added the 11/25-byte `DATA` arm.
- **Source**:
  - GECK *Stats Tab - NPC*, from the local dump: "The Attributes and skills will be automatically calculated if [Autocalc] is checked … **Attributes may not be changed if Autocalc is checked.**" When the box is clear, the attributes are authored on the NPC.
  - The `parse_npc` comment of #5005 itself: "`NPC_` `DATA`: i32 Base Health + the 7 SPECIAL attributes (11 bytes)".
  - fandom infobox (*Jason Bright*): `special = 6,5,7,2,6,6,3`, `level = 6`, `Hit Points: 135`.
  - fandom infobox (*Martina Groesbeck*): `special = 4,6,4,5,4,7,3`, `level = 6`, `Hit Points: 80`.
- **Description**: The population path assumes every FO3/FNV `NPC_` is auto-calculated. For a record with the ACBS "Auto-calc stats" bit clear, the engine uses the SPECIAL the record authors in `DATA` bytes 4–10, and that SPECIAL differs from the class in nearly every such record. #5238's curve is correct: fed the authored END, it reproduces both samples below exactly. The defect is that its END input, and all seven seeded SPECIAL values, come from the class.
- **Evidence**:
  - Census (raw `NPC_`/`CLAS` walk of the masters; ACBS bit `0x10`):

    | | FNV | FO3 |
    |---|---|---|
    | auto-calc OFF | 1,533 | 712 |
    | of which `DATA` SPECIAL ≠ class `ATTR` | **1,495** | **708** |
    | auto-calc ON, `DATA` ≠ class | 159 / 2,283 | 0 / 935 |

    FO3's 0 / 935 confirms that the GECK writes the class attributes into `DATA` when auto-calc is on. The `DATA` bytes are therefore the engine's SPECIAL, and the class is only the source the GECK copies from.
  - Seed vs wiki. The probe (`npchp`) runs the current tree's `derive_resolved_actor_values`:

    | NPC | auto-calc | `DATA` END | class END | Base | L | Seeded | Wiki HP | `base+5·(DATA END−1)+5·(L−1)` |
    |---|---|---|---|---|---|---|---|---|
    | GhoulJasonBright | off | 7 | 4 | 80 | 6 | **120** | 135 | 135 |
    | MartinaGroesbeck | off | 4 | 6 | 40 | 6 | **90** | 80 | 80 |
    | GSEasyPete (control) | off | 4 | 4 | 50 | 1 | 65 | 65 | 65 |

  - All 15 of the #5238 vanilla samples are auto-calc, or (Easy Pete) carry a `DATA` SPECIAL equal to the class's. The pin set therefore cannot detect this.
- **Impact**:
  - About 1,500 FNV and 700 FO3 actors, disproportionately the hand-authored named NPCs that quests and dialogue target, spawn with the class's SPECIAL.
  - That error flows into everything SPECIAL drives: Health (via END), the 13 auto-calc skills, Carry Weight, Crit, the resists, and every `GetActorValue <SPECIAL>` / skill CTDA.
  - Skills for these actors are also the class-derived formula rather than their authored `DNAM` values. That half is still genuinely unparsed.
- **Related**: #2957 (the closed deferral); #5005 (parses the sub-record); #5238 (now half-reads it); `/audit-esm` Dim 4 owns the parse.
- **Suggested Fix**:
  1. In the FO3/FNV `DATA` arm, store the 7 SPECIAL bytes, for example as `NpcRecord::data_attributes: Option<[u8; 7]>`.
  2. In `derive_autocalc_actor_values`, use them when `acbs_flags & 0x10 == 0`. Read the flag from the resolved `Use Stats` record, which is the record the attributes come from.
  3. Correct the module doc so it says only the `DNAM` skill half remains unparsed.
  4. Add a real-master `#[ignore]` pin for GhoulJasonBright = 135 and MartinaGroesbeck = 80.

### CHAR-2026-10-05-D1-01: `NpcHealthCurve` applies `fAVDNPCHealthEnduranceOffset` to the **level** term as well. No level-offset setting exists, and the player analogue shows the level anchor is independent of the endurance offset
- **Severity**: LOW. It is value-identical on every vanilla FO3/FNV load order, and diverges only when a plugin authors `fAVDNPCHealthEnduranceOffset`.
- **Dimension**: Ruleset Seam (profile row sourcing)
- **Game**: fo3, fnv
- **Location**: `crates/core/src/character/profile.rs`:
  - `NpcHealthCurve::evaluate`: `self.level_multiplier * (level + self.offset)`.
  - `NpcHealthCurve::with_gmst`: `offset: gmst("fAVDNPCHealthEnduranceOffset")`.
  - the `npc_health_curve_overlays_authored_gmsts` pin.
  - The same formula is in `docs/engine/charal-fnv-fo3-ruleset.md`'s Health (NPCs) paragraph: "`fAVDNPCHealthLevelMult·(Level + fAVDNPCHealthEnduranceOffset)`".
- **Status**: NEW (`c71eeed80`).
- **Source**:
  - (1) `strings` over `Fallout3.exe` and `FalloutNV.exe`: the only NPC Health settings are `fAVDNPCHealthEnduranceMult`, `fAVDNPCHealthEnduranceOffset` and `fAVDNPCHealthLevelMult`. There is no level offset.
  - (2) Both masters author the player-side `fAVDHealthEnduranceOffset` = **0.0** (verified this run). Yet the sourced player formulas still anchor the level term. FNV is `100 + 20·END + 5·(Level−1)` per the FO4 capture's "Cross-game Health" table, which also notes "FNV also re-anchors the level term to `(Level − 1)`". With the endurance offset at 0, the player's level anchor cannot come from it.
  - (3) The commit message itself says "Applied per-term it composes to the −10 constant", which is a fit, not a citation.
- **Description**: The vanilla constant (−10 = −5 END + −5 level) is right. The model, though, binds the level term's −1 to a setting named for Endurance. That is the "plausible decomposition" the no-guessing rule warns against: a mod that retunes `fAVDNPCHealthEnduranceOffset` (for example to −2) would move every NPC's Health by `5·Δ` twice instead of once. The overlay test pins exactly that behaviour.
- **Impact**: None on vanilla data; no shipped FO3/FNV plugin authors the setting. Under a mod that does, every auto-calc NPC's Health is off by `level_multiplier·Δ`.
- **Suggested Fix**: Give the level term its own engine constant, for example a `level_anchor: f32 = -1.0` field documented as engine-hardcoded and mirroring the player formula's `(Level−1)`, that no GMST overlays. Alternatively, keep the binding but mark it UNSOURCED in the doc and the capture until an exe disassembly settles it.

### CHAR-2026-10-05-D5-01: #5238 replaced, rather than split, the capture's Health row. The FO3/FNV player Health formula and its "(player)" scope no longer appear in `charal-fnv-fo3-ruleset.md`, and five code and doc sites still describe the old state
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: fo3, fnv
- **Location and evidence**:
  1. `docs/engine/charal-fnv-fo3-ruleset.md`, the derived table. The row `| Health | … | 90 + END·20 + Level·10 | 100 + END·20 + (Level−1)·5 | BUILT (player ruleset + NPC auto-calc seed) |` became `| Health (NPC) | … |`. The player formulas that `fallout3_ruleset` / `falloutnv_ruleset` encode, with their bias 90/95 and `.player_only()` scope, now have no row in their own capture. They survive only in `charal-fo4-ruleset.md`'s cross-game table, which is labelled "file into the sibling rulesets when opened". The prose keeps only the multipliers.
  2. `crates/core/src/character/fallout.rs`, the `fallout3_ruleset` AP-row comment: "`charal-fnv-fo3-ruleset.md`'s derived-stat table annotates scope on every other locked row (Health "(player)", …)". The doc of `fo3_fnv_action_points_scope_is_player_only_pending_a_source` likewise says "Health's player-only scope, which the capture document does state". Both are now false.
  3. `crates/plugin/src/esm/records/actor_value_derive.rs`, the module doc `## Model (cited)`: "**Health**: FO3 `90 + 20·END + 10·Level`; FNV `100 + 20·END + 5·(Level−1)`, from the locked CHARAL ruleset capture". That is the retired NPC curve; #5238 updated the function but not this header. The "unparsed" claim in the same doc belongs to D4-01.
  4. The pin count is overstated. The `NpcHealthCurve` doc, the capture paragraph and the audit skill all say "15/15 … pinned by `fo3_fnv_npc_health_curve_reproduces_the_vanilla_samples`", but that test asserts 8 samples (5 FNV + 3 FO3). The real-master `#[ignore]` test asserts 3.
  5. `byroredux/src/inventory.rs`, the player-template test message "FNV Health 95 + 20·END(5) + 5·L(1), **the NPC curve** the player record's class derives". The 200 is the `PlayerOnly` row; the NPC curve gives 20.

  Also, `ROADMAP.md` ("CHARAL runtime-dead surface") still opens with "The formulas are right: 62 constants were verified with zero mismatch", citing 2026-08-15. #5238 falsified that for the NPC Health row.
- **Status**: NEW (`c71eeed80`, `297a0e64a`).
- **Impact**: The next auditor looking for the source of FO3/FNV player Health finds none in its own capture, and code comments assert a capture annotation that is gone. This is the "a row with no document value is UNSOURCED" trap, which bites only because the row moved.
- **Suggested Fix**:
  - Restore a `Health (player)` row beside `Health (NPC)`, carrying the two player formulas and the "(player)" scope. Cite the fandom *Hit Points* page and the authored `fAVDHealth*` GMSTs (FO3 20/10, FNV 20/5, endurance offset 0.0).
  - Fix the `actor_value_derive.rs` header, the two `fallout.rs` comments and the `inventory.rs` message.
  - Change "15/15 pinned" to "8 pinned + 3 real-master", or add the remaining 7 samples to the pin.
  - Soften the ROADMAP sentence.

## Observations (not findings)

- **FNV auto-calc records whose `DATA` SPECIAL differs from their class (159 of 2,283).** FO3 has zero such records. The GECK says auto-calc attributes come from "the level and class", so these may be level-adjusted values the GECK baked in. If D4-01's fix reads `DATA` only when auto-calc is OFF, these stay on the class answer. Worth one wiki spot-check when D4-01 lands.
- **The player Health rows ignore the authored player GMSTs.** The player rows hardcode `20/10/90` (FO3) and `20/5/95` (FNV), while the NPC curve now overlays `fAVDNPCHealth*` from the load order. A mod that retunes `fAVDHealthEnduranceMult` changes the NPC side's discipline but not the player's. This is not a defect today; it would be the symmetric follow-up to `with_gmst`.
- **`health_gmst_dump.rs` comment.** Its comment calls the FO3/FNV GMST `DATA` "1 type byte + 4-byte value". The real float `DATA` is 4 bytes (verified), so the measurement went through the `>= 4` arm and is valid. Only the comment is wrong. The example is tooling, not CHARAL.
- **No alive vanilla NPC seeds Health ≤ 0 under the new curve** (census of base > 0, non-PC-mult records: FNV 0, FO3 0). The level-0 records of #4232 contribute `−5` at worst.
- **The player-template fallback.** When the Health row cannot resolve (no ruleset, or no `Endurance` AVIF), the player keeps the NPC-curve Health. The `inventory.rs` attach test exercises exactly that, which is why it expects 20. Vanilla masters always resolve the row, so this is inert.

## Regression checks (closed since baseline)

| Issue | Verified |
|---|---|
| #5238 | `NpcHealthCurve` is 5 / 5 / −1 with the `with_gmst` overlay, and `derive_autocalc_actor_values` adds the clamped `data_base_health`. Mults verified as authored in both masters. The offset was verified at the exe default in `Fallout3.exe`; `FalloutNV.exe` carries the same setting name, but its value could not be read: a pointer scan finds no static reference to the name string, which is the known limitation for that exe. The 15/15 vanilla fit across both masters is the FNV-side evidence. Probe: Easy Pete 65 ✓. The bin fixtures were re-derived (`297a0e64a`). The level-term binding is D1-01, and the END source is D4-01. |
| #5240 | All five capture rows quote the GECK page verbatim (checked against the local dump). Melee is annotated CONFLICTED. The pin was renamed and is green. |

## Known-Open Register (re-confirmed on HEAD; none re-filed)

1. **FNV/FO3 tag-skill per-level formula**: still undocumented and absent. CLAS SPECIAL is still read from `ATTR`.
2. **FO3/FNV AP NPC scope (#2937, closed as documented)**: still `.player_only()`. The GECK page has no AP entry (#5240). The Crit/Unarmed/resist scopes are now sourced, and Melee is CONFLICTED and deliberately retained.
3. **VATS runtime**: absent; only the AP formulas exist.
4. **Regen / affliction / level-up**:
   - `PoolRegenConfig` has 4 inserts, all in `regen.rs` `mod tests`.
   - `pool_regen_tick_system` is registered-inert.
   - `affliction_tick_system` is unregistered (`registry_completeness_tests.rs`: forward-latent).
   - `level_cap()` has test consumers only.
5. **Oblivion `RulesetBuilder::None`**: deliberate and pinned. FO76/Starfield have captures, no builders and `NpcStatModel::None`.
6. **#4137** (OPEN): six `template_flags` bits have no consumer.
7. **#4232** (OPEN): `effective_actor_level` still returns `npc.level.max(0)` verbatim; there is a single definition in `actor/mod.rs`.
8. **#4415** (OPEN): the magic runtime is partial. `apply_constant_modifiers` is unchanged since baseline.
9. **#5239 / #5241 / #5242** (OPEN, baseline D1-01 / D1-02 / D5-01): the code and docs are unchanged, so all three are still present.
10. **#5079** (OPEN, `/audit-legacy-compat`): the duplicated Child race-flag translation.
11. **`CharacterLevel` unsaved** and **`FactionReputation` has no production insert**: unchanged.

## Cross-Audit Routing

- **D4-01**: the `DATA` SPECIAL parse is `/audit-esm` Dim 4 (`NPC_` decoding). Consuming it on the auto-calc-OFF branch is CHARAL. The combat and dialogue follow-through goes to `/audit-gameplay` and `/audit-scripting` (`GetActorValue`).
- **D1-01, D5-01**: CHARAL only.
- Component shape → `/audit-ecs`; the scheduler comment (#5241) → `/audit-concurrency` Dim 4.

Suggested next step: `/audit-publish docs/audits/AUDIT_CHARACTER_2026-10-05.md`. Use the domain label `character` with `game:fnv` + `game:fo3` on all three findings, and add `esm-plugin` on D4-01.
