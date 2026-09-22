**HEAD**: `f97775ca8` · **Baseline**: [`AUDIT_CHARACTER_2026-09-19.md`](AUDIT_CHARACTER_2026-09-19.md) (HEAD `479163836`) · **Audited**: Dim 1 (Ruleset Seam), Dim 2 (Derived Formulas, constants re-read against captures), Dim 4 (Population Boundary), Dim 5 (Coverage & Doctrine) · **Unchanged since baseline (skimmed)**: Dim 3 (Progression, Pools, Afflictions & Reputation)

# Character / CHARAL Audit — 2026-09-21

`/audit-character`, default scope, `--depth deep`, run as one leg of `/audit-suite --preset
comprehensive`. The five dimensions were analysed one at a time, with no sub-agents. Delta:
`479163836..f97775ca8` (142 commits, 16 in scope). The in-scope commits that matter:
- `eb3784309`: player stamp, fixing #4447, #4450 and #4458.
- `d55ee2586`: #4448.
- `767dbc5b3`: #4449.
- `044425aad`: #4451.
- `db39fe004`: native HUD vitals, a new consumer of the player's `ActorValues`.
- `d574d9bd1` and `62fc0bf22`: FO3/FNV MenuXml and FO4 Scaleform `--hud` drivers.
- `ec3a18d2f`: Draugr combat marker.
- `24ccc8f74`: loot re-equip.

## Tests recorded (read-only; nothing launched, no `--ignored`)

