**HEAD**: cb5368348 · **Baseline**: [`AUDIT_CHARACTER_2026-10-16.md`](AUDIT_CHARACTER_2026-10-16.md) (HEAD `fa7591ca2`) · **Audited**: Dim 4 (Population Boundary), scoped to `byroredux/src/npc_spawn.rs` + `npc_spawn/{resumable/,ai_package.rs}` and the plugin-side population code they call (`actor_value_derive.rs`, `equip.rs` `ResolvedNpc`, `actor/mod.rs` `effective_actor_level`); scope=streaming-deep AREA-SCOPED · **Unchanged since baseline (skimmed)**: none whole. Dims 1, 2, 3, 5 are out of area (one-line entries below).

# Character / CHARAL Audit — 2026-10-19 (streaming-deep, AREA-SCOPED)

**This report is AREA-SCOPED (`--area streaming-deep`), not a full-codebase CHARAL audit.** The next full run must not treat it as a baseline for Dims 1-3 and 5; the baseline for those remains `AUDIT_CHARACTER_2026-10-16.md`. Lens: the character/NPC population path inside `npc_spawn` (parse -> `ResolvedNpc` -> `ActorValues` / `CharacterLevel` / `Background` / `Perks` / `FactionRanks` / `SpellList`). Read-only; no engine launched; no private cargo target dir; scratch (census scripts only) under `/tmp/audit/streaming-deep/character/`.

## Dimension status

| Dim | Status |
|---|---|
| 1 Ruleset Seam & Doctrine | Out of area, except the population-side `GameKind` grep: `npc_spawn.rs` / `resumable/` `GameKind` uses are asset-path and mesh tables (skeleton, body, KF clips), not rules. The only rule-shaped one is `ActorProtection::from_acbs_flags(.., index.game)` (`resumable/mod.rs:390`) = existing #5921 (open, still true). `build_character_ruleset` (`npc_spawn.rs:302`) is still the only construction site; its two callers (`cell_loader/references/mod.rs:348`, `inventory.rs:452`) both treat `None` as "no CHARAL". |
| 2 Derived Formulas | Out of area (no CHARAL core file touched in the delta). The area-side coefficients were checked against sources, see the verification table. |
| 3 Progression & Pools | Out of area. |
| 4 Population Boundary | **Audited** (this report). |
| 5 Coverage & Doctrine | Out of area, apart from the three doc-rot items filed here. |

## Tests recorded

| Command | Result |
|---|---|
| `cargo test -p byroredux-plugin --lib -- actor_value_derive equip effective_actor_level pc_level` | 112 passed, 0 failed, 5 ignored (real-master pins, not run: `--ignored` on the plugin crate is forbidden by the suite rules) |
| `cargo test -p byroredux --bin byroredux -- resolve_inherited_call_sites npc_spawn morality perks` | 150 passed, 0 failed, 10 ignored. `resolve_inherited_call_sites_are_enumerated_and_pinned` green: production `resolve_inherited_*` calls are still exactly two (`cell_loader/references/mod.rs`, `player_body.rs`). |

## Delta since the baseline (Dim 4 paths)

17 commits since 2026-10-16. The ones that matter here: `b9e60846e` #5914 (Skyrim pools gain the class/level term), `b14a1a7ec` #5920 (the player receives its own `Perks`), `a1eece82a` #5927 (Skyrim NPC Morality), `91d5e8d5c` #5922 (Skyrim skill tie order), `a5343c806` #6051 / `a95dc4ce3` (declined packages), `2a8663f39` #5408 (facegeom-less head from race base parts). The ObScript commits (`af5db229d`, `da2676b70`, ...) touch `cell_loader/references/` for scripting, not the population stamps.

## Regression checks (2026-10-16 findings that touch this area)

