# VATS and Action Points — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in
review round 1 (perk entry-point roles corrected against GECK *Entry
Point*, the base-chance finding sharpened with its arithmetic consequence,
FO4 AP regen re-sourced to `fActionPointsRestoreRate`, two verify items
closed from local GECK pages, Starfield evidence re-based on game data,
Q3/Q4 aligned with the sibling PRDs, menu phase tied to Game UI Phases
1 and 3 with `GUI-OPS-005` driving it headless, co-op session gate noted). Revision 1 was the first draft: the
#6350 remainder after Phases 1–3 (VATS menu, per-limb damage and limb
health, playback and camera, Concentrated Fire, the captured to-hit ground
truth #6403, FO4's slow-motion lane). Statements about how the original engines
behave cite external sources (the GECK and Fallout 4 CK wiki dumps under
`/mnt/data/src/reference/`, the Fallout wiki, the games' own master files
read with a stated recipe); a claim that could not be verified is marked
**Phase 0: verify** instead of being stated as fact.

**Scope**: The Vault-Tec Assisted Targeting System as the player meets it in
Fallout 3, Fallout: New Vegas and Fallout 4: entering the mode, pricing every
body part of every valid target, queueing attacks against the Action Points
pool, executing them in a playback with its camera, and the limb damage those
attacks do. The promise is to the player of those three titles and to the
mods that re-tune VATS through game settings, body-part data and weapon
records. Outside it: the menu framework itself (the
[Game UI Integration PRD](../prd/game-ui-integration.md), M48), the perk
entry-point evaluator (the [Perk Entry Points PRD](../prd/perk-entry-points.md),
M24.2),
Fallout 76's real-time VATS, Skyrim's kill cameras, and gore rendering.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| VATS tuning (`fVATS*` / `fActionPoints*`), AP regen, sourcing | [CHARAL FNV/FO3 ruleset § VATS tuning](../engine/charal-fnv-fo3-ruleset.md#vats-tuning-action-points-regen-and-the-radiation-gauge--built-6350) |
| AP pool and its regeneration lane | [CHARAL §4.7](../engine/charal.md#47-poolregenaccumulator--new-built-pool-regeneration-the-layers-only-fixed-step-system) |
| FO4 Action Points and regen | [CHARAL FO4 ruleset § Action Points](../engine/charal-fo4-ruleset.md#action-points--locked) |
| VATS runtime: mode machine, scan, queue, limb damage, playback, camera | none yet; Phase 0 writes `docs/engine/vats.md` |
| VATS menu on MenuXml (FO3/FNV) and Scaleform (FO4) | [UI System § MenuXml](../engine/ui.md#menuxml-legacy-ui-m484-oblivion--m485-fo3), [UI System § Scaleform HUD](../engine/ui.md#scaleform-hud-m486-skyrim--m487-fallout-4) |
| Schedule placement of `vats_mode_system` | [Game Loop § schedule](../engine/game-loop.md#the-system-schedule-m27-declared-access) |
| `Control::Vats` input-enable domain | [Scripting § Input enable layers](../engine/scripting.md#input-enable-layers-m472) |
| `vats.*` console commands | [Debug CLI](../engine/debug-cli.md) |
| Menu registry, stack, input routing, MenuXml menus beyond the HUD | [Game UI Integration PRD](../prd/game-ui-integration.md) (`GUI-REG`, `GUI-XML`) |
| Perk entry-point hooks the VATS values compose through | [Perk Entry Points PRD](../prd/perk-entry-points.md) (`PERK-HOOK`) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `VATS-` prefix.

---

## 1. Product thesis

VATS is the Fallout games' signature combat mechanic: the player stops the
fight, reads a hit chance off every limb of every enemy, spends Action Points
on a queue of shots, and watches them play out. Fallout 3's design paused
real-time combat for it ("While using V.A.T.S., the otherwise real-time
combat is paused", Fallout wiki,
[*Fallout 3 V.A.T.S.*](https://fallout.wiki/wiki/Fallout_3_V.A.T.S.));
Fallout 4 slows global time instead (Fallout 4 CK wiki, *OnTimer -
ScriptObject*: "if global time is slowed because the player is in VATS
selection"). A Fallout title without it is not that title.

ByroRedux has built the data half and the loop's skeleton (§3): every number
comes from the load order's game settings with the vanilla value as the
fallback, and the targeting loop runs from the debug console. What is missing
is everything the player actually perceives (the menu, the playback, limbs
that cripple) and the one test that proves the hit chances are the game's
rather than a plausible reconstruction (#6403).

The corollary that shapes everything below: **every VATS number is load-order
data or captured ground truth, never an engine constant**, and every per-game
difference lives in the game's CHARAL profile row (`CharacterRulesProfile::vats()`),
never in a branch inside the scan, the playback or the menu. A reader can
therefore say what the engine will not do: it will not ship a hit-chance
formula that no capture has checked, it will not hard-code a camera, and it
will not test `GameKind` inside a VATS system.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **VATS** | The targeting mode as a whole. Its engine states are the five `GetVATSMode` values (§3.1). |
| **AP** | Action Points: the player's `ActionPoints` actor value, the currency of a VATS queue. Player-only on FO3/FNV ([CHARAL FNV/FO3 ruleset](../engine/charal-fnv-fo3-ruleset.md#vats-tuning-action-points-regen-and-the-radiation-gauge--built-6350)). |
| **`VatsModel`** | The per-game tuning model (`crates/core/src/character/vats.rs`): one field per `fVATS*` / `iVATS*` / `fActionPoints*` setting, load-order GMST over a compiled-in default. Data. |
| **`VatsState`** | The transient runtime resource (`byroredux/src/components/vats.rs`): mode, scan result, queue, AP reservation, die-roll state. State. |
| **Profile row** | `CharacterRulesProfile::vats()`: the game's `VatsModel` defaults, `None` where VATS is not sourced and wired. The only place a per-game VATS difference may live. |
| **Limb table** | A `BPTD` record: the body parts of one skeleton. Humanoids use the `DefaultBodyPartData` slot; creatures name theirs in `CREA.PNAM`. |
| **Body part** | One `BPTD` entry (`BptdPart`): name, target bone, damage multiplier, health percent, limb-condition actor value, to-hit chance, flags. |
| **Limb condition** | The actor value a body part names (`PerceptionCondition` … `BrainCondition` on FO3/FNV). The limb is **crippled** when it reaches zero. |
| **Scan** | `vats_scan`: price every body part of every valid target with the equipped weapon. |
| **Queue / reservation** | The confirmed-pending attacks and the AP they will cost; AP is charged at confirm. |
| **Playback** | `GetVATSMode` 4: the queued attacks executing, with the VATS camera. |
| **Camera shot / camera path** | `CAMS` / `CPTH` records: a playback camera, and the conditional list that selects shots (GECK wiki *CameraShot*, *Camera Paths*). |
| **Ground-truth capture** | A hit percentage read off the vanilla game's VATS display for a fixed, written-down scenario, pinned as a test constant. |
| **Concentrated Fire** | The perk that raises the chance of consecutive shots at the same part by `iVATSConcentratedFireBonus` per prior shot. |

---

## 3. Core model

### 3.1 Mode states

The engine's mode vocabulary is the script-facing `GetVATSMode` contract
(GECK wiki *GetVATSMode*: 0 `VATS_MODE_NONE`, 1 `VATS_TARGET_SELECT`, 2
`VATS_MENU_MODE`, 3 `VATS_SCAN_MODE`, 4 `VATS_PLAYBACK`). `VatsMode` in
`byroredux/src/components/vats.rs` carries those numeric values. The table is
the state at HEAD on 2026-11-01.

| Value | State | World time | Today |
|---|---|---|---|
| 0 | None | 1.0 | Default; `VatsState::world_time_scale` returns 1.0 |
| 1 | Target select | FO3/FNV 0.0 (paused); FO4 slowed (VATS-FO4-002) | Entered by the `V` key edge or `vats.enter` (`vats_mode_system`, `byroredux/src/systems/vats.rs`); publishes 0.0 for every game |
| 2 | Menu mode | **Phase 0: verify** what the original engine means by it | Variant exists, never constructed (`#[allow(dead_code)]` on `VatsMode`) |
| 3 | Scan mode | **Phase 0: verify** what the original engine means by it | Variant exists, never constructed |
| 4 | Playback | Camera-shot multipliers (VATS-PLAY-004) | Variant exists, never constructed: `vats_confirm` executes the queue instantly and returns to None |

Each state is reported to the user by `vats.status` (`mode=none`,
`target-select`, `menu`, `scan`, `playback`) and, once VATS-SCR-001 lands, to
scripts by `GetVATSMode`.

### 3.2 Inventory: what #6350 Phases 1–3 shipped

The inventory is the state at HEAD on 2026-11-01 (`66550cf8a`). It is
hand-written; Phase 0's `docs/engine/vats.md` replaces it with the design's
own status table, and this section then links there.

| # | Mechanism | Class | Today |
|---|---|---|---|
| 1 | `VatsModel` from load-order GMSTs | shipped | `VatsModel::with_gmst` (`crates/core/src/character/vats.rs`); built per load by `build_vats_model` (`byroredux/src/npc_spawn/stamps.rs`) from the profile row; FO3 and FNV rows in `crates/core/src/character/profile.rs`, every other game `vats: None` |
| 2 | Live AP pool with regen | shipped, unit unverified on FNV | `PoolRegenConfig::action_points` from `VatsModel::ap_regen_per_second`; the FNV per-frame reading is flagged unverified in the ruleset capture |
| 3 | Limb tables | shipped (data) | `BptdPart` decodes every `BPND` field (`crates/plugin/src/esm/records/misc/equipment.rs`); the scan reads name, target bone and to-hit chance only |
| 4 | Weapon AP cost and base VATS chance | shipped (data) | `ItemKind::Weapon { ap_cost, vats_base_to_hit, .. }`, FO3/FNV `DNAM` offsets 68 and 40 (`crates/plugin/src/esm/records/items.rs`) |
| 5 | Mode machine and world freeze | shipped | `vats_mode_system`, entry gates in `vats_entry_refusal`; the scale is applied in `byroredux/src/app_events.rs` |
| 6 | Targeting scan | shipped, partial | `vats_scan`: engage distance, bone positions, a three-ray occlusion fan (`PART_FAN_OFFSETS`), neutral skill 50 fallback; melee weapons priced like ranged ones |
| 7 | AP-gated queue and confirm | shipped | `vats_queue_part`, `vats_confirm`, `vats_cancel`; shots at one target fold into one `HitEvent` plus `HitShots` |
| 8 | Live gate | shipped | `docs/smoke-tests/vats-targeting.sh` (FNV, plumbing scope stated in its header) |
| 9 | To-hit ground truth | open (#6403) | `to_hit_chance_matches_recorded_in_game_percentages` is `#[ignore]`d with an empty `GROUND_TRUTH` |
| 10 | Per-limb damage, limb health, crippling | open | `HitEvent` (`crates/scripting/src/events.rs`) carries no body part; no system writes a limb-condition value |
| 11 | Playback and camera | open | `CAMS` / `CPTH` decode as EDID + FULL stubs only (`crates/plugin/src/esm/records/dispatch_misc_stub.rs`); `VatsClock` is written each frame and read by nothing |
| 12 | VATS menu | open | No menu; the loop is console-driven |
| 13 | Concentrated Fire | open (#6422) | `concentrated_shot: 1` hard-coded in `vats_scan` |
| 14 | FO4 lane | open | `CharacterRulesProfile::FALLOUT4` carries `vats: None`; FO4 has no model, no AP regen and no slow-motion scale |

---

## 4. Goals and non-goals

### 4.1 Goals

- **VATS-G-001 — Hit chances are the game's.** For each in-scope game, every
  captured ground-truth scenario is reproduced within one percentage point.
- **VATS-G-002 — The full loop is playable without the console.** On FNV and
  FO3 the player enters, selects, queues, confirms, watches playback and
  returns, with keyboard input and the vanilla menu, on one live gate each.
- **VATS-G-003 — Limbs cripple.** VATS hits damage the targeted limb's
  condition and a limb at zero reports crippled, on FNV and FO3.
- **VATS-G-004 — No dead tuning.** Every in-scope `VatsModel` field has a
  runtime reader or is listed as out of scope in `docs/engine/vats.md`.
- **VATS-G-005 — FO4 runs its own dialect.** FO4 enters VATS in slow motion
  with its own to-hit and AP rules, from its profile row.

### 4.2 Non-goals

- **VATS-NG-001.** The menu framework (MenuXml evaluation, Scaleform host
  methods, menu-stack policy, input routing) is the
  [Game UI Integration PRD](../prd/game-ui-integration.md)'s (M48; its
  `GUI-REG` registry and `GUI-XML-001`…`003` MenuXml menus). This PRD owns
  what the VATS menu shows and which VATS functions its controls call.
- **VATS-NG-002.** Evaluating perk entry points is the
  [Perk Entry Points PRD](../prd/perk-entry-points.md)'s (M24.2,
  `PERK-HOOK-*`). This PRD owns the VATS values those entry points modify and
  the VATS context they read (§5.7).
- **VATS-NG-003.** Fallout 76's VATS: it is real-time (PC Gamer,
  [*Fallout 76 will have 'real-time' VATS*](https://www.pcgamer.com/fallout-76-will-have-real-time-vats/),
  the headline; the article body was not retrievable on 2026-11-01, so no
  quotation is claimed),
  online-only, and the engine's FO76 support is parse-level
  ([game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail)).
- **VATS-NG-004.** Skyrim's kill cameras. The CK wiki's VATS-area condition
  functions select Skyrim's kill cameras (CK wiki *GetVATSBackAreaFree*: "Used
  to select the best available kill cam at the time of the kill event"), not
  a targeting mode; they belong to a combat-presentation feature, not here.
- **VATS-NG-005.** Gore rendering (meat caps, severed-limb meshes, debris).
  This PRD records *which* limb is gone (VATS-LIMB-008); drawing it is a
  renderer and NIFAL feature.
- **VATS-NG-006.** VATS for NPCs. AP is a player-only row on FO3/FNV (the
  GECK *Stats Tab — NPC* list has no AP entry, cited in the
  [ruleset capture](../engine/charal-fnv-fo3-ruleset.md#vats-tuning-action-points-regen-and-the-radiation-gauge--built-6350)).
- **VATS-NG-007.** Starfield ship-subsystem targeting: a ship-combat
  mechanic, not character VATS (§7.1).
- **VATS-NG-008.** VATS inside a co-op session. The
  [P2P Co-op PRD](p2p-coop.md)'s Q5 recommends that no peer stop or slow the
  shared clock while another is connected, so VATS would be unavailable in a
  session on FO3, FNV and FO4; this PRD anticipates that session gate on
  `vats_entry_refusal` and specifies single-player VATS only.

---

## 5. Requirements by area

### 5.1 To-hit chain (`VATS-TOHIT`)

- **VATS-TOHIT-001.** `VatsModel::to_hit_chance` MUST reproduce every FNV and
  FO3 ground-truth scenario recorded in Phase 0 within ±1 percentage point, in
  `to_hit_chance_matches_recorded_in_game_percentages` with its `#[ignore]`
  removed. Today: the test is ignored and `GROUND_TRUTH` is empty
  (`crates/core/src/character/vats.rs`, #6403), at HEAD on 2026-11-01.
- **VATS-TOHIT-002.** Each ground-truth row MUST be evaluated with the model
  of the game it was captured on (`VatsModel::FALLOUT_NV` for an FNV capture).
  Today: the test's capture procedure is written for FNV but the test builds
  `VatsModel::FALLOUT3.with_gmst(..)` (`crates/core/src/character/vats.rs:644`),
  and its step 4 still describes the 10mm as "base 65 %" although the decoder
  now reads the FNV 10mm's base chance as 15 (`items.rs`, #6449).
- **VATS-TOHIT-003.** The weapon record's Base VATS To-Hit Chance MUST enter
  the chain in the role Phase 0 establishes. Today, at HEAD on 2026-11-01,
  step 1 of `to_hit_chance` computes `weapon_base_to_hit × hit_chance_mult ×
  part_to_hit / 100` (`crates/core/src/character/vats.rs:351-353`), and
  `vats_scan` feeds it the base chance of the *player's* `EquippedWeapon`
  (`resolve_weapon`, `byroredux/src/systems/vats.rs:277-302`, used at
  `:427-431`). With the FNV 10mm's decoded 15 (#6449) and the shared
  `skill_factor` 1.0, that chain cannot show more than 15 × 1.5 = 22.5 % for
  that pistol at skill 100, range 0 and full visibility. The sources pull
  both ways. Reading A (the target's held weapon): the GECK wiki *Weapons*
  page calls the field "the chance to hit the weapon when it's held by
  another actor" and says it "is set based upon the size of the weapon", and
  *HighlightBodyPart* lists the weapon as body location 14. Reading B (the
  shooter's weapon): the same *Weapons* sentence says the value "is factored
  before distance, gun skill, and screen percentage", and *VATS Settings*
  says `fVATSHitChanceMult` "multiplies the accuracy of a weapon". **Phase 0:
  verify** which reading the game uses: at equal range, capture the chance
  shown on a body part with two weapons whose base values differ (10mm 15,
  minigun 45), and the chance shown on a target's held weapon for two held
  weapons with different base values. The reading whose prediction matches
  is the role.
- **VATS-TOHIT-004.** The screen-percentage term SHOULD be the part's
  projected, unoccluded share as `fVATSScreenPercentFactor` scales it ("the
  effect of limb screen percentage can be reduced or eliminated", GECK wiki
  *FVATSScreenPercentFactor*). Today: three rays at the bone ±18 units
  (`PART_FAN_OFFSETS`, `byroredux/src/systems/vats.rs`). The acceptance is a
  Phase 0 capture with a part half behind cover.
- **VATS-TOHIT-005.** A melee or unarmed weapon MUST make a target valid only
  within its reach × `fVATSMeleeReachMult`, capped at `fVATSMeleeMaxDistance`
  (GECK wiki *VATS Settings*). Today: the scan prices any equipped weapon out
  to `fVATSMaxEngageDistance`, and `melee_max_distance` / `melee_reach_mult`
  have no reader outside `vats.rs`, at HEAD on 2026-11-01.
- **VATS-TOHIT-006.** A crippled arm on the player SHOULD reduce the shown
  chance ("Weapon accuracy is reduced in V.A.T.S. by a crippled arm", Fallout
  wiki *Fallout 3 V.A.T.S.*). **Phase 0: verify** the magnitude by capture.
- **VATS-TOHIT-007.** Body parts that name the same limb-condition actor value
  SHOULD be offered as one VATS entry (GECK wiki *Body Part*: "Body Parts are
  'grouped' in VATS by Actor Value"). Today: `VatsState::targets` lists one
  row per `BPTD` part.

### 5.2 Action Points (`VATS-AP`)

- **VATS-AP-001.** A weapon that authors no override AP cost MUST be priced at
  the `fActionPoints*` standard of its animation type. Today: `resolve_weapon`
  prices every weapon whose `DNAM` offset 68 reads 0 at `attack_pistol`,
  whatever its animation type (`byroredux/src/systems/vats.rs:297-301`;
  the offset-68 field is authored only on override-flagged records,
  `crates/plugin/src/esm/records/items.rs:467-475`), at HEAD on 2026-11-01.
  **Phase 0: verify** the anim-type → standard mapping by reading the AP shown
  in vanilla FNV for one non-override weapon per animation type.
- **VATS-AP-002.** The FNV AP regeneration rate MUST use the unit Phase 0
  measures, pinned by a test asserting AP per second. Today: `0.06` is read as
  per frame at 60 Hz (3.6 AP/s), and the ruleset capture says no source states
  the unit. **Phase 0: verify** by timing AP refill from empty to full in
  vanilla FNV with a known maximum and comparing to 0.06/s and 3.6/s.
- **VATS-AP-003.** The queue SHOULD accept the non-attack VATS actions the
  engine prices (reload at `fActionPointsReload`; the GECK wiki *GetVATSValue*
  "VATS Action" query names "Attack, Reload, Throw Grenade, etc."). Today: the
  queue holds attacks only (`VatsTargetPart`).
- **VATS-AP-004.** While VATS is active, damage the player takes MUST be
  multiplied by `fVATSPlayerDamageMult` ("All damage taken by the player is
  multiplied by this value when VATS is active", GECK wiki *VATS Settings*;
  FNV authors 0.75, matching "receives only 75% of the normal damage", Fallout
  wiki [*Fallout: New Vegas V.A.T.S.*](https://fallout.wiki/wiki/Fallout:_New_Vegas_V.A.T.S.)).
  Today: `player_damage_mult` has no reader outside tests, at HEAD on 2026-11-01.
- **VATS-AP-005.** A VATS attack's critical chance SHOULD include
  `fVATSCriticalChanceBonus` (GECK wiki *VATS Settings*: "Used in the Critical
  Hit Chance Formula") once the combat funnel models critical hits. Today:
  `byroredux/src/combat.rs` has no critical-hit path (no `crit` match), so this
  requirement waits on that system and Phase 1 records the dependency.

### 5.3 Limb damage and limb health (`VATS-LIMB`)

- **VATS-LIMB-001.** Every actor whose limb table resolves MUST carry the
  limb-condition actor values its body parts name, at 100. FO3/FNV: GECK
  wiki *Stats List* gives each of the seven condition values (Perception …
  Brain) "Base value is 100"; Fallout 4: FO4 CK wiki *OnCripple - Actor*,
  "The Actor Value is always initialized to 100". Today: no
  system stamps them; they appear only in name tables
  (`crates/scripting/src/obscript_actor_values.rs`).
- **VATS-LIMB-002.** A VATS hit MUST reach the combat funnel with the body part
  it struck. Today: `HitEvent` has no part field and `vats_confirm` folds all
  shots at one target into one event (`crates/scripting/src/events.rs`,
  `byroredux/src/systems/vats.rs`).
- **VATS-LIMB-003.** Health damage from a part hit MUST be multiplied by that
  part's `BPND` damage multiplier (GECK wiki *BodyPartData*: "The damage
  multiplier used for damage done to this body part"). Today: `vats_confirm`
  applies only the weapon's VATS damage multiplier.
- **VATS-LIMB-004.** A part hit MUST reduce the part's limb condition by the
  amount the Phase 0 formula gives from the hit damage, the weapon's Limb Dmg
  Mult (GECK wiki *Weapons*) and the part's Health Percent ("the percentage of
  the actor's total health at which that body part becomes crippled", GECK wiki
  *BodyPartData*). **Phase 0: verify** the formula: record limb condition
  before and after single hits of known damage on a vanilla NPC.
- **VATS-LIMB-005.** A limb whose condition reaches zero or below MUST report
  crippled in the new `vats.limbs <entity>` console command, and MUST report
  healthy again when the value rises above zero (FO3/FNV: GECK wiki
  *GetVATSValue*, *Cripple Part*: "condition changes from > 0 to <= 0";
  FO4: FO4 CK wiki *OnCripple - Actor*, "falls to or below 0 … Should the
  Actor Value later rise above 0, the limb becomes un-crippled").
- **VATS-LIMB-006.** A crippled limb SHOULD apply its gameplay effect (legs:
  movement speed; arms: accuracy, per the same Fallout wiki page), with
  magnitudes from **Phase 0: verify** captures.
- **VATS-LIMB-007.** Hits outside VATS SHOULD damage limbs through the same
  path (FO4 CK wiki *OnCripple - Actor*: reduced "in normal combat or VATS").
  **Phase 0: verify** the same rule on FO3/FNV.
- **VATS-LIMB-008.** A severed or exploded part SHOULD be recorded per actor
  so `IsLimbGone` and `GetKillingBlowLimb` (GECK wiki, body locations −1…13)
  answer from it. Today: `IsLimbGone` always answers 0 and
  `GetKillingBlowLimb` answers −1 for a living actor and declines on a corpse
  (`crates/scripting/src/obscript_more/state.rs:537-548`, "the engine keeps
  no dismemberment"), at HEAD on 2026-11-01.
- **VATS-LIMB-009.** Limb conditions MUST survive save and load. Today:
  `ActorValues` is a saved component (`byroredux/src/save_io/registry.rs`), so
  values stamped there persist; the dismemberment record of VATS-LIMB-008 is
  new saved state and needs a `FORMAT_MAJOR` bump, which is a host contract
  change (README T2) this PRD declares.

### 5.4 Playback and camera (`VATS-PLAY`)

- **VATS-PLAY-001.** Confirm MUST enter `Playback` (value 4) and execute the
  queue in order, returning to `None` after the last attack. Today:
  `vats_confirm` writes every `HitEvent` and calls `exit_all` in the same call.
- **VATS-PLAY-002.** Each attack MUST play the player's attack animation and
  apply its rolled outcome when the attack lands, so a rolled miss never
  damages the target.
- **VATS-PLAY-003.** `CAMS` and `CPTH` MUST decode the fields the GECK wiki
  *CameraShot* and *Camera Paths* pages list (model path, location, target,
  action, flags, time multipliers, min/max time, image-space modifier; shot
  list and conditions). Today: both are EDID + FULL stubs
  (`dispatch_misc_stub.rs`), at HEAD on 2026-11-01.
- **VATS-PLAY-004.** Playback MUST pick the camera path the GECK rule picks:
  "the first, deepest path whose conditions are met" (*Camera Paths*), with
  `GetVATSValue` available to those conditions (VATS-PERK-002).
- **VATS-PLAY-005.** During playback the world, player and target time scales
  MUST follow the active shot's Global / Player / Target multipliers (GECK wiki
  *CameraShot*). Today: `VatsState::world_time_scale` returns only 0.0 or 1.0.
- **VATS-PLAY-006.** The camera and the menu MUST advance on `VatsClock` while
  the world is paused. Today: `VatsClock` is set in `app_events.rs` and has no
  reader.
- **VATS-PLAY-007.** A shot whose camera path resolves no shot SHOULD fall back
  to the first-person view the *CameraShot* "First Person Camera" option
  describes. **Phase 0: verify** the vanilla fallback.
- **VATS-PLAY-008.** An automatic weapon's VATS attack SHOULD fire for
  `fVATSShotBurstTime` (`fVATSShotLongBurstTime` with Long Bursts set), which
  "determines the number of shots" (GECK wiki *VATS Settings*).
- **VATS-PLAY-009.** `fVATSPlaybackDelay` MAY gap consecutive attacks once
  **Phase 0: verify** establishes its meaning (the GECK page gives no
  description).

### 5.5 VATS menu (`VATS-UI`)

- **VATS-UI-001.** On FO3 and FNV the VATS menu MUST be the game's own
  `vats_menu.xml` rendered through the MenuXml route. The file is in
  `Fallout - Misc.bsa` of both installs (read 2026-11-01 with
  `grep -a -o 'vats_menu\.xml'` over the archive).
- **VATS-UI-002.** The menu's part list, hit percentages, AP costs and queue
  MUST equal what `vats.status` reports in the same frame; the menu reads
  `VatsState` and `VatsModel` only.
- **VATS-UI-003.** Target cycling, part selection, queue, confirm and cancel
  MUST call `vats_queue_part`, `vats_confirm` and `vats_cancel`, the functions
  the console already calls. Today: only the enter/exit key (`V`) is bound
  (`byroredux/src/interaction.rs`).
- **VATS-UI-004.** The selected part SHOULD be highlighted on the target
  through the same highlight state `HighlightBodyPart` sets. The GECK wiki
  *HighlightBodyPart* page says only that "Entering and leaving VATS will
  clear all highlighted body parts" and that one part is highlighted at a
  time; **Phase 0: verify** that vanilla VATS highlights the hovered part by
  capturing target select in vanilla FNV.
- **VATS-UI-005.** On FO4 the menu MUST be `Interface/VATSMenu.swf` from
  `Fallout4 - Interface.ba2` (read 2026-11-01 the same way) through the
  Scaleform host.
- **VATS-UI-006.** Per-game menu differences MUST be expressed in the menu
  profile the UI layer already keys per game, never as a game test inside the
  VATS systems.

### 5.6 State, rolls and saves (`VATS-MODE`)

- **VATS-MODE-001.** The die-roll state MUST follow decision Q1. Today:
  `VatsState::default()` is inserted at boot (`byroredux/src/boot/world.rs`)
  with `roll_state` 0 and `VatsState` is not saved
  (`registry_completeness_tests.rs` lists it transient), so every session's
  first VATS rolls are the same sequence.
- **VATS-MODE-002.** A save request while VATS is active MUST be refused with a
  reported reason, or handled as the vanilla game handles it once **Phase 0:
  verify** records that (attempt a quicksave in vanilla FNV target select and
  playback). Today: nothing in `byroredux/src/save_io/` reads `VatsState`.
- **VATS-MODE-003.** Leaving VATS by any path other than confirm MUST leave the
  AP pool unchanged, and a test MUST pin it for cancel, death and cell change.
- **VATS-MODE-004.** The meanings of `VATS_MENU_MODE` and `VATS_SCAN_MODE`
  MUST be settled (**Phase 0: verify**, by printing `GetVATSMode` from a
  vanilla script while stepping through VATS) before either is constructed.

### 5.7 Perk hooks (`VATS-PERK`)

- **VATS-PERK-001.** The repeat count `PERK-HOOK-003` of the Perk Entry
  Points PRD passes as `concentrated_shot` MUST count queued shots at the
  same VATS entry, where an entry is one limb-condition group
  (VATS-TOHIT-007). `PERK-HOOK-003` owns when the count applies (FO3/FNV
  *Has Concentrated Fire*, code 33 in GECK wiki *GetNthPerkEntryFunction*,
  composed non-zero); this ID owns only what "same part" means. The GECK
  wiki *VATS Settings* example speaks of "a limb", not of a group;
  **Phase 0: verify** by queueing two shots at each of two parts that share
  a condition value (a creature's paired legs) with the perk taken and
  reading the second shot's chance. Today: `concentrated_shot: 1` is
  hard-coded (`byroredux/src/systems/vats.rs:443`, #6422).
- **VATS-PERK-002.** VATS MUST expose its per-action context (weapon, target,
  targeted part, action, distance, success, critical, fatal) for the
  `GetVATSValue` queries the GECK wiki lists, for camera-path conditions and
  for the Calculate To Hit Chance entry point. Today: no `GetVATSValue` arm
  exists in the condition catalog (`crates/scripting/src/condition/`).
- **VATS-PERK-003.** When the Perk Entry Points PRD's Q6 composes the
  "Calculate To Hit Chance" entry point (code 8; GECK wiki *Entry Point*:
  "Run when the player queues a shot in VATS", with "NOTE: Broken as of
  1.1.0035 patch"), its result MUST be the chance `vats.status` shows for
  the queued shot. Whether it composes on FO3 at all is that PRD's Q6, not
  this one's.
- **VATS-PERK-004.** The AP cost `vats.status` shows MUST include the FNV
  "Action Point Cost" entry point (code 40) in the form `PERK-HOOK-005`'s
  verify item settles. "Calculate Weapon Attack AP Cost" (code 3) does not
  price VATS: GECK wiki *Entry Point* runs it "when the player attacks
  outside of VATS", so it is outside this PRD.
- **VATS-PERK-005.** The "Action Point Regen" entry point (code 39) SHOULD
  modify the regen rate the AP lane applies, in the form `PERK-HOOK-005`'s
  verify item settles.
- **VATS-PERK-006.** The "Adjust Limb Damage" entry point (code 6) SHOULD
  modify the limb-condition loss on the *player's* own limbs, the subject
  GECK wiki *Entry Point* gives it ("Run when the player's limbs are damaged
  by something"), through the limb path of VATS-LIMB-004 and VATS-LIMB-007.
  It does not modify the limb damage the player's VATS attacks deal.

All of them compose through the Perk Entry Points PRD's evaluator, which
also owns *Player Kill AP Reward* (`PERK-HOOK-004`); this PRD adds no
perk-specific branch.

### 5.8 Script surface (`VATS-SCR`)

- **VATS-SCR-001.** The `GetVATSMode` condition function MUST return
  `VatsMode::as_u32()`. Today: `as_u32` is `#[allow(dead_code)]` and no
  condition arm reads it.
- **VATS-SCR-002.** FO4's `Game.IsVATSPlaybackActive()` MUST return true
  exactly while the mode is `Playback` (FO4 CK wiki *IsVATSPlaybackActive -
  Game*).
- **VATS-SCR-003.** A destructible reference flagged by `SetVATSTarget` SHOULD
  appear in the scan. Today: the flag is stored
  (`crates/scripting/src/obscript_more/state.rs`) and nothing reads it.

### 5.9 Fallout 4 lane (`VATS-FO4`)

- **VATS-FO4-001.** `CharacterRulesProfile::FALLOUT4` MUST carry a VATS row
  built from Fallout4.esm's own settings. Today: `vats: None`
  (`crates/core/src/character/profile.rs`), so FO4 builds no model and no AP
  regen lane.
- **VATS-FO4-002.** FO4 target select MUST publish `fVATSTimeMultTargetSelect`
  as the world time scale instead of 0.0. Fallout4.esm authors 0.04 (read
  2026-11-01: the float after `DATA` following the setting's EDID). **Phase 0:
  verify** that this setting is the selection slow-down the FO4 CK wiki
  describes (*OnTimer - ScriptObject*, *Wait - Utility*).
- **VATS-FO4-003.** The FO4 to-hit chance MUST be computed from the
  `fVATSChance*` family Fallout4.esm authors (`fVATSChanceWeaponBaseMultiplier`,
  `…PerceptionMultiplier`, `…DistanceMultiplier`, `…BodyPartMultiplier` and
  siblings), selected by the profile row, with **Phase 0: verify** of their
  composition by FO4 ground-truth captures.
- **VATS-FO4-004.** FO4 AP MUST regenerate at the rate the
  [CHARAL FO4 ruleset § Action Points](../engine/charal-fo4-ruleset.md#action-points--locked)
  owns, built from Fallout4.esm's own settings. The design's rate comes from
  the Fallout wiki *Agility (Fallout 4)* page, while Fallout4.esm authors
  `fActionPointsRestoreRate` = 4.0 (read 2026-11-01: the float after `DATA`
  following the setting's EDID). **Phase 0: verify** how the authored
  setting and Agility combine, by timing AP refill from empty in vanilla FO4
  at two Agility values; the design is then corrected if it disagrees.
  Today: the FO4 AP maximum row exists (`fallout4_ruleset`,
  `crates/core/src/character/fallout.rs`); no regen lane runs.
- **VATS-FO4-005.** FO4 weapon records MUST decode the VATS fields the scan
  reads. Today: `ap_cost` reads FO4 `DNAM` offset 112 and `vats_base_to_hit`
  is 0.0 on FO4 (`items.rs`). **Phase 0: verify** the FO4 layout against
  xEdit `wbDefinitionsFO4.pas`.
- **VATS-FO4-006.** The FO4 critical meter (`fVATSCriticalChargeBase`,
  `fVATSCriticalChargeMult`) follows decision Q2.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Die rolls from the game's RNG; reseed behaviour **Phase 0: verify** | Session-deterministic LCG in `VatsState` | — | Identical first rolls every session, so a reload replays or a new session repeats outcomes → VATS-MODE-001, Q1 |
| 2 | Screen percentage of the limb (GECK *FVATSScreenPercentFactor*) | Three-ray occlusion fan | — | Partly covered parts priced high or low → VATS-TOHIT-004 |
| 3 | `fActionPointsRestoreRate` in the game's own unit | FNV value read per frame at 60 Hz | — | FNV regen off by up to 60× → VATS-AP-002 |
| 4 | Every vanilla actor has a limb table | A modded actor with no resolvable table gets one "Body" part | — | Modded creatures lose limb targeting silently → VATS-LIMB-001 reports actors without a table in `vats.limbs` |
| 5 | Mode state is not part of the save (assumed; save-in-VATS behaviour **Phase 0: verify**) | `VatsState` transient | — | A save taken mid-VATS restores outside VATS with AP already charged or a queue lost → VATS-MODE-002, VATS-MODE-003 |
| 6 | One hit per shot | Shots at one target folded into one `HitEvent` + `HitShots` | — | Per-part damage and per-shot effects lost → VATS-LIMB-002 |
| 7 | FO3/FNV pause, FO4 slow-down | One time-scale axis per profile row | — | None while the scale comes from the row (VATS-FO4-002); a hard-coded scale would be the per-game branch §1 forbids |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow the engine tiers in
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail).

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | n/a | No VATS: TES IV has no such mode; the Oblivion CS wiki dump has no VATS page or function (`grep -ril vats cs-uesp-wiki` finds none), and the profile row is `vats: None` |
| Fallout 3 | yes | Wave 1. Same engine dialect as FNV with its own GMST values (`VatsModel::FALLOUT3`); paused target select (Fallout wiki *Fallout 3 V.A.T.S.*) |
| Fallout: New Vegas | yes | Wave 1, reference title and live gate (`vats-targeting.sh`) |
| Skyrim (LE / SE / AE) | n/a | No player VATS: the CK wiki *Weapons* page says "Base VATS To-Hit Chance: Not applicable"; the VATS-area functions there select kill cameras (VATS-NG-004) |
| Fallout 4 | yes | Wave 2: slow-motion target select, its own `fVATSChance*` chain, AP regen, `VATSMenu.swf` (§5.9) |
| Fallout 76 | out | Real-time VATS in an online-only game, parse-level engine support (VATS-NG-003) |
| Starfield | n/a | No character VATS; its ship-subsystem targeting is a separate mechanic (VATS-NG-007). Evidence today: `Starfield - Interface.ba2` names no VATS `.swf` or `.gfx` movie (`grep -a -o -i` for `vats…\.(swf\|gfx)`, read 2026-11-01). The GMST list does not settle it: Starfield.esm still carries 50 `fVATS*` names, the FO4 `fVATSChance*` family among them (read the same day). **Phase 0: verify** against a primary source: a Bethesda statement, or an in-game capture of the vanilla controls menu showing no VATS binding |

### 7.2 Acceptance

A scenario passes when its observable holds on vanilla data from the user's
own install (`BYROREDUX_FNV_DATA` and siblings); no game file is checked in.

| # | Scenario | Data | Pass rule |
|---|---|---|---|
| 1 | `cargo test -p byroredux-core --features inspect to_hit_chance_matches_recorded_in_game_percentages` | none (constants captured in Phase 0) | Test runs (not ignored) and passes; FNV and FO3 rows present |
| 2 | `docs/smoke-tests/vats-targeting.sh` | FNV | Passes unchanged |
| 3 | `vats-targeting.sh` on FO3 (an FO3 cell mode added in Phase 1) | FO3 | Passes |
| 4 | New script `vats-limbs.sh`: `vats.enter`, three `vats.queue Head`, `vats.confirm`, then `vats.limbs <entity>` | FNV, FO3 | Head condition below its pre-shot value after landed shots; `crippled` reported once it reaches 0; F5/F9 round trip keeps the value |
| 5 | New script `vats-playback.sh`: `vats.confirm` then `vats.status` polled | FNV | `mode=playback` observed with the active shot's time scale, then `mode=none`; AP charged once |
| 6 | New script `vats-menu.sh`: `ui.open` the VATS menu (`GUI-OPS-005`), read its part rows, `ui.close … cancel` | FNV, FO3 | The menu's part rows equal `vats.status` rows in the same frame; after `cancel`, `vats.status` reports `mode=none` and AP is unchanged |
| 7 | New script `vats-fo4.sh`: `--game fo4` cell, `vats.enter`, `vats.status` | FO4 | `world_time_scale=0.04` (or the Phase 0 value); FO4 ground-truth rows pass in scenario 1 |
| 8 | `cargo test -p byroredux --bin byroredux vats` (the existing scan and confirm tests plus the new cancel/death/cell-change AP tests of VATS-MODE-003) | none | All pass |

---

## 8. Metrics

The numbers live in the #6350 row of
[ROADMAP Tier 7](../../ROADMAP.md#tier-7--deep-gameplay-systems) and, for the
live gates, in [docs/smoke-tests/README.md](../smoke-tests/README.md).

| Metric | Definition | Granularity |
|---|---|---|
| Ground-truth agreement | captured scenarios reproduced within ±1 pp ÷ captured scenarios | per game |
| Tuning coverage | in-scope `VatsModel` fields with a runtime reader ÷ in-scope fields | per profile row |
| Limb-table resolution | scanned actors with a resolved `BPTD` ÷ scanned actors | per game, per smoke cell |
| VATS gate pass rate | VATS smoke scripts passing ÷ VATS smoke scripts applicable | per game, per phase |

---

## 9. Delivery sequence

Phases are ordered by what they unblock: correct numbers first, then the
damage they cause, then what the player sees. Depends on M42 and CHARAL
(as the ROADMAP row says), plus the two sibling PRDs named per phase.

**Phase 0 — Design, captures and verify list (no engine code).** Write
`docs/engine/vats.md`, the runtime design that owns the mode machine, scan,
queue, limb damage, playback and camera, with its own status table replacing
§3.2. Record the ground-truth scenarios in vanilla FNV and FO3 (distance,
part, skill, partial cover, crippled arm, held weapon) and, for Phase 6, FO4.
Close every **Phase 0: verify** item: the base-chance role (VATS-TOHIT-003),
partial-cover and crippled-arm captures (VATS-TOHIT-004/006), the anim-type
AP mapping and the FNV regen unit (VATS-AP-001/002), the limb damage formula
and non-VATS rule (VATS-LIMB-004/007), the Concentrated Fire grouping and the
hover highlight (VATS-PERK-001, VATS-UI-004), the playback
fallback and delay (VATS-PLAY-007/009), save-in-VATS and the two unnamed modes
(VATS-MODE-002/004), the FO4 time scale, chance chain, AP regen and `DNAM`
layout (VATS-FO4-002/003/004/005), and Starfield's disposition. Resolve §10. Exit:
`docs/engine/vats.md` is checked in with a capture table for FNV and FO3, and
every verify item is answered there or filed as an issue.

**Phase 1 — Numbers the game agrees with (FO3/FNV).** VATS-TOHIT-001…007,
VATS-AP-001…004, VATS-MODE-001…004, VATS-SCR-001. Exit: acceptance scenarios
1, 2, 3 and 8 pass; ground-truth agreement is 100 % for FNV and FO3.

**Phase 2 — Limbs.** VATS-LIMB-001…009 (with the `FORMAT_MAJOR` bump of
VATS-LIMB-009). Exit: scenario 4 passes on FNV and FO3.

**Phase 3 — Playback and camera.** VATS-PLAY-001…009, VATS-PERK-002's
`GetVATSValue` queries for camera paths. Exit: scenario 5 passes on FNV.

**Phase 4 — The menu.** VATS-UI-001…004 and VATS-UI-006 on the MenuXml
route. Depends on the [Game UI Integration PRD](game-ui-integration.md)
(M48): its Phase 1 (menu registry `GUI-REG-001`…`009`, including the
input-owner rule `GUI-REG-009`, and the `ui.open` / `ui.close` driver
`GUI-OPS-005`) and its Phase 3 (MenuXml menu loading, input and engine
values `GUI-XML-001`…`003` for FNV and, through the shared Fallout
`MenuProfile`, FO3). Exit: scenario 6 passes on FNV and FO3.

**Phase 5 — Perk hooks.** VATS-PERK-001 and VATS-PERK-003…006 through the
evaluator of the [Perk Entry Points PRD](../prd/perk-entry-points.md)
(M24.2, `PERK-HOOK-003`, `PERK-HOOK-005`, and its Q6 for code 8). Exit: a
test with vanilla Concentrated Fire's own `PERK` record reproduces the GECK
wiki's 63 → 68 → 73 example, and one test per entry point of VATS-PERK-003
(if Q6 composes it), -004, -005 and -006 passes.

**Phase 6 — Fallout 4.** VATS-FO4-001…006, VATS-UI-005, VATS-SCR-002.
Exit: scenario 7 passes and ground-truth agreement is 100 % for FO4.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — Die-roll state across saves.** Recommended: save `roll_state` with
   the game (a `FORMAT_MAJOR` bump), seeded once per new game from the OS, so
   a reload replays the same outcomes and tests stay replayable. Alternative
   A: reseed from the OS at every boot and never save it; reloads can then
   re-roll a missed queue. Alternative B: keep today's fixed seed; every
   session repeats the same first rolls. Phase 0's vanilla reseed check
   informs this but does not bind it.
2. **Q2 — FO4 wave scope.** Recommended: slow-motion target select, the FO4
   chance chain, AP regen and `VATSMenu.swf`; the critical meter (banked
   criticals) waits for a combat critical-hit system (VATS-AP-005).
   Alternative: include the critical meter in Phase 6, which pulls a combat
   critical system into this PRD.
3. **Q3 — Whether Phase 4 waits for M48.** Which surface is the product
   (the game's own document or a native page) is the Game UI Integration
   PRD's Q1, not reopened here; VATS-UI-001 follows that PRD's recommended
   answer. This question is only sequencing. Recommended: Phase 4 waits for
   the Game UI PRD's Phase 1 (`GUI-REG`, `GUI-OPS-005`) and Phase 3
   (`GUI-XML-001`…`003`) and ships `vats_menu.xml` through
   MenuXml, consistent with the shipped FO3/FNV HUD route. Alternative: a
   native S3 page first (as the dialogue page shipped), which unblocks
   Phase 4 before M48's menu-stack work but adds a second VATS menu to retire
   later, and VATS-UI-001 then waits for a later phase.
4. **Q4 — Where Concentrated Fire is checked (withdrawn, rev 2).** Settled
   by the Perk Entry Points PRD's `PERK-HOOK-003`, which routes it through
   the *Has Concentrated Fire* entry point; VATS-PERK-001 now owns only the
   grouping rule. Recommended: nothing to decide here; the owner decides it
   in that PRD's approval round.
5. **Q5 — Playback camera depth.** Recommended: the authored `CPTH`/`CAMS`
   selection (VATS-PLAY-003/004) in Phase 3. Alternative: first-person
   playback only in Phase 3 with authored cameras later; quicker, but
   VATS-PLAY-005's per-shot time scales then have no source and playback
   looks unlike the game.

---

## 11. Document map

| Need | Go to |
|---|---|
| VATS settings, sourcing, FNV re-tunes, regen-unit caveat | [charal-fnv-fo3-ruleset.md § VATS tuning](../engine/charal-fnv-fo3-ruleset.md#vats-tuning-action-points-regen-and-the-radiation-gauge--built-6350) |
| Pool regeneration mechanism | [charal.md §4.7](../engine/charal.md#47-poolregenaccumulator--new-built-pool-regeneration-the-layers-only-fixed-step-system) |
| FO4 Action Points formulas | [charal-fo4-ruleset.md § Action Points](../engine/charal-fo4-ruleset.md#action-points--locked) |
| MenuXml and Scaleform menu routes | [ui.md](../engine/ui.md) |
| Per-game engine tiers | [game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail) |
| `vats.*` console commands | [debug-cli.md](../engine/debug-cli.md) |
| VATS runtime design | `docs/engine/vats.md` (written in Phase 0) |
| Menu framework the VATS menu runs on | [game-ui-integration.md](game-ui-integration.md) |
| Co-op session gate on VATS | [p2p-coop.md](p2p-coop.md) (its Q5) |
| Perk entry-point hooks | [perk-entry-points.md](../prd/perk-entry-points.md) |
| Live gates index | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: perk entry-point roles (codes 3, 6, 8) against GECK *Entry Point*; VATS-TOHIT-003 code path, arithmetic cap and both source readings; FO4 AP regen vs `fActionPointsRestoreRate`; VATS-LIMB-001/005 sourced from GECK *Stats List* / *GetVATSValue*; VATS-UI-004 citation; Starfield evidence; HEAD precision (VATS-AP-001, VATS-LIMB-008); Q3 sequencing only, Q4 withdrawn to `PERK-HOOK-003`; Phase 4 depends on Game UI Phases 1 and 3, scenario 6 driven by `GUI-OPS-005`; VATS-NG-008 co-op gate (P2P Co-op Q5) | claude: revise |
