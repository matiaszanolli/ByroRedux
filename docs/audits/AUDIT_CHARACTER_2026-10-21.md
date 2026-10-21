**HEAD**: 4a3e89d56 · **Baseline**: [`AUDIT_CHARACTER_2026-10-16.md`](AUDIT_CHARACTER_2026-10-16.md) (HEAD `fa7591ca2`) for Dims 1, 2, 3 and 5; [`AUDIT_CHARACTER_2026-10-19.md`](AUDIT_CHARACTER_2026-10-19.md) (HEAD `cb5368348`, area-scoped) for Dim 4 · **Audited**: Dim 1 (Ruleset Seam), Dim 3 (Progression & Pools), Dim 4 (Population Boundary), Dim 5 (Coverage & Doctrine) · **Unchanged since baseline (skimmed)**: Dim 2 (Derived Formulas). Its sources `derived/fallout/tes/skyrim/resistance.rs`, `crates/core/src/combat.rs` and `stealth.rs` have no diff since `fa7591ca2`.

# Character / CHARAL Audit, 2026-10-21

`/audit-character` ran at the default scope (`--depth deep`) as part of the comprehensive suite. Per the suite rules it ran solo (no sub-agents), read-only, with no engine launch. Builds used the repo `target/`. Read-only Python probes (GMST and record census, exe default-table reads) live in `/tmp/audit/character/scripts/`. No probe crate was created.

## Tests recorded

| Command | Result |
|---|---|
| `cargo test -p byroredux-core --features inspect --lib character` | 140 passed, 0 failed |
| `cargo test -p byroredux --bin byroredux -- resolve_inherited_call_sites grant_experience level_up experience encumbrance barter bleedout player_derived scripted_faction morality` | 19 passed, 0 failed. `resolve_inherited_call_sites_are_enumerated_and_pinned` is green: there are still exactly two production `resolve_inherited_*` calls (`cell_loader/references/mod.rs`, `player_body.rs`) |

## Delta since the baselines

- **CHARAL core** (`fa7591ca2..HEAD`): `3162b21b8` #5921 (five consumer `GameKind` tables become `CharacterRulesProfile` rows; ACBS Essential/Protected decoded in `parse_npc`), `612bd179e` #5916 (Skyrim level banks a perk plus a pool pick), `8791be5fe` #5915 (Fallout XP curve and cap read GMSTs), `118edb1e6` #5918 (bounded roll), `dc259eef9` #5925 (one reward query), `components.rs` (`LevelUpPoints::pool_picks`, `FactionReputation::set_fame/set_infamy`).
- **Population boundary** (`cb5368348..HEAD`): `415c35cff` #6080 (runtime-FaceGen gender from the Use-Traits terminal), `1dfac76f6` #6113 (pre-baked FaceGen keyed to the terminal), `78facba89` #6120 (Starfield 16-byte ACBS), `011ad0c07` #6075, `611769b16` #6071-#6074 (eviction carries script state). The rest are rendering and asset fixes in the same paths.
- **Docs**: `8ba4fa6bb` #5929 (FNV/FO3 and Skyrim captures gain level-up, kill-XP and NPC-derivation sections), `2c16ef06d` #5928 (barter caps), `bae1aa0aa` #6082, `801083125` #5930.

## Executive Summary