| Finding | State | Evidence |
|---|---|---|
| M-01 / #5914 Skyrim pools omit class/level term | **Fixed, in place** | `actor_value_derive.rs:337-346` `skyrim_class_pool_points`, weights from `class.rs:146-153` bytes 32..34 (layout checked: 4 + 1 + 1 + 18 skill weights + f32 + u32 puts the three attribute weights at 32..34). Unit tests green; the real-data oracle `skyrim_autocalc_pools_match_the_baked_dnam_pools` is `#[ignore]`d and was NOT run. |
| M-06 / #5919 `ScriptedFactionChanges` not pre-reload | **Fixed, in place** | `live_load.rs:36-41` `PRE_RELOAD_RESOURCES` carries it; replay at `synth_child.rs:97` and `live_load.rs:665`. See the SAVE-SD-01 note below. |
| M-07 / #5920 player gets no `Perks` | **Fixed, in place** | `inventory.rs:425` reads `resolved.shell.perks`, `:848-856` stamps; the attach test no longer hand-inserts (`:3732-3786`). |
| L-01 / #5922 skill tie order | **Fixed in code**, doc not updated (see CHAR-SD-02) | `actor_value_derive.rs:430-432`. |
| L-06 / #5927 NPC Morality | **Fixed, in place** | `actor_value_derive.rs:231-244`; Skyrim AVIF EDID is `AVMorality`, resolved through `actor_value_form_id`'s `AV`-prefix fallback (`index.rs:1033-1045`); `AIDT` byte 3 on Skyrim.esm only holds 0..3 (3292 / 1723 / 99 / 4), so the `_ => None` arm is unreachable on vanilla. |
| M-08 / #5921 `GameKind` consumers | **Still open, still true** for the in-area site (`resumable/mod.rs:390`). |
| #4232 `effective_actor_level` returns 0 verbatim | **Still open, still true** (`actor/mod.rs:176-182`: non-multiplier branch is `.max(0)`). Single implementation; no `.max(1)` copy anywhere. |
| #4137 unconsumed `template_flags` bits | **Open, premise stale** (CHAR-SD-03). |
| #4817 / #4822 / #5238 / #5338 / #5391 | In place: `arm_combat_disposition` at finalize (`resumable/mod.rs:650`); `SpellList` inserted even when empty (`npc_spawn.rs:124-127`); NPC Health curve + authored-DATA SPECIAL (`actor_value_derive.rs:657-695`). |

## Constant Verification Table (area-side numbers only)

| Item | Code | Document + line | Verdict |
|---|---|---|---|
| Skyrim NPC skill base / per-level points / cap | `SKYRIM_BASE_SKILL` 15, 8 per level above 1, clamp 100 (`actor_value_derive.rs:403-408`) | UESP *Skyrim Mod/Mod File Format/CLAS* (local `uesp-wiki`) lines ~66-75; `charal-skyrim-ruleset.md:731-740` | PASS |
| Skyrim pool growth | Health `5·(L-1)`, `10·(L-1)` by the three `CLAS DATA` weights, leftovers by weight desc then **reverse** AV order (`:458-503`) | UESP CLAS lines 104-107, 112 ("stamina, magicka, then health") | PASS vs the page; the capture's prose is inconsistent (CHAR-SD-02) |
| Skyrim skill leftover tie | descending AV index (`:430-432`) | UESP CLAS line 75 says lower index first; `charal-skyrim-ruleset.md:733-737` records that the baked `DNAM` values overrule it (916 / 929) | PASS against the capture (documented divergence from the page); fn doc stale (CHAR-SD-02) |
| FO3/FNV auto-calc skill | `2 + 2·governing + ceil(Luck·0.5)` (`:121-128`) | `charal-fnv-fo3-ruleset.md:46`; GECK *Derived Skill Settings* | PASS (the two multipliers are unauthored engine defaults, disclosed in place) |
| FO3/FNV NPC Health | `NpcHealthCurve` via `with_gmst` (`:226`, `:684-695`) | `charal-fnv-fo3-ruleset.md:94` | PASS (not recomputed this run; no CHARAL core file changed) |
| ACBS PC-Level-Mult bit | `ACBS_PC_LEVEL_MULT = 0x0080` (`actor/mod.rs:139`) | `wbDefinitionsFNV.pas` ACBS flags bit 7; `wbDefinitionsTES5.pas` `{0x00000080} 'PC Level Mult'` | PASS |
| ACBS Auto-calc bit | `0x0010` on both arms (`:455`, `:631`) | both xEdit files: bit 4 | PASS |
| Template flag bit meanings | `equip.rs:372-397` | `wbTemplateFlags` order (Traits 0, Stats 1, Factions 2, Spell List 3, AI Data 4, AI Packages 5, Base Data 7, Inventory 8) | PASS |