| Command | Result |
|---|---|
| `cargo test -j 4 -p byroredux-core --features inspect character` | 121 passed, 0 failed (120 CHARAL + 1 name-matched material test; +2 vs baseline: the #4447 and #4450 pins) |
| `cargo test -j 4 -p byroredux-core --features inspect -- combat stealth` | 32 passed (combat 6 + stealth 26, = baseline) |
| `cargo test -j 4 -p byroredux-plugin --lib -- actor_value_derive consumables::tests::fn586` | 29 passed (28 `actor_value_derive`, +1 vs baseline; #4448 fn-586 pin) |
| `cargo test -j 4 -p byroredux --bin byroredux -- resolve_inherited_call_sites obscript_dialect player_character_template attach_to_player vitals_snapshot install_catalog_resolves_per_game_vital_keys melee_damage player_npc_form_id` | 10 passed. **The bin crate builds on the default toolchain (rustc 1.96)**, which the baseline could not do. |
| Real-data probes (read-only Python `mmap` GRUP walkers, `/tmp/audit/character/`) | Covered: the base Player `NPC_` (0x7) plus its CLAS/RACE on `FalloutNV.esm`, `Fallout3.esm`, `Fallout4.esm` and `Skyrim.esm` (SE); the AVIF id table of each; a census of `Fallout4.esm` `NPC_` PRPS vs DNAM. |

## Executive Summary

**8 new findings: 0 CRITICAL · 0 HIGH · 3 MEDIUM · 5 LOW. 0 regressions**:
- All six issues closed since the baseline were re-verified fixed on HEAD: #4447, #4448, #4449, #4450, #4451 and #4458.
- 13 still-open issues were re-confirmed and cited, not re-filed.

**Formula constants: again no drift.** Every coefficient, bias, cross term, cap, RoundMode and scope in:
- `fallout.rs`, `tes.rs`, `skyrim.rs` and `resistance.rs`: re-read against the capture documents this run.
- `combat.rs` and `stealth.rs`: unchanged since baseline.

Families verified: FO3, FNV, FO4, Oblivion and Skyrim. The formula-file diffs since baseline are doc/test only. FO76 and Starfield still have captures but no code.

**The defects are in how the new player actor is populated, not in the formulas.** `eb3784309` (#4458) made the player a stat-bearing CHARAL actor for the first time. It did so by running the base Player `NPC_` record through the *NPC* population path.

The real-master probe shows what that yields:
- **Skyrim**: correct pools (NordRace 50 + offsets 50 = `SKYRIM_POOL_BASE` 100).
- **FO3/FNV**: correct Health, but only by coincidence. At L1/END5 the NPC curve equals the player formula (200).
- **FO4**: the NPC DNAM bake, Health **150** and AP **100**. The capture's player formulas give **85** and **70** at the record's SPECIAL-1/L1.
- **FO3/FNV AP**: nothing at all. AP is a `PlayerOnly` derived row, the auto-calc path never seeds it, and no consumer evaluates `PlayerOnly` rows for any entity (**CHAR-2026-09-21-D4-01**).

The new HUD consumers compound this:
- The native vitals bar reads carried values only, so the FO3/FNV AP bar never draws.
- The vanilla `--hud` bars key on `0x2C9`/`0x2D0`. These are `fallout.rs`'s synthetic unit-test FormIDs; real FNV Health is `0x450`, and FO4 `0x2C9` is `Experience`. So those bars never move on FO3/FNV/FO4 (**D1-01**).
- The capture documents still say no player-actor entity exists, which hides the gap from the next contributor (**D5-01**).

**Wiring state.** Unchanged for rulesets, regen and affliction:
- `PoolRegenConfig` has zero production inserts.
- `affliction_tick_system` is unregistered.
- `level_cap()` has no production consumer.

The only wiring event is the new player seed column in the coverage matrix.

## Constant Verification Table

Doc keys: **FO4** = `charal-fo4-ruleset.md`, **FNV/FO3** = `charal-fnv-fo3-ruleset.md`, **OBL** =
`charal-oblivion-ruleset.md`, **SKY** = `charal-skyrim-ruleset.md`, **CORE** = `charal.md`.

### Derived-stat formulas (Dim 2 — re-read this run; no code change since baseline)

| Formula / constant | Code | Document | Verdict |
|---|---|---|---|
| FO4 Health `floor(77.5+4.5·END+2.5·L+0.5·L·END)` player-only | `fallout.rs:137-143` | FO4:72,87 | PASS (the only cross-term row) |
| FO4 AP `60+10·AGI` player-only | `fallout.rs:146-151` | FO4:132 | PASS |
| FO4 Carry Weight `200+10·STR` | `fallout.rs:153-155` | FNV/FO3:106-110 | PASS |
| FO3 Health `90+20·END+10·L` / AP `65+2·AGI` cap 85 (player-only) | `fallout.rs:166-192` | FNV/FO3:93-94 | PASS ×2 |
| FNV Health `95+20·END+5·L` / AP `65+3·AGI` cap 95 (player-only) | `fallout.rs:205-220` | FNV/FO3:93-94 | PASS ×2 |
| FO3/FNV CW 150+10·STR · Melee 0.5·STR · Crit 1.0·Luck cap 10 · Unarmed ceil(0.5+0.05·U) | `fallout.rs:55-79` | FNV/FO3:95-98 | PASS ×4 |
| Crit/Melee/Unarmed scope `ActorGeneral` (explicit, unsourced, pinned) | `fallout.rs:44-51` + test | FNV/FO3:96-98 annotation | PASS (#4450 fixed; module doc residue → D2-01) |
| RadResist (END−1)·2 cap 85 · PoisonResist (END−1)·5 uncapped · `damage_multiplier` | `resistance.rs:75-89,125-128` | FNV/FO3:99-100,355-374 | PASS ×3 |
| Oblivion Health 2·END · Magicka 2·INT · Fatigue 4×1.0 · Armor 0.35+0.0065·skill | `tes.rs:40,47,66-68,85` | CORE:317-320; OBL damage formula | PASS ×4 |
| Skyrim Light Armor 1+0.004·skill (player-only mult) · CW 250+0.5·base Stamina | `skyrim.rs:79,86-88` | SKY:204-221,651-659 | PASS ×2 |
| `combat.rs` / `stealth.rs` coefficients | unchanged since baseline | OBL damage formula; FNV/FO3:231-310 | PASS (32 tests green) |

### Progression / pools / reputation (Dim 3 — byte-identical to baseline)
The baseline's 60 rows (33 leveling + 27 pools/afflictions/resistance/reputation) stand. `leveling.rs`,
`regen.rs`, `affliction.rs`, `reputation.rs`, `components.rs` and `skyrim.rs` have an empty diff since
`479163836`.

### Player seed, new this run: real Player `NPC_` records vs captures

| Row | Stamped value (real master) | Document value | Verdict |
|---|---|---|---|
| FNV player Health (SPECIAL 5×7, L1) | 200 (NPC curve 95/20/5) | FNV/FO3:93 `100+20·5+5·0` = 200 | PASS (coincidence of curves) |
| FO3 player Health (SPECIAL 5×7, L1) | 200 (NPC curve 90/20/10) | FNV/FO3:93 `90+100+10` = 200 | PASS (coincidence of curves) |
| FO3/FNV player Action Points | **absent** (not seeded; `PlayerOnly` row never evaluated) | FNV/FO3:94 → FNV 80, FO3 75 | **FAIL → D4-01** |
| FO4 player Health (END 1, L1) | **150** (NPC DNAM bake) | FO4:72,87 → 85 | **FAIL → D4-01** |
| FO4 player Action Points (AGI 1) | **100** (NPC DNAM bake) | FO4:132 → 70 | **FAIL → D4-01** |
| Skyrim player Health/Magicka/Stamina | 100/100/100 (NordRace 50 + ACBS +50 each) | SKY:583-586 (`SKYRIM_POOL_BASE` 100) | PASS |
| FO3/FNV body-condition AV base 100 (`body_condition_base`) | 100 | none in FNV/FO3; cited only at `playable-vertical-slice.md:1168-1174` (GECK *Stats List*) | **UNSOURCED-IN-CAPTURE → D1-03** |

## Coverage Matrix

Re-derived from `profile.rs` arms, workspace greps and the real-master probe (Dim 5):

| Family | Ruleset implemented | Ruleset wired | Derived stats | Leveling model | Player seed (#4458, new) | Regen wired | Affliction wired |
|---|---|---|---|---|---|---|---|
| FO3 | ✓ `fallout3_ruleset` | ✓ `RulesetBuilder::Fallout3` | ✓ 8 rows | ✓ `150·L+50`, cap 20 (model only) | ✓ NPC auto-calc path; Health 200 ✓; **no AP** | ✗ registered-inert | ✗ unregistered |
| FNV | ✓ `falloutnv_ruleset` | ✓ `RulesetBuilder::FalloutNewVegas` | ✓ 8 rows | ✓ `150·L+50`, cap 30 | ✓ as FO3; **no AP** | ✗ | ✗ |
| FO4 | ✓ `fallout4_ruleset` | ✓ `RulesetBuilder::Fallout4` | ✓ 3 rows | ✓ `75·L+125`, uncapped | ~ NPC stored path; Health 150 / AP 100 ✗ (capture 85/70) | ✗ | ✗ |
| Skyrim SE | ✓ `skyrim_ruleset` | ✓ `RulesetBuilder::Skyrim` | ✓ 2 rows | ✓ `25·L+75` + skill-XP curve | ✓ race+offset 100/100/100 ✓ | ✗ | ✗ |
| Oblivion | ✓ `oblivion_ruleset` | ✗ `RulesetBuilder::None` (deliberate, pinned) | ✓ tests only | ✓ 10 major-skill-ups | ✗ `NpcStatModel::None` | ✗ | ✗ |
| FO76 | ✗ | ✗ | ✗ | ✗ curve LOCKED, uncoded | ~ unsourced `Stored` (#4453) | ✗ | ✗ |
| Starfield | ✗ | ✗ | ✗ | ✗ PENDING | ~ unsourced `Stored` (#4453) | ✗ | ✗ |

Meaning of the ✗ in the regen and affliction columns:
- **"Registered-inert"**: `pool_regen_tick_system` is registered at `boot/schedule/update.rs:380`, but all four `PoolRegenConfig` inserts are inside `regen.rs` `mod tests`.
- `affliction_tick_system` has no registration anywhere.
- The "Leveling model" column is a model, not a runtime: `level_cap()`'s only consumers are tests.

## Findings

**MEDIUM (3)**

### CHAR-2026-09-21-D4-01: The player is populated through the NPC path, and no consumer evaluates the `PlayerOnly` ruleset rows for the one entity they exist for — FO4 player gets Health 150/AP 100 (capture: 85/70), FO3/FNV player has no Action Points
- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: FO4 (wrong values); FO3/FNV (missing AP); all (structural)
- **Source**: charal-fo4-ruleset.md:72,87 (player HP `floor(77.5 + 4.5·END + 2.5·L + 0.5·L·END)`), :77-78 ("health rescales **dynamically** with any Endurance / level change"), :96-100 (player-only), :132 (`AP = 60 + 10·Agility`), :536-541 ("the END/AGI curves produce the *player's* live HP/AP, while NPCs ship a precomputed `Calculated Health`"); charal-fnv-fo3-ruleset.md:94 (FO3 `65 + 2·AGI` cap 85, FNV `65 + 3·AGI` cap 95)
- **Location**: `byroredux/src/inventory.rs:227-245` (`build_player_character_template` → `derive_npc_actor_values`); `byroredux/src/inventory.rs:525-537` (stamp in `attach_to_player`); `crates/plugin/src/esm/records/actor_value_derive.rs:376-398` (FO4 stored path); `crates/scripting/src/condition.rs:500-533` (`GetActorValue` skips `PlayerOnly` for every entity); `byroredux/src/inventory.rs:197-223` (`vitals_snapshot` reads carried entries only)
- **Status**: NEW (introduced by `eb3784309`, the #4458 fix; #4458 itself is correctly fixed — the stamp is live)
- **Description**: `build_player_character_template` derives the player's `ActorValues` with `derive_npc_actor_values(player_npc, index)`, the NPC population function. For the player, several ruleset rows are `PlayerOnly` derived formulas: FO3/FNV/FO4 Health and AP, Skyrim Light Armor, and Oblivion's pools. The capture documents state the player's live values come from those formulas and NPCs ship baked values. The code instead gives the player the NPC answer, and nothing ever evaluates the `PlayerOnly` rows for the player:
  - `GetActorValue` excludes `PlayerOnly` for every entity. Its comment still says "the player isn't modelled yet".
  - `vitals_snapshot` reads carried values only.
  - No other consumer asks.

  Real-master results (read-only probe of Player `NPC_` 0x00000007):
  - **FO4** (`Fallout4.esm`: PRPS SPECIAL 1×7, ACBS level 1): PRPS also authors `Health=40.0` and `ActionPoints=0.0`, and DNAM authors calc_health **150** and calc_ap **100**. The DNAM pairs are pushed after PRPS, and `from_pairs` is last-write-wins, so the player carries Health **150** and AP **100**. The capture's player formulas give **85** and **70** (1.76× / 1.43× off).
  - **FO3/FNV** (auto-calc off, `PlayerClass` ATTR 5×7, level 1): Health 200 matches the capture only because the NPC curve equals the player formula at L1/END5. ActionPoints is never seeded.
- **Evidence**: The two FO4 values come straight from `actor_value_derive.rs:380-398`:
  ```rust
  out.extend_from_slice(props);                    // PRPS: (Health, 40.0), (ActionPoints, 0.0), …
  for (avif_editor_id, baked) in [("Health", …calculated_health), ("ActionPoints", …)] {
      if baked > 0 { … out.push((fid, f32::from(baked))); }   // (Health, 150.0), (ActionPoints, 100.0)
  }
  ```
  The FO3/FNV AP result comes from `condition.rs:500-533`. The carried fast path misses, the AP row is `PlayerOnly`, and the function returns `0.0`.
- **Impact**:
  - **FO4**: the carried values win in `GetActorValue`, the native HUD, `combat_damage_system` (the player's Health pool now takes NPC `StartCombat` strikes) and drowning. A temporary END change cannot rescale HP, which the capture requires.
  - **FO3/FNV**: `player.GetActorValue ActionPoints` reads 0.0 (should be 80 FNV / 75 FO3). The native HUD's AP bar (db39fe004) never draws. `install_catalog_resolves_per_game_vital_keys` asserts only that the AP *key* resolves, and no test composes a production-stamped player.
  - Latent: `consume_item` rejects a whole item when any effect's AV is not carried (`inventory.rs:1075-1086`), and the plugin maps FO3/FNV MGEF AV 12 → `ActionPoints` (`consumables.rs:152`). Any AP-restoring ingestible would therefore be unusable by the player. No vanilla item in today's supported restorative set targets AP.
- **Related**: #4458 (the fix this follows from), #2937 (FO3/FNV AP scope), #4452 (the other scope-contract consumer), CHAR-2026-09-21-D5-01 (the docs that hide this), CHAR-2026-09-21-D4-03 (the rest of the half-populated player), CHAR-2026-09-21-D4-02 (same PRPS/DNAM collision, NPC-wide)
- **Suggested Fix**: Seed only SPECIAL/skills (and FO3/FNV body conditions) from the Player record. Then give the player its `PlayerOnly` rows: evaluate them for `PlayerEntity` when stamping, or let `GetActorValue`/`vitals_snapshot` fall through to `derived_value` for the player. Drop the NPC-baked Health/AP from the player seed. Add a real-master leg asserting FO4 Health 85 / AP 70 and FNV AP 80.

### CHAR-2026-09-21-D1-01: The vanilla `--hud` bars key on `fallout.rs`'s synthetic unit-test FormIDs (`0x2C9`/`0x2D0`) and read whichever actor comes first in storage — FO3/FNV/FO4 bars never move
- **Severity**: MEDIUM
- **Dimension**: Ruleset Seam
- **Game**: FO3 / FNV (MenuXml `--hud`), FO4 (Scaleform `--hud`), Skyrim (actor-identity half only)
- **Source**: real-master AVIF tables (read-only probe): FNV/FO3 `AVHealth 0x450`, `AVActionPoints 0x44C`, and neither `0x2C9` nor `0x2D0` is an AVIF; FO4 `Health 0x2D4`, `ActionPoints 0x2D5`, **`0x2C9 = Experience`**, `0x2D0` = none; Skyrim `0x3E8/0x3E9/0x3EA` = AVHealth/AVMagicka/AVStamina ✓. `0x2C9`/`0x2D0` are the ids of the test-only `full()` resolver in `crates/core/src/character/fallout.rs` (`:233-241`, beside `Strength => 0x05`).
- **Location**: `byroredux/src/hud.rs:110-118` (`FALLOUT_BARS` `av: Some(0x2C9)` / `Some(0x2D0)`) and its doc `:38-45`; `byroredux/src/scaleform_hud.rs:83-88` (FO4 `ScaleformBar` `0x2C9`/`0x2D0`, commented "FO4 keys per `crates/core/src/character/fallout.rs`"); `byroredux/src/hud.rs:690-715` (`fraction`)
- **Status**: NEW (`d574d9bd1`, `62fc0bf22`, 2026-09-19)
- **Description**: AVIF identities are AUTHORED and must be resolved per load (CHARAL doctrine; `EsmIndex::actor_value_form_id`). The FO3/FNV MenuXml and FO4 Scaleform HUD profiles instead hardcode ids copied from a unit-test fixture. Separately, `fraction()` has two problems:
  - It scans `world.query::<ActorValues>()` and returns the first entity in `SparseSetStorage`'s dense order (insertion order, perturbed by swap-remove) that carries the key. It never reads `PlayerEntity`.
  - That "any stamped actor" fallback predates eb3784309 and is obsolete now that the player carries `ActorValues`.

  The native vitals path one module over (`inventory.rs:179 build_player_vitals` + `vitals_snapshot`) already does both things correctly: it resolves keys through `actor_value_form_id` and reads the player.
- **Evidence**: `hud.rs:110-118`:
  ```rust
  static FALLOUT_BARS: &[HudBar] = &[
      HudBar { label: "hp", av: Some(0x2C9) },
      HudBar { label: "ap", av: Some(0x2D0) },
  ];
  ```
  `fraction` falls back to `1.0` when no entity carries the key (`hud.rs:713`).
- **Impact**:
  - On real FO3/FNV/FO4 content no entity carries these keys, so the HP/AP bars always draw full, regardless of damage. On FO4 the "health" bar is keyed on the `Experience` AV.
  - On Skyrim the keys are right, but the bar can track an NPC's health rather than the player's.
  - The smoke gates pin bar fractions via `hud.values`, so none of them catches it.
- **Related**: CHAR-2026-09-21-D1-02, CHAR-2026-09-21-D4-01; PERF-D1-2026-09-21-03 and #3429 (HUD cost, not keys)
- **Suggested Fix**: Resolve each bar's key once per load via `index.actor_value_form_id(editor_id)`, reusing `build_player_vitals`'s resolution. Read `PlayerEntity`'s `ActorValues` rather than scanning all actors. Delete the literal ids and the doc claim at `hud.rs:43-45`. The fix belongs to `/audit-ui` (`hud.rs`/`scaleform_hud.rs` owners).

### CHAR-2026-09-21-D5-01: Wiring-event doc rot — four places still say no stat-bearing player actor exists, after #4458 made one (and hide D4-01)
- **Severity**: MEDIUM
- **Dimension**: Coverage & Doctrine
- **Game**: all (FO4 + FO3/FNV most directly)
- **Source**: not numeric. What these docs should say comes from `byroredux/src/inventory.rs:501-537`: `attach_to_player` stamps `ActorValues` + `ActorVitals` from the base Player `NPC_`, which the real-master probe confirmed on FO3/FNV/FO4/Skyrim.
- **Location**: `docs/engine/charal.md:407-410` (§7 "No player chargen yet. There is still no stat-bearing player-actor entity (`scene.rs`'s `player_entity` is an `AnimationPlayer`)"); `docs/engine/charal-fo4-ruleset.md:101-104` (Health caveat 2 "No player-actor entity yet … application deferred") and `:166-168` (AP "Application caveat"); `crates/scripting/src/condition.rs:503-504` ("NPCs bake them, the player isn't modelled yet"); `docs/feature-matrix.md:260-266` (CHARAL table has an NPC-population row, no player row)
- **Status**: NEW
- **Description**: `eb3784309` wired a stat-bearing player, but none of the CHARAL prose that states the opposite was updated. The two FO4 capture caveats are exactly what a contributor fixing D4-01 would read. They say the player formulas have nowhere to apply and application is deferred, while the code has already applied the NPC path instead. `mod_docstring_indexes_every_sub_module` checks module names only, so nothing catches this.
- **Evidence**: see Location. The player seed has existed since 2026-09-19 (`eb3784309`); all four sites predate it and were not touched by it.
- **Impact**:
  - The capture documents, the authority for this layer, misstate the player's state in a way that steers the next change away from the real defect.
  - The feature matrix gives no signal that a player seed exists, or which games get it.
- **Related**: CHAR-2026-09-21-D4-01, #4458; sibling doc-rot clusters #4459, #4460 (still open)
- **Suggested Fix**: Rewrite the three doc sites to say the player carries a seed from the base Player `NPC_` via `derive_npc_actor_values` (#4458), and that the player-only formulas are not yet evaluated for it (D4-01). Fix the condition.rs comment. Add a "Player actor-value seed" row to feature-matrix's CHARAL table (FO3/FNV/FO4/Skyrim ✓, Oblivion ✗, FO76/Starfield ~).

**LOW (5)**

### CHAR-2026-09-21-D4-02: FO4's stored path emits duplicate Health/AP keys on ~2,500 vanilla NPCs; the capture-correct DNAM-wins outcome holds only by push order
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: FO4 (and FO76/Starfield via the same `Stored` model)
- **Source**: charal-fo4-ruleset.md:536-541 (NPC Health/AP "read **straight from `DNAM`**"). Census of `Fallout4.esm` (read-only, `/tmp/audit/character/census_fo4_prps*.py`) covers 3,015 `NPC_` records:
  - PRPS authors Health on 2,848. Of those, 2,525 also carry DNAM calc_health > 0, and **2,490 disagree**.
  - PRPS authors ActionPoints on 2,800. Of those, 2,415 carry DNAM AP > 0, and 799 disagree.
  - Colliding PRPS Health values: 100.0 ×611, 0.0 ×350, **−10.0 ×185**, 50.0 ×170, 40.0 ×131.
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:376-398`; `crates/core/src/ecs/components/actor_values.rs:103-109`
- **Status**: NEW
- **Description**: `derive_stored_actor_values` copies PRPS verbatim and then pushes the baked DNAM Health/AP. The same AVIF keys therefore appear twice, and `ActorValues::from_pairs` (`set_base` in order) keeps the last one. The result matches the capture only because the DNAM pushes come second:
  - Nothing states this precedence. The function doc says "PRPS verbatim plus baked DNAM", and the capture never mentions PRPS authoring these keys.
  - No test covers the collision: `fo4_stored_returns_prps_verbatim_plus_baked_derived`'s PRPS has no Health/AP pair.
- **Evidence**: `out.extend_from_slice(props);` (`:380`) precedes the `for (avif_editor_id, baked) in [("Health", …), ("ActionPoints", …)]` push loop (`:381-398`).
- **Impact**:
  - Correct today.
  - A reorder, sort-by-key or dedup-first refactor would silently give ~1,000 vanilla FO4 actors 0 or −10 base Health, which means dead on spawn or undamageable.
- **Related**: CHAR-2026-09-21-D4-01 (the player sees the same collision, where even the DNAM answer is wrong); #3481 / #4086 (earlier FO4 stored-path precedence fixes)
- **Suggested Fix**:
  - Drop PRPS pairs keyed on the Health/AP AVIFs when a baked value is present, so the precedence is explicit.
  - Add a fixture with PRPS Health −10 and DNAM 150, asserting 150.
  - Record the census in the FO4 capture's NPC-storage section.

### CHAR-2026-09-21-D4-03: The player is half-populated — `CharacterLevel`/`Background` withheld on a justification the #4458 stamp itself falsified, deferred to two closed unrelated issues; consumers disagree on the absent player level
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: FO3 / FNV / FO4 / Skyrim
- **Source**: charal.md §4.4 (`Background` "carries what population consumed (race / class)"). Real Player records: FNV/FO3 `PlayerClass` 0x57E6A + race 0x19; Skyrim `AAAPlayerSpellswordClass` + NordRace; FO4 `ZeroSPECIALclass` + HumanRace; ACBS level 1 on all four.
- **Location**: `byroredux/src/scene.rs:1123-1133`; level defaults in `crates/scripting/src/condition.rs:499,628-633,679` (0), `byroredux/src/combat.rs:454-456` (1), `byroredux/src/cell_loader/references/attach.rs:219-222` (1)
- **Status**: NEW
- **Description**: The note updated by eb3784309 keeps `CharacterLevel` and `Background` off the player on two grounds, and neither holds:
  - It says "`Background` has no honest value until the player has a real race/class". Yet the same commit derives the player's `ActorValues` *from* the Player record's class, race and level 1. That is exactly the provenance `Background` records.
  - It defers population to "#3004 / #2986". Both issues are CLOSED and concern NPC AVIF prefixes / NPC Health derivation. No open issue tracks player level/background.

  Meanwhile the absent level reads differently depending on the consumer:
  - 0 in `GetLevel`, in `GetActorValue`'s derived rows and in `GetXPForNextLevel`. For FNV that last one gives `150·0+50` = 50 XP, versus 200 at L1.
  - 1 in `melee_damage_charal_bonus` and in container leveled loot.

  All of this is for an actor whose Health was derived at level 1.
- **Evidence**: `scene.rs:1129-1133`: "`CharacterLevel` and `Background` remain deliberately absent … `Background` has no honest value until the player has a real race/class. Populating those is CHARAL work (#3004 / #2986)". Also `gh`: #3004 CLOSED ("auto-calc derivation has no Health term"), #2986 CLOSED ("AVIF EditorIDs are AV-prefixed").
- **Impact**:
  - Player-level CTDA gates read 0.
  - XP-to-next is computed for level 0.
  - The deferral has no live tracker, so it cannot be scheduled.
- **Related**: CHAR-2026-09-21-D4-01, #4458, #3158; `/audit-gameplay` noted the level-1 loot bootstrap as documented policy (unfiled)
- **Suggested Fix**: Stamp `CharacterLevel { level: effective_actor_level(player) }` and `Background` from the resolved Player record beside `ActorValues`. Otherwise, re-point the deferral at an open issue with a reason that still holds.

### CHAR-2026-09-21-D1-02: The native HUD's per-game pool roster is a `GameKind` match in a CHARAL consumer
- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: all
- **Source**: charal.md §1 table ("Derived pools" per family is per-game ruleset data); CHARAL doctrine "the per-game seam is data in the tables, never a branch in a consumer" (`crates/core/src/character/ruleset.rs:5-7`)
- **Location**: `byroredux/src/inventory.rs:155-177` (`vital_bar_candidates(game: GameKind)`), consumed at `:179-189`
- **Status**: NEW (`db39fe004`)
- **Description**: Which pools a game's character has is per-game ruleset data: Health/Magicka/Stamina, Health/Magicka/Fatigue, or HP/AP. Here it is expressed as a consumer-side `GameKind` match, because `CharacterRulesProfile`/`CharacterRuleset` expose no pool roster. This is the shape #4447 moved off a consumer for body conditions. No FO3-vs-FNV discrimination is needed, so behaviour is correct.

  The Oblivion arm is unreachable in production. `actor_value_form_id` resolves only AVIF records, and `Oblivion.esm` authors none (charal.md:349-350). The test feeds synthetic AVIFs.
- **Evidence**: `fn vital_bar_candidates(game: GameKind) -> &'static [(&'static str, &'static str)] { match game { GameKind::Skyrim => …, GameKind::Oblivion => …, GameKind::Fallout3NV | GameKind::Fallout4 | GameKind::Fallout76 => …, GameKind::Starfield => … } }`
- **Impact**: A future FO3/FNV divergence or new family requires a consumer edit invisible to the profile table; the Oblivion arm gives false coverage confidence.
- **Related**: #4447 (precedent), CHAR-2026-09-21-D1-01
- **Suggested Fix**: Put the pool roster on `CharacterRulesProfile`, as editor-id/label pairs, and have `build_player_vitals` read it. Drop the Oblivion arm, or mark it blocked on #3768's pre-AVIF resolver.

### CHAR-2026-09-21-D1-03: `body_condition_base = 100` has no row in the CHARAL capture — its only citation sits in the vertical-slice log
- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: FO3 / FNV
- **Source**: `docs/engine/playable-vertical-slice.md:1168-1174` (GECK [Stats List](https://geckwiki.com/index.php/Stats_List)); `charal-fnv-fo3-ruleset.md` has no body-condition row
- **Location**: `crates/core/src/character/profile.rs:93-100,134,150`; `docs/engine/charal-fnv-fo3-ruleset.md:89-101`
- **Status**: NEW
- **Description**: The #4447 fix correctly moved the base-100 constant onto the profile. It left the capture document silent, even though that document is the stated authority for every CHARAL constant. A profile-row audit can verify every other FO3/FNV row against `charal-fnv-fo3-ruleset.md`, but not this one.
- **Evidence**: The profile doc cites "GECK Stats List"; the capture has no occurrence of "body condition" or of the seven AV names.
- **Impact**: A future edit to the constant has no capture line to be checked against, which is the exact gap the no-guessing doctrine guards.
- **Related**: #4447, #4453 (the unsourced-profile-row class)
- **Suggested Fix**: Add a "Body-condition AVs (7) — base 100, GECK *Stats List*" row to the FNV/FO3 capture, naming `consumables::BODY_CONDITION_VALUES`.

### CHAR-2026-09-21-D2-01: `fallout.rs`'s module docstring still states Crit/Melee/Unarmed are actor-general as fact — the #4450 fix edited only the function doc
- **Severity**: LOW
- **Dimension**: Derived Formulas
- **Game**: FO3 / FNV
- **Source**: charal-fnv-fo3-ruleset.md:96-98 (scope annotated "unsourced", #4450)
- **Location**: `crates/core/src/character/fallout.rs:11-21`
- **Status**: NEW (residue of #4450, which cited only `:44`)
- **Description**: The module doc says "Carry Weight / Melee Damage / Critical Chance / Unarmed Damage are actor-general. That justification is sourced for Health … but **not** for FO3/FNV Action Points". It presents the three unsourced scopes as settled, and lists AP as the only unsourced exception. This is the overstatement #4450 was filed for. The function doc at `:44-51` and the capture were fixed; the module summary a reader meets first was not.
- **Evidence**: `fallout.rs:14-15` vs `:44-51`.
- **Impact**: Doc only; misleads a reader about which scopes are sourced.
- **Related**: #4450 (CLOSED), #2937
- **Suggested Fix**: Add to the module doc: "Critical Chance / Melee Damage / Unarmed Damage are an explicit, unsourced `ActorGeneral` choice (#4450, pinned by `fo3_fnv_crit_melee_unarmed_scopes_are_actor_general_pending_a_source`)".

## Regression checks (closed since baseline — all fixed on HEAD)

| Issue | Verified |
|---|---|
| #4447 | `CharacterRulesProfile::body_condition_base` (`profile.rs:134,150`); the consumer gates on it (`actor_value_derive.rs:233-251`); no `GameKind` left; pinned twice (core + plugin), both green |
| #4448 | Doc on the profile; `obscript_dialect_follows_the_profile_not_the_game_kind` (bin) and `fn586_condition_gate_is_profile_scoped_not_game_scoped` (plugin), both green; no third borrower |
| #4449 | `tes.rs` now "wearer's own ArmorSkill"; no "OpponentArmorSkill" anywhere in code or docs |
| #4450 | Capture rows annotated scope-unsourced and flipped to BUILT; pin green. Module-doc residue → D2-01 |
| #4451 | `RoundMode` doc scoped to sourced rows |
| #4458 | Production path live on interior (`cell_loader/load.rs:634,1017`) and exterior (`scene/world_setup.rs:1011`) loads before `spawn_player_body` → `attach_to_player`; real Player records yield populated seeds on FO3/FNV/FO4/Skyrim. Correct as a stamp; *what* it stamps → D4-01 |

## Known-Open Register (re-confirmed on HEAD; none re-filed)

1. **FNV/FO3 tag-skill per-level formula**: still undocumented and absent, not guessed (`actor_value_derive.rs` "Deferred" doc). CLAS SPECIAL is still read from `ATTR`.
2. **FO3↔FNV divergent player Health/AP (#2937 lineage)**: all affected rows still ship `.player_only()` and are disclosed. D4-01 adds that no consumer evaluates those rows for the player at all.
3. **VATS runtime**: still absent; only the AP formulas exist.
4. **Regen / affliction / level-up**:
   - `PoolRegenConfig` has zero production inserts. All four are in `regen.rs` `mod tests`; `pool_regen_tick_system` is registered-inert at `boot/schedule/update.rs:380`.
   - `affliction_tick_system` is unregistered.
   - `level_cap()` has test consumers only.
5. **Oblivion `RulesetBuilder::None`**: deliberate and pinned. FO76/Starfield have captures but no builders.
6. **#4452** (OPEN): `melee_damage_charal_bonus` still skips `DerivedScope`. Since eb3784309 it also runs for the player, and is still coincidentally correct (MeleeDamage is ActorGeneral).
7. **#4453** (OPEN): FO76/Starfield `Stored` rows are still unsourced. The blast radius grew: `attach_to_player` now runs their Player record through it.
8. **#4454** (OPEN): Skyrim NPC pools still carry 2 of 3 terms, undisclosed in place. The *player's* pools are correct at 100.
9. **#4455** (OPEN): `charal.md:348` still cites `profile.rs:82-87`; `OBLIVION` now sits near `:114-121`.
10. **#4456** (OPEN): `AfflictionTable` tie-break; code unchanged.
11. **#4457** (OPEN): TPLT hoist not done.
    - The guard still counts 11 sites; the uncounted `resolve_inherited_inventory` ×2 (`npc_spawn.rs:1184`, `inventory.rs:411`) and `resolve_inherited_field` ×2 are unchanged.
    - No new site: the player path reuses `derive_npc_actor_values`, and ec3a18d2f/24ccc8f74 add no raw `NpcRecord` reads.
12. **#4459 / #4460 / #4461 / #4462 / #4463** (OPEN doc rot): all still present.
    - #4459: `character/mod.rs:64`.
    - #4460: `boot/world.rs:42`, `charal.md:265`, `charal-oblivion-ruleset.md:416`, `regen.rs:153-157`.
    - #4461: ROADMAP `:1230`.
    - #4462: partially addressed by eb3784309. 3 of 7 rows flipped; AP, Carry Weight, RadResist and PoisonResist are still LOCKED.
    - #4463: feature-matrix `:265`, `:341`.
13. **#4137** (OPEN): six `template_flags` bits still have no consumer. **#4232** (OPEN): `effective_actor_level` still returns 0 verbatim. The single-definition guard holds (`actor/mod.rs:96`, `.max(0)`); `attach.rs:222`'s `.max(1)` is the player-level policy, not a copy.
14. **Player on the #2957 deferral**: this is a newly visible consequence, not a new finding. The real FO3/FNV Player record has auto-calc **off**, so the player joins the documented "non-auto-calc NPCs get class-averaged stats" deferral. Its stored SPECIAL (5×7) and base Health (100) coincide with the derivation; its stored skill block stays unparsed.
15. **`FactionReputation`**: no production insert. The SDK writer rejects explicitly, and the save allowlist marks the component forward-latent. Checked, not filed.

## Cross-Audit Routing

- D1-01's fix (`hud.rs`, `scaleform_hud.rs`) → `/audit-ui`.
- D4-01 and D4-03 touch `inventory.rs` / `scene.rs` / `condition.rs`; the fix sites span `/audit-gameplay` (player seed, consumables, HUD vitals) and `/audit-scripting` (`GetActorValue` / `GetLevel`).
- D4-02 (PRPS/DNAM decode precedence) → `/audit-esm` Dim 4 for the census record.
- Scheduler/lock order of the combat path: ECS-2026-09-21-D5-02 (#4574) and CONC-D3-2026-09-21-02, cited, not re-reported.
- Component shape → `/audit-ecs`. Save schema of the new player `ActorValues` column → `/audit-save`.