**5 findings: 0 CRITICAL · 0 HIGH · 2 MEDIUM · 3 LOW.** None is a regression. All 18 findings of 2026-10-16 (#5914-#5931) and the two published 2026-10-19 findings (#6080, #6082) are closed. The fixes checked against code are in place (Regression Checks below).

**Constants verified against sources this run**:

- **Fallout XP curve and cap.** These GMSTs were read straight from the masters: FO4 `iXPBumpBase` 75 / `iXPBase` 200, which gives `75L+125`, with no cap authored. FalloutNV.esm `iMaxCharacterLevel` 30 / `iXPBumpBase` 150. Fallout3.esm `iXPBumpBase` 150. BrokenSteel.esm `iMaxCharacterLevel` 30. Skyrim.esm `fXPLevelUpBase` 75 / `fXPLevelUpMult` 25.
- **The #5921 profile rows.** They were checked against the executables' setting tables, read by value-before-name-pointer:
  - Fallout4.exe: `fBarterMax` 2.5, `fBarterMin` 1.5, `fBarterBuyMax` 1.05, `fBarterSellMax` 0.95, `fEssentialDeathTime` 30, `fEssentialHealthPercentReGain` 0.3.
  - SkyrimSE.exe: 2.4 / 1.4 / `fBarterBuyMin` 1.05 / 0.95 / 30 / 0.3.
  - Fallout3.exe: 10 / 0.3 / `fMoveEncumEffect` 0.4 / `iMaxCharacterLevel` 20.

  Every code default matches its executable. Oblivion's 10 s comes from the code comment ("loaded as a constant"); the table reader could not confirm it, because the 32-bit layout differs.

The two MEDIUM findings:

- **M-01**: New Vegas never lifts its level cap past 30. Each add-on's "+5" is applied by the executable; no master authors it.
- **M-02**: The fields that "Use Traits" governs (sex, voice type) are still read from the placed shell record outside the two spawn arms #6080/#6113 fixed. This affects dialogue voice, `GetIsSex`, `GetVoiceType`, `GetLeveledActorBase` and the gendered animation subgraph.

## Constant Verification Table

| Item | Code | Source | Verdict |
|---|---|---|---|
| FO3/FNV XP curve | `LevelingModel::FO3/FNV` `150L+50`, overlaid by `iXPBumpBase`/`iXPBase` (`leveling.rs` `with_gmst`) | `charal-fnv-fo3-ruleset.md:450-457`; GECK *Experience Settings* differencing gives `bump·L + (base−bump)`; masters author `iXPBumpBase` 150 | PASS |
| FO4 XP curve | `75L+125`, uncapped | `charal-fo4-ruleset.md:488-498`; Fallout4.esm `iXPBumpBase` 75 / `iXPBase` 200, no `iMaxCharacterLevel` | PASS |
| Skyrim XP curve | `25L+75`, `xp_per_skill_rank` 1 | `charal-skyrim-ruleset.md`; Skyrim.esm `fXPLevelUpBase` 75 / `Mult` 25 | PASS |
| FO3 cap | 20, raised by authored `iMaxCharacterLevel` | `charal-fnv-fo3-ruleset.md:459`; Fallout3.exe default 20; BrokenSteel.esm authors 30 | PASS |
| FNV cap | 30 (FalloutNV.esm) | `charal-fnv-fo3-ruleset.md:459` "50 with the four add-ons, +5 each" | **FAIL with DLCs → M-01** |
| Level rewards | FO3 `10+INT` + perk each level; FNV `10+INT/2` + perk on even levels; FO4 `(0, point)`; Skyrim perk + pool pick of 10 | `charal-fnv-fo3-ruleset.md:473-482`, `:466-469`; `charal-fo4-ruleset.md:492-496` | PASS |
| Kill XP | 0/10/20/30/50, 40 % share, FO3/FNV only | `charal-fnv-fo3-ruleset.md:484-491` | PASS FO3/FNV; FO4 row mislabelled (L-02) |
| Bleedout default | FO3/FNV 10 s, Skyrim/FO4 30 s, Oblivion 10 s, 0.3 regain | FO3/FNV `charal-fnv-fo3-ruleset.md:496-498`; Skyrim/FO4/Oblivion only in `scripting.md:618-622` and the exe tables | Values PASS (exe-verified); capture missing for 3 games (L-02) |
| Over-encumbrance | FO3/FNV `SpeedMult` 0.4; Skyrim `NoSprint`; FO4 `None` | FO3/FNV capture `:499-500`; Skyrim only in `encumbrance.rs` docs (UESP *Carry Weight*) | Values PASS; capture missing (L-02) |
| Barter (Skyrim) | Speechcraft, /100, cap 100, exe 2.4/1.4, `fBarterBuyMin` 1.05, `fBarterSellMax` 0.95, hooks 8/60 | `charal-skyrim-ruleset.md:97-117`; Skyrim.esm authors 3.3/2.0 only | PASS |
| Barter (FO4) | Charisma, /10, uncapped, exe 2.5/1.5, `fBarterBuyMax` 1.05, `fBarterSellMax` 0.95 | Fallout4.esm authors `fBarterMax` 3.5, `fBarterMin` 2.0, `fBarterBuyMax` 1.2, `fBarterSellMax` 0.8, which matches the capture formula `:229-240` | Code PASS; capture `:241-244` false (L-01) |
| ACBS Essential / Protected | bit 1 everywhere; bit 11 Protected except Oblivion/FO3/FNV | xEdit: FNV/FO3 bit 11 "No Blood Spray"; FO4/SF/FO76 bit 11 "Protected" | PASS |
| `SetReputation` axis | 1 Fame / 0 Infamy, clamped | GECK *GetReputation* / *AddReputationExact* | PASS |
| Derived rows (Dim 2) | unchanged since 2026-10-16 | 2026-10-16 table | carried PASS |

## Coverage Matrix

Re-derived from the `profile.rs` arms and workspace greps:

| Family | Ruleset implemented | Wired | Derived stats | Leveling model | Leveling runtime | Regen wired | Affliction wired |
|---|---|---|---|---|---|---|---|
| FO3 | ✓ `fallout3_ruleset` | ✓ | ✓ | `XpCurve` + GMST overlay | ✓ kill XP, `RewardPlayerXP`, `levelup` | ✗ inert | ✗ unregistered |
| FNV | ✓ `falloutnv_ruleset` | ✓ | ✓ | `XpCurve` + GMST overlay | ✓ (cap 30 even with DLCs, M-01) | ✗ | ✗ |
| FO4 | ✓ `fallout4_ruleset` | ✓ | ✓ | `XpCurve` uncapped | ~ no kill XP, no SPECIAL spend path | ✗ | ✗ |
| Skyrim | ✓ `skyrim_ruleset` | ✓ | ✓ | `SkillXp` + GMST overlay | ~ `AdvanceSkill` roll; perk + pool pick banked; `levelup pool` spend; no menu | ✗ | ✗ |
| Oblivion | ✓ `oblivion_ruleset` (built) | ✗ `RulesetBuilder::None` (deliberate) | n/a | `SkillUse` | ✗ | ✗ | ✗ |
| FO76 / Starfield | ✗ captures only | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |

## Findings

### MEDIUM

### CHAR-2026-10-21-M-01: New Vegas never lifts its level cap past 30, because each add-on's +5 raise is applied by the executable and appears in no master
- **Severity**: MEDIUM
- **Dimension**: Progression & Pools
- **Game**: fnv
- **Location**: `crates/core/src/character/leveling.rs` (`LevelingModel::FNV`, `level_cap: 30`; `with_gmst` `XpCurve` arm; the `XpCurve` variant doc, which says "a DLC master that raises the cap (`BrokenSteel.esm`) is honoured through the merged load order")
- **Status**: NEW
- **Source**: `charal-fnv-fo3-ruleset.md:459`: "FNV **30** (50 with the four add-ons, +5 each)". DeadMoney.esm `MESG` `NVDLC01LoadedMsg` (0x0100F41E) and HonestHearts.esm `MESG` `NVDLC02LoadMessage02` (0x01008A34) both say "…has been loaded. Your level cap has been raised by 5."
- **Description**: #5915 made the cap an overlay of the authored `iMaxCharacterLevel`. That covers FO3: BrokenSteel.esm authors 30. It does not cover New Vegas. A GMST scan of all ten FNV masters finds `iMaxCharacterLevel` only in FalloutNV.esm (30). The add-on masters author no such setting. The add-on quest script (`NVDLC01MQ00Script` source text) only shows the message, so the raise happens inside the executable. With Dead Money, Honest Hearts, Old World Blues and Lonesome Road loaded, the engine still stops the roll at 30, where the game allows 50. The `FNV` constant's doc ("cap 30 (50 with all add-ons)") and the `XpCurve` doc both read as if the add-on case is handled.
- **Evidence**: `/tmp/audit/character/scripts/gmst.py` over `CaravanPack`, `ClassicPack`, `DeadMoney`, `FalloutNV`, `GunRunnersArsenal`, `HonestHearts`, `LonesomeRoad`, `MercenaryPack`, `OldWorldBlues`, `TribalPack`. Only `FalloutNV.esm iMaxCharacterLevel 30` prints. `roll_levels` stops at `level >= level_cap()`, pinned at 30 by `experience_roll_stops_at_the_fallout_new_vegas_level_cap`.
- **Impact**: An FNV player with the add-ons loaded stops levelling at 30: no XP-driven levels, skill points or perks for 20 levels the game grants. This is silent; no log line names the cause.
- **Related**: #5915 (the GMST overlay fixed FO3 + Broken Steel only).
- **Suggested Fix**: First source how the executable triggers the raise, without guessing; the candidate is a loaded-master check for the four story add-ons. Then express it as data on the FNV profile, for example a `(master name, cap delta)` table applied after the GMST overlay. Until then, correct the two `leveling.rs` docs to say the add-on raise is not modelled.

### CHAR-2026-10-21-M-02: Sex and voice type, both "Use Traits" fields, are still read from the placed shell by dialogue voice, `GetIsSex`, `GetVoiceType`, `GetLeveledActorBase` and the animation subgraph (the siblings #6080 left)
- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: fnv, fo3, skyrim
- **Location**: `byroredux/src/systems/dialogue_voice.rs:375-416` (`npc_record` = `SceneAliasCandidate.base_form_id`, the placed shell; reads `voice_form_id` and `acbs_flags`), `:204` (`oblivion_voice_race`); `crates/scripting/src/actor_base_traits.rs:41-69` (`install_actor_base_traits` builds `female`/`voice` from every raw `NpcRecord`; `is_female` / `voice_type` feed condition 70 `GetIsSex`, `ActorBase.GetSex` and `GetVoiceType`); `crates/scripting/src/fragment/vm/natives/reference.rs:210` (`getleveledactorbase` aliased to `getactorbase`); `crates/scripting/src/anim_subgraph.rs:76`
- **Status**: NEW. The #6080 issue listed these four sites as "outside this area and not verified"; its fix (`415c35cff`) and #6113 (`1dfac76f6`) changed only the spawn arms.
- **Source**: GECK *Template Data*: "Use Traits: Use the traits of the ActorBase". CK/GECK *Traits Tab*: the Female checkbox and Voice Type are trait fields. xEdit `wbDefinitionsFNV.pas:6858-6860`: `VTCK` `.SetDontShow(wbActorTemplateUseTraits)`, and ACBS bit 0 Female is likewise Use-Traits. CK *GetLeveledActorBase*: "Gets the temporary ActorBase of a leveled Actor - the one generated by the game from the leveled list". CK *GetSex - ActorBase*: "To check for the sex of a Leveled Actor spawned in the world … call this function on GetLeveledActorBase".
- **Description**: Since #6080/#6113 the spawned body, head and race come from the Use-Traits terminal. Every other reader of the same two trait fields still reads the shell:
  - The voice resolver keys the archive folder on the shell's `VTCK` and the sex folder on the shell's Female bit.
  - `ActorBaseTraits` is a map built from raw `NpcRecord`s and looked up by the shell base. It answers `GetIsSex`, `GetSex` and `GetVoiceType`.
  - `GetLeveledActorBase()` returns the shell, so the CK's documented leveled-actor idiom `GetLeveledActorBase().GetSex()` gets the shell's sex.
- **Evidence**: Read-only census of the masters (`/tmp/audit/character/scripts/npc_traits.py`, terminals expanded through `LVLN` entries):

  | Master | Use-Traits + `TPLT` shells | Shell `VTCK` problem | Shell sex differs from every terminal |
  |---|---|---|---|
  | FalloutNV.esm | 744 | 646 author a `VTCK` that none of their terminals use | 35 (391 have mixed-sex terminals) |
  | Fallout3.esm | 337 | 275 author a `VTCK` that none of their terminals use | 0 |
  | Skyrim.esm | 2,053 | 1,133 author no `VTCK` at all | 106 |

  The shell value is not the one the game uses in any of these cases.
- **Impact**:
  - Templated FO3/FNV actors speak with a voice type the game would not use.
  - About 1,100 templated Skyrim actors resolve no voice key at all. Their lines fall back to the subtitle-time estimate, so they play no audio.
  - Sex-gated dialogue and package conditions (`GetIsSex`) and the gendered animation subgraph can disagree with the body the actor visibly spawned with.
  - Mod scripts that follow the CK's `GetLeveledActorBase` idiom read the wrong sex.
- **Related**: #6080, #6113, #4092/#4457 (same class: governed field read from the shell), #4137, #5750 (`GetIsVoiceType` is outside the catalog; once added it would inherit this read).
- **Suggested Fix**: Record the resolved Use-Traits terminal on the spawned actor, alongside `SceneAliasCandidate.base_form_id`, which must stay the shell for `GetActorBase`. Then read voice, sex and the animation subgraph through that terminal. Point `GetLeveledActorBase` at the terminal and keep `GetActorBase` on the shell, as the CK documents. Extend `resolve_inherited_call_sites_are_enumerated_and_pinned`'s field scan to `acbs_flags` / `voice_form_id` reads outside `equip.rs`.

### LOW

### CHAR-2026-10-21-L-01: The #5928 fix wrote a false sentence into the FO4 barter capture: Fallout4.esm does author the price caps, and they are 0.8 / 1.2
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: fo4
- **Location**: `docs/engine/charal-fo4-ruleset.md:241-244` (added by `2c16ef06d`)
- **Status**: NEW (introduced by the #5928 fix)
- **Source**: Fallout4.esm `GMST`s read this run: `fBarterSellMax` 0.8, `fBarterBuyMax` 1.2, `fBarterMax` 3.5, `fBarterMin` 2.0. Fallout4.exe defaults: `fBarterSellMax` 0.95, `fBarterBuyMax` 1.05. The executable has no `fBarterBuyMin` string.
- **Description**: The capture now says the executable's "`fBarterSellMax=0.95` and `fBarterBuyMin=1.05`; Fallout4.esm does not author them. The sale may not exceed 95% of item value, and the purchase may not fall below 105%". That is false on three counts:
  - FO4's floor setting is `fBarterBuyMax`, not `fBarterBuyMin`.
  - The master authors both caps.
  - The effective limits are 80 % and 120 %.

  The paragraph also contradicts the capture's own formula block three lines above ("floored at 1.2 / ceiled at 0.8"). The code is right: `BarterModel { buy_floor_setting: "fBarterBuyMax", .. }` reads the authored values, and `barter.rs`'s module doc states them correctly. The Skyrim half of the same commit is correct.
- **Impact**: The capture is the authority for every CHARAL constant. A future reader "fixing" the code to match it would raise FO4 sell prices from 80 % to 95 % of value.
- **Suggested Fix**: Replace the paragraph: Fallout4.esm authors `fBarterBuyMax` 1.2 / `fBarterSellMax` 0.8 (the executable defaults 1.05 / 0.95 apply only when a load order removes them).

### CHAR-2026-10-21-L-02: The #5921 profile rows have no capture line for Skyrim/FO4/Oblivion; FO4's row cites a capture note that does not exist and states "awards no kill experience" where the capture says FO4 has kill XP
- **Severity**: LOW
- **Dimension**: Ruleset Seam / Coverage & Doctrine
- **Game**: skyrim, fo4, oblivion
- **Location**: `crates/core/src/character/profile.rs` rows: `SKYRIM` (`over_encumbered: NoSprint`, `essential_death_time_secs: Some(30.0)`), `FALLOUT4` (`:397` "#5921 capture note: intended, not an oversight"; `:400` `kill_experience: false`; `essential_death_time_secs: Some(30.0)`), `OBLIVION` (`Some(10.0)`); field doc `:240-244`; `docs/engine/charal-{skyrim,fo4,oblivion}-ruleset.md`
- **Status**: NEW (#5929 added the FO3/FNV sections only)
- **Source**: SKILL: "a coefficient no capture supports is a finding". The values are right: they match the executables' setting tables (Executive Summary), `scripting.md:618-622` and the UESP *Carry Weight* note in `encumbrance.rs`. `charal-fo4-ruleset.md:501-502`: the XP multiplier "scales XP from all sources … Survival mode doubles kill XP".
- **Description**: #5921 made these tables CHARAL profile data, but only the FO3/FNV capture carries them. The Skyrim, FO4 and Oblivion captures lack:
  - the bleedout death-time default;
  - the `NoSprint` rule;
  - the barter executable defaults.

  The FO4 encumbrance comment cites a "capture note" that `charal-fo4-ruleset.md` does not contain (no encumbrance section exists). FO4's `kill_experience: false` follows the field doc "whether killing an actor awards experience". That says FO4 awards none, while the capture says it does. The true state is that FO4's kill-XP formula is uncaptured, so the consumer is off.
- **Impact**: Document-first verification cannot check these rows. The FO4 row encodes "unsourced" as a false game fact, which a later reader may treat as settled.
- **Suggested Fix**: Add a short "Bleedout, encumbrance, barter defaults" block to each of the three captures, citing the exe tables. For FO4, state there that over-encumbrance and kill XP are not wired because the rules are uncaptured. Reword the `kill_experience` doc to "whether kill experience is wired (sourced)".

### CHAR-2026-10-21-L-03: Prose still says Skyrim NPC pools are race base + ACBS offset only, and that `with_gmst` handles only `SkillXp` (stale since #5914 / #5915)
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: skyrim
- **Location**:
  - `crates/core/src/character/profile.rs:353-365` (`SKYRIM` row comments: "#4454 … the class term is … uncoded, so leveled NPCs get flat pools", and "`LevelingModel::with_gmst`'s one handled variant (`SkillXp`) … without this arm `with_gmst` executed only inside its own unit test").
  - `crates/plugin/src/esm/records/actor_value_derive.rs:11-15` (module doc) and `:138-140` (`derive_npc_actor_values` doc).
  - `docs/feature-matrix.md:303-305`.
- **Status**: NEW. #6082 corrected the Skyrim capture and `derive_skyrim_actor_values`' own doc, not these.
- **Source**: `derive_skyrim_actor_values` doc (`actor_value_derive.rs:308-316`, #5914: the `10·(level−1)` class term and Health `5·(level−1)` are implemented); `LevelingModel::with_gmst` `XpCurve` arm (#5915).
- **Impact**: Doc rot only, but in exactly the places the next audit and the feature matrix read first.
- **Suggested Fix**: Update the three Skyrim-pool descriptions to name the class/level term. Drop the "one handled variant" comment, or reword it to say Skyrim was the first family wired to `with_gmst`.

## Considered and discarded

- **Skyrim exe barter defaults 2.4/1.4 vs the capture's 3.3/2.0**: the capture names the *authored* Skyrim.esm values (verified 3.3/2.0); the code's 2.4/1.4 are SkyrimSE.exe defaults (verified) used only when no master authors the setting. Consistent.
- **ObScript `ModAV`/`ForceAV` onto the player's derived pools**: `mod_actor_value_permanent` *adds* into `permanent_mod` (never overwrites); `refresh_player_only_bases` preserves that layer. `SetAV` routes through `set_actor_value_base`, which honours `is_player_derived_pool`. No #5239-class revert.
- **`levelup pool` writes `set_base`**: Skyrim's Health/Magicka/Stamina are not derived rows (`skyrim.rs`: only player-only Carry Weight inputs), so no per-frame refresh reverts the +10.
- **FNV perk cadence**: `new_level.is_multiple_of(2)` grants perks at even levels; matches the capture.
- **`GameKind` in `npc_spawn*`**: all remaining uses are skeleton, body and KF asset tables or wire-format decodes (`is_child_race` race-flag bit), not rules.

## Regression Checks

| Item | State |
|---|---|
| #5921 (consumer `GameKind` tables) | Fixed: `encumbrance.rs`, `barter.rs`, `combat.rs:506`, `bleedout.rs::engine_defaults`, `PerkEntryTable` read profile rows; `per_game_rule_rows_are_stated_on_every_profile` pins them; doctrine grep clean |
| #5915 (Fallout curve/cap GMSTs) | Fixed for FO3/BS (see M-01 for FNV add-ons) |
| #5916 / #5918 / #5925 | Fixed: pool picks banked and spent; `roll_levels` bounded (`ceil`, `max(1)`, `u16::MAX`); one reward query |
| #5917 (base INT) | Fixed: `ActorValueView::Base` in `grant_experience` |
| #5914 / #5922 / #5927 / #5920 / #5919 | In place (re-confirmed 2026-10-19, no later diff to their sites) |
| #6080 / #6113 | Spawn arms fixed; siblings outside spawn remain (M-02) |
| #4232 | Still open, still true (`effective_actor_level` non-multiplier branch `.max(0)`; single implementation, no `.max(1)` copy) |
| #4137 | Open (template-bit consumers); unchanged |

## Known-Open Register (restated, none re-filed)

- FNV/FO3 tag-skill per-level formula undocumented; CLAS SPECIAL in `ATTR`.
- FO3/FNV AP/Crit/Melee/Unarmed `.player_only()` (#2937, documented).
- VATS runtime absent.
- Regen and affliction are inert (verified this run). `PoolRegenConfig` is never inserted outside tests. `pool_regen_tick_system` is registered (`boot/schedule/update.rs:741`). `affliction_tick_system` has no scheduler registration (`registry_completeness_tests.rs:461`).
- Oblivion `RulesetBuilder::None` is deliberate (pin present). FO76/Starfield have captures only.
- #4137, #4232.

## Cross-audit notes

- **ESM-2026-10-21-02** (FO76 `NPC_` ACBS 20-byte layout unmatched → flags 0) and the **GAME-D4** death/magic-effect findings are already reported; not repeated. With FO76 flags at 0, `NpcRecord.essential/protected` (#5921) are false for every FO76 actor. That is a consequence of ESM-02, not a separate finding.
- **Starfield essential actors (observation, routed to `/audit-gameplay` / `/audit-starfield`)**: since #6120 (`78facba89`), Starfield `ACBS` flags decode, so `essential` is now true for Starfield's essential bases. `CharacterRulesProfile::STARFIELD.essential_death_time_secs` is `None`. Starfield.exe does carry `fEssentialHealthPercentReGain` but not `fEssentialDeathTime`. So a spared Starfield essential enters bleedout with no `BleedoutRules` and never recovers. The real Starfield recovery rule is unsourced here, so this is not filed.
- M-02 touches dialogue (`/audit-gameplay`), the Papyrus natives and condition 70 (`/audit-scripting`) and the animation subgraph (`/audit-nifal` / animation). It is filed here because the class is the CHARAL Population Boundary's "governed field read from the shell".

## Summary

5 findings: **0 CRITICAL · 0 HIGH · 2 MEDIUM · 3 LOW**, all NEW. There are no regressions. All 2026-10-16 and 2026-10-19 published fixes are closed and verified in place.

Not checked:
- The `#[ignore]`d real-master oracles, because plugin `--ignored` is forbidden by the suite rules. The read-only Python censuses above stood in for them.
- Anything needing the engine.
- Oblivion's 10 s bleedout default at exe level.

Next: `/audit-publish docs/audits/AUDIT_CHARACTER_2026-10-21.md` (domain label `character`; `game:fnv` for M-01, `game:fnv`/`game:skyrim` for M-02, `game:fo4` for L-01, `doc-rot` for L-01/L-03).