## Findings

### CHAR-SD-01: The runtime-FaceGen spawn arm (Oblivion/FO3/FNV) takes gender from the shell record, but the Female flag is a Use-Traits field and the race beside it comes from the Use-Traits terminal
- **Severity**: MEDIUM
- **Dimension**: Population Boundary
- **Game**: fo3, fnv (Oblivion has no `TPLT`; Skyrim/FO4 take the correct path)
- **Location**: `byroredux/src/npc_spawn/resumable/runtime.rs:222` (`Gender::from_acbs_flags(npc.acbs_flags)`); correct siblings: `resumable/prebaked.rs:102` and `npc_spawn.rs:517` (`traits.acbs_flags`). Also `runtime.rs:302` logs the shell's `npc.race_form_id` rather than the resolved race, and `runtime.rs:475` (creature arm) reads the shell as well, harmless there.
- **Status**: NEW (same defect class as #4092 / #4457, which fixed the race half)
- **Source**: `wbDefinitionsFNV.pas:6828`: `.SetFlagHasDontShow(0, wbActorTemplateUseTraits)` on ACBS flag bit 0 `'Female'` (xEdit copy at `/mnt/data/tmp/`); the same file tags bit 4 / bit 7 as Use Stats and bits 1 / 3 / 9 as Use Base Data, which is exactly the split `ResolvedNpc` implements. `ResolvedNpc::traits` is documented as "race, ACBS gender flags, skeleton" (`equip.rs:677-678`).
- **Description**: `prepare_runtime_state` resolves the NPC once (`spawn_placement_root`), takes the race from `resolved.traits` (`ActorBodyClass.race_form_id`, `build_npc_equip_state`, and the caller's `RaceRecord` via `resolve_inherited_traits`), then derives `gender` from the **shell's** ACBS. That `gender` selects the body meshes (`humanoid_body_paths`), the head-part gender section (`want_gender_tag`), the gendered armor models inside `build_npc_equip_state`, and is persisted as `ActorBodyClass.gender` for mid-life gear imports. The pre-baked arm and `npc_skeleton_path` read the same flag from the traits terminal, so the two arms disagree for the same record shape.
- **Evidence**: Throwaway read-only census of the shipped masters (`/tmp/audit/streaming-deep/character/census.py`): FalloutNV.esm has 744 `NPC_` shells with Use Traits + `TPLT`; 127 point at a direct `NPC_` (0 differ in the Female bit) and 617 point at an `LVLN`. Using the code's own deterministic pick (highest eligible level, last on a tie, no nested lists), **156 of the 617** LVLN-templated FNV shells have a Female bit different from the picked template; Fallout3.esm has 164 LVLN-targeted shells, 1 differs. The measure is approximate (it does not model nested lists), so the true count is a bound-ish figure, not exact. The real-engine pick among eligible entries is random, which only makes the template the sole stable gender source.
- **Impact**: A shell flagged female whose picked template is male (or the reverse) spawns with the template's race and Background but the other gender's body, head parts and gear models. Visual/identity inconsistency, no crash; confined to LVLN-templated FNV shells (about a quarter) and almost none on FO3. The `resolve_inherited_call_sites_are_enumerated_and_pinned` guard cannot see it, because it counts `resolve_inherited_*` calls and not raw `npc.<field>` reads of governed fields.
- **Related**: #4092, #4457, #4137 (CHAR-SD-03). Outside this area and not verified for templated shells: `systems/dialogue_voice.rs:204,416`, `crates/scripting/src/anim_subgraph.rs:76`, `crates/scripting/src/actor_base_traits.rs:50` also read the shell's `acbs_flags` for sex.
- **Suggested Fix**: Use `resolved.r#traits.acbs_flags` at `runtime.rs:222` (the creature arm's `:475` can follow for uniformity), log the resolved race at `:302`, and add a spawn-boundary test with a female shell over a male template asserting `ActorBodyClass.gender` on both arms.

### CHAR-SD-02: Two prose descriptions of the Skyrim NPC derivation contradict the code they describe
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine (doc rot)
- **Game**: skyrim
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:410-416` (doc of `skyrim_class_skill_points`: "actor-value index (lower first)"; the body at `:430-432` sorts by descending index); `docs/engine/charal-skyrim-ruleset.md:726-729` ("distributes `10 × (level − 1)` points across Magicka and Stamina using their class weights"; the code and `charal-skyrim-ruleset.md:612` split them across Health, Magicka and Stamina by all three `CLAS DATA` weights).
- **Status**: NEW (the code side of both is the verified one: #5922 / #5914)
- **Source**: UESP *CLAS* lines 104-107 (Health also takes a weighted share on top of `5·(L-1)`); `charal-skyrim-ruleset.md:733-737` (higher-index-first tie, settled by baked `DNAM`).
- **Description**: #5922 flipped the skill tie order and #5914 added the pool term, but the function doc still states the old order and the "LOCKED" capture paragraph (the authority for constants) states a two-pool split that the same document's line 612 and the code contradict. Whoever re-derives from the capture will drop Health's weighted share.
- **Evidence**: grep for "lower first" / "across Magicka and Stamina" at the cited lines; `skyrim_class_pool_points` indexes all three `weights`.
- **Impact**: Documentation only; the shipped numbers match the baked data per the commit's own oracle.
- **Related**: #5914, #5922.
- **Suggested Fix**: Correct the function doc to "higher index first" and the capture paragraph to "Health, Magicka and Stamina by their class weights, Health also +5 per level".

### CHAR-SD-03: #4137's "six unconsumed template bits" premise is stale, and `NpcRecord::template_flags` still tells readers five consumed bits have "no consumer yet"
- **Severity**: LOW
- **Dimension**: Population Boundary (doc rot)
- **Game**: all
- **Location**: `crates/plugin/src/esm/records/actor/npc.rs:378-380`; open issue #4137
- **Status**: Existing: #4137 (update the issue, not a new one)
- **Source**: current consumers: `ResolvedNpc` fields `factions` (0x04), `spells` (0x08), `ai_data` (0x10), `ai_packages` (0x20), `base_data` (0x80) (`equip.rs:670-696`, consumed at `resumable/mod.rs:380-395`, `npc_spawn.rs:118`, `ai_package.rs:822`).
- **Description**: The doc comment lists `0x0004`, `0x0008`, `0x0010`, `0x0020` and `0x0080` as "parsed and stored for the dispatcher; no consumer yet". All five have consumers. Of #4137's six bits, 0x08, 0x10 and 0x80 are consumed now; the genuinely unconsumed ones are `0x0040` Model/Animation (hair, eyes, FaceGen recipe), `0x0200` Script, `0x0400` Def Pack List, plus Skyrim `0x0800` / `0x1000` and the rest of FO4's 13-slot `TPTA`. The comment's `Use Traits (race)` also omits the gender flag that CHAR-SD-01 shows is governed by the same bit.
- **Evidence**: grep of `TEMPLATE_FLAG_USE_*` consumers; `equip.rs:372-397`.
- **Impact**: Misleads the next reader into thinking CHARAL-adjacent reads (Perks aside) have free choice of source.
- **Related**: CHAR-SD-01.
- **Suggested Fix**: Rewrite the bullet to list only truly unconsumed bits, note that Perks and head parts ride no template flag in xEdit, and re-scope #4137 to `0x40` / `0x200` / `0x400`.

## Considered and discarded (premise disproved)

- **Auto-calc FO3/FNV NPC whose class does not resolve returns no values even with authored `DATA` SPECIAL** (`derive_autocalc_actor_values` returns on a missing class before reading `data_attributes`): the census found zero vanilla `NPC_` in FalloutNV.esm / Fallout3.esm without a resolvable class and without Use Stats. Not reachable on shipped data; modded-only.
- **FNV/FO3 creatures with `DATA` health <= 0 get no Health and no `ActorVitals`**: 77 FNV / 28 FO3, nearly all authored corpses (`...DEAD`, `Dead...`), consistent with the #5005 corpse rule.
- **Raw `PRKR` ranks bypass `Perks::set_rank` invariants** (`stamp_character_components` builds `entries` directly): Skyrim.esm and Fallout4.esm carry only rank 1 and zero duplicate perks per NPC.
- **`ResolvedNpc::resolve` runs twice per NPC** (`spawn_placement_root` and `finish_actor`, `resumable/mod.rs:648`): by design, a yielded job cannot hold the borrowed view across frames; same data, small cost.
- **Scripted `AddPerk` on an NPC that has no `Perks`** (the #4458 / #5413 class): `fragment/effects.rs:581-583` inserts a default component, so there is no silent no-op.
- **`Morality` seeded as 0 for "Any Crime" NPCs makes the captured bribe formula price them at 0**: follows the cited formula; no contradicting source found, so not reportable (*feedback_no_guessing*).

## Coverage Matrix

Not re-derived (out of area). Baseline from 2026-10-16 stands: FO3 / FNV / FO4 / Skyrim wired, Oblivion blocked by design (`build_character_ruleset` -> `None`, callers handle it), FO76 / Starfield captures only.

## Known-Open Register (restated, none re-filed)

#5921 (`GameKind` consumers, includes the in-area `ActorProtection` site) · #4232 (`effective_actor_level` 0 verbatim, confirmed) · #4137 (re-scope, CHAR-SD-03) · FNV/FO3 tag-skill per-level undocumented · FO3/FNV AP/Crit/Melee/Unarmed `.player_only()` (#2937) · VATS runtime absent · regen/affliction insertion sites absent (not re-verified, out of area).

## Cross-audit notes (dedup, nothing re-reported)

- **SAVE-SD-01** (refused load already overwrote live ledgers, `live_load.rs:512`): the #5919 fix routes `ScriptedFactionChanges` through the same pre-reload restore, so the faction ledger is among the ledgers lost on a refused load. Owned by the save audit.
- **SAVE-SD-03** (`ReferenceState` lacks `ActiveMagicEffects`, ...): character-relevant; confirmed the parked row carries `actor_values`, `spells`, `perks`, `control`, `bleedout` (`reference_state.rs:25-60`). Not re-reported.
- Sibling IDs EXT-D6-01..04, EXT-D2-05, #6053, CONC-SD-01..03, PERF-SD-01..05, #5715, #5999, GAME-SD-01..04, ECS-SD-01..04 were not touched by anything found here.

## Summary

3 findings: **0 CRITICAL · 0 HIGH · 1 MEDIUM · 2 LOW** (CHAR-SD-03 is an update to existing #4137). 0 regressions; 4 prior fixes (#5914, #5919, #5920, #5927) verified in place. Could not check: the `#[ignore]`d real-master oracles (`skyrim_autocalc_pools_match_the_baked_dnam_pools`, roster / named-NPC pins; plugin `--ignored` is forbidden), anything needing the engine or Vulkan, and the other four CHARAL dimensions.

Next: `/audit-publish docs/audits/AUDIT_CHARACTER_2026-10-19.md` (domain label `character`; add `game:fnv` / `game:skyrim` per finding).
