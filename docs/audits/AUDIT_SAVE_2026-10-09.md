# Save / Load Subsystem Audit (M45 + M45.1) — 2026-10-09

**HEAD**: 3bcf6c8e8 · **Baseline**: `docs/audits/AUDIT_SAVE_2026-10-08.md` (HEAD `00f580e09`) · **Audited**: Dimensions 1, 2, 4, 5 (delta-first, with priority on the streaming area) · **Unchanged since baseline (skimmed)**: Dimension 3 (Container & Disk). `crates/save/src/{disk,registry,driver,validate}.rs` and `crates/core/src/atomic_file.rs` have no commits; `snapshot.rs` changed only the `FORMAT_MAJOR` constant and its doc. The guard tests were re-run.

This run is part of `/audit-suite --preset streaming-deep`. All five dimensions were analysed synchronously, with no sub-agents; scratch notes are in `/tmp/audit/save/dim_{1..5}.md`.

The window holds 81 commits. Sixteen of them touch save, cell-loader, NPC-spawn or streaming paths. The save-relevant changes:
- `FORMAT_MAJOR` went from 33 to 35 in two separate bumps:
  - #5412 (`a614eb273`): `ActorValue.set_override`.
  - #5394 (`03e51dfad`): a new saved resource, `StoryEventAliasFill`.
- The prior HIGH, SAVE-D5-2026-10-08-01, was fixed by #5379 (`203be9ed4`). #5384 (`faf8e5682`) then reworked the re-adoption code.
- #5391 (`42aab4c09`) added `EditorPlacement`, plus an `EatSleepLocation` inside the Eat/Sleep behaviours.

Dedup sources:
- `/tmp/audit/issues.json` (147 open issues).
- Closed-issue searches: `set_override`, `vitals_snapshot`, `SetBase HUD`, `purge subtree`, `orphan Parent save`, `CinematicReAdoption`.
- Today's sibling reports (`AUDIT_CONCURRENCY_2026-10-09.md`, `AUDIT_EXTERIOR_2026-10-09.md`, `AUDIT_PERFORMANCE_2026-10-09.md`).

Already reported this suite and only cross-referenced here: CONC-D5-2026-10-09-01, CONC-D7-2026-10-09-01, PERF-D1-2026-10-09-01.

## Executive Summary

| `lib.rs` / `snapshot.rs` design claim | Status |
|---|---|
| Full ECS snapshot of game state | **CODE-CONFIRMED.** `StoryEventAliasFill` is now registered (#5394 fixes GAME-D7-2026-10-08-01). The two new impls, `VoiceSoundCache` and `EditorPlacement`, are allowlisted with accurate reasons. The stale Eat/Sleep `Seated` reasons are unchanged (#5458, still open). |
| Versioned container + CRC32 over the payload | **CODE-CONFIRMED.** The header gates are unchanged, and 41 + 15 crate tests pass. |
| Atomic write (tmp → fsync → re-read+verify → rename → dir-fsync) | **CODE-CONFIRMED.** No changes. |
| Ring never clobbers the last good save | **CODE-CONFIRMED.** |
| Validation gate refuses to persist a poisoned save | **CODE-CONFIRMED as a gate. DRIFTED in effect:** after a session replace that purges a pending convoy, the gate refuses *every* later save, because the purge leaves an orphaned hierarchy that nothing ever repairs (SAVE-D4-2026-10-09-01). |
| `FORMAT_MAJOR` bump is the only schema-evolution path | **CODE-CONFIRMED for this window.** Both bumps refreshed `BASELINE_SHAPE_FINGERPRINT` and `BASELINE_MAJOR` in the same commit, with no `serde(default)`. The ledger still lacks a v32→v33 entry (#5459, open), and the SM-type guard blind spot is unchanged (#5403, open). |
| Load runs off-frame; additive overlay + reconcilers | **CODE-CONFIRMED.** #5379 no longer lets re-adoption cell-own the player. The drain sequence is unchanged; the double `reconcile_worn_gear` is still present (#5460). |
| Reproducible CRC at equal state | The doc disclaims it. `StoryEventAliasFill` is one more `HashMap` resource. Note only. |

**Findings: 2 NEW. 0 CRITICAL, 0 HIGH, 2 MEDIUM, 0 LOW.**

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 2 | SAVE-D4-2026-10-09-01, SAVE-D2-2026-10-09-01 |
| LOW | 0 | — |

**Prior-cycle findings:**
- **SAVE-D5-2026-10-08-01 (#5379): FIXED and verified.**
  - `release_finished_tethers` collects the player's `Children` closure and keeps every member out of `unplaced` (`byroredux/src/systems/cinematic.rs:504-563`).
  - `retry_cinematic_readoption` consumes player-subtree entries without stamping them (`:647-663`). #5384's subtree stamping also skips player-subtree members (`:694`).
  - `purge_cinematic_retention_state` despawns and clears `CinematicReAdoption.pending` (`byroredux/src/cell_loader/unload.rs:90-113`).
  - The allowlist reason was rewritten.
  - Residual: since #5384 the purge removes roots only. The concurrency audit filed that as CONC-D5-2026-10-09-01 (ghost geometry, GPU refcounts, collider). The save-gate consequence is SAVE-D4-2026-10-09-01 below.
- **SAVE-D2-2026-10-08-01 (#5403), SAVE-D2-2026-10-08-02 (#5459), SAVE-D1-2026-10-08-01 (#5458), SAVE-D5-2026-10-08-02 (#5460): all still open and still present.**
  - #5459: the ledger now jumps from v31→v32 straight to v33→v34.
  - #5458: #5391 rewrote the `EatSleepState` row but kept the false clause "the arrival it guards is Seated, which is registered".
  - #5460: `save_io.rs:1939-1940`.
- **#5059, #5060: open and unchanged.**
- **GAME-D7-2026-10-08-01 (#5394) and GAME-D5-2026-10-08-03 (#5391): fixed.** Their save-side halves were verified in Dim 1.

## Data-Loss Class Matrix

| Finding | Class | Dim | Severity | Status |
|---|---|---|---|---|
| SAVE-D4-2026-10-09-01: the session-replace purge orphans the pending convoy's subtree; `validate_world` then refuses every later save for the life of the process | irrecoverable-write (save-refusal soft-lock; no on-disk save is harmed) | 4 (root cause in 5) | MEDIUM | NEW (save-side consequence of CONC-D5-2026-10-09-01's root cause) |
| SAVE-D2-2026-10-09-01: #5412's `set_override` layer is missed by three hand-composed readers (HUD bar fraction, debug vitals, SDK projection) | none (read-side display/API) | 2 (cross-domain) | MEDIUM | NEW (regression introduced by the #5412 fix) |
| #5403 / #5459 / #5458 / #5460 / #5059 / #5060 | see the baseline report | 1, 2, 5 | MEDIUM / LOW | Existing |

## Completeness Ledger (delta from baseline; full table in `AUDIT_SAVE_2026-09-11.md`)

| Column / type | Kind | Saved | Overlaid | Notes |
|---|---|---|---|---|
| `StoryEventAliasFill` (#5394) | Resource, `HashMap<QuestFormId, StoryEventFill>` | yes (v35) | n/a (wholesale) | `slots` (session `EntityId`s) are `serde(skip)`. `reference_forms` are global REFR FormIDs or `0x14`, with the same load-order posture as `QuestStageState`. Installed insert-if-absent (`story_manager.rs:421`). Pinned by `story_event_alias_fill_survives_save_load_as_reference_forms`. Visible to the shape guard: the save derive is the nearest attribute. |
| `ActorValue.set_override` (#5412) | field of `ActorValues` | yes (v34) | yes | Round-trips through the overlay and through the `ReferenceState` park, since the whole `ActorValues` is cloned. See SAVE-D2-2026-10-09-01 for the read side. |
| `EditorPlacement` (#5391) | Component | no (allowlisted) | n/a | The reason is accurate. `spawn_placement_root` (`npc_spawn/resumable/mod.rs:369-379`) stamps it, and both NPC spawn paths run through that function. Only the player body skips it. Nothing mutates it. |
| `EatBehavior` / `SleepBehavior` (`location: EatSleepLocation`) | Component | no | n/a | Re-derived from the package. The `Seated` clause in the reason is still false: #5458. |
| `VoiceSoundCache` (#5382) | Resource | no (allowlisted) | n/a | The reason is accurate (a decode LRU). |
| `RaceSpells` on the player (#5413) | Component | no | n/a | The reason says "re-stamped at every spawn/reload", but the process-lifetime player is stamped only once. This is harmless: there is no production writer and no runtime race change (`rg SetRace` finds nothing). Note only. |

Two-list drift: none. `MUTABLE_DELTA_COLUMNS` is unchanged, and the only new registration is a resource.

Streaming-area check, run because the suite focuses on this area:
- `cell_loader/stream_snapshot.rs`, `cell_loader/reference_state.rs` and `npc_spawn/loot_appearance.rs` have no commits since the baseline.
- A resumable NPC that is still mid-job at save time carries no `FormIdComponent`. `stamp_quest_reference` stamps it only at job completion, together with `reference_state::restore` (`cell_loader/references/synth_child.rs:53-98`).
- So the remap skips its rows, and its parked row stays in the saved `PersistentReferenceStates` to be consumed on respawn. There is no clobber.
- None of the in-area commits adds runtime state that would need a `ReferenceState` park or an overlay column:
  - #5391's Eat/Sleep state;
  - #5359 WNAM and #5358 BODT, which are spawn-derived;
  - #5379 and #5384's re-adoption list, which is unsaved and purged.

## Findings

### MEDIUM

#### SAVE-D4-2026-10-09-01: the session-replace purge despawns only the pending convoy roots, so their subtree keeps a `Parent` that points at a dead entity, and `validate_world` refuses every later save for the life of the process

- **Severity**: MEDIUM
- **Dimension**: Save-Side Gates (consequence); the root cause is in Live Load-Apply (teardown completeness)
- **Data-Loss Class**: irrecoverable-write (save-refusal soft-lock: progress made after the load cannot be persisted; every on-disk save stays intact)
- **Location**:
  - `byroredux/src/cell_loader/unload.rs:97-113` (`purge_cinematic_retention_state`: `world.despawn_batch(pending)` at `:105`).
  - `byroredux/src/systems/cinematic.rs:550-563` (#5384: only PARENTLESS members are queued).
  - `crates/core/src/ecs/world.rs:170-187` (`despawn_batch` is non-recursive and touches only the listed ids).
  - `crates/save/src/validate.rs:121-158` (`validate_hierarchy`, the "parent has no Children component" arm at `:152-156`).
  - `byroredux/src/save_io.rs:1030-1047` (`SaveCommand::execute` aborts on any issue).
- **Status**: NEW. It shares its root cause with CONC-D5-2026-10-09-01, but its impact is different and that report does not name it.
  - CONC-D5 covers frozen ghost geometry, mesh/texture/BLAS refcounts that never reach zero, and a phantom Rapier collider.
  - This finding is the save gate: after such a load the player cannot save at all.
- **Trigger Conditions**:
  1. A scripted convoy (horse, cart, riders) releases at its route terminal while a convoy root sits outside every loaded cell, so it waits on `CinematicReAdoption.pending`. The #3817 test models exactly this as "the horse that drove past the loaded ring".
  2. An in-process save load or debug load runs before a cell loads beneath that root.
- **Description**:
  1. Since #5384, the pending list holds parentless roots only, and "the root's adoption stamps the subtree". The subtree nodes lost their `CellRoot` to `strip_retained_cell_root` when the home cell unloaded under retention.
  2. On a session replace, the purge `despawn_batch`es the roots. Each root's `Children` row dies with it, but its direct children keep `Parent(root)`.
  3. Nothing ever reaches those children:
     - the teardown walks only `CellRootIndex`;
     - the #5418 detach pass handles victims whose *parent survives*, which is the opposite direction;
     - a later load's purge finds an empty pending list.
  4. From then on, every `validate_world` call reports one `Hierarchy` error per orphaned child.
  5. Every save path runs `validate_world` and aborts on any issue: F5 quicksave, the pause menu, console `save`, a scripted `RequestSave`, and the remote console. Loading a different save does not clear the orphans, so the refusal lasts until the process restarts.
- **Evidence**:
  - Out-of-repo probe (`/tmp/audit/save/probe`, path deps on `byroredux-core[save]` and `byroredux-save`). The world is root (with `Children`) → child (`Parent`) → grandchild. `validate_world` reports 0 issues before `world.despawn_batch(vec![root])` (what the purge does) and 1 issue after: `[Hierarchy] entity 1: Parent(0) but parent has no Children component`.
  - `purge_despawns_pending_readoption_entities` (`unload.rs:1144`) uses one childless entity, so it cannot see this.
- **Impact**: After that load, every save attempt in the process returns "save ABORTED: N referential-integrity issue(s)". Nothing on disk is corrupted, but any progress made after the load is lost unless the player restarts and reloads. The post-load diagnostic `validate_world` (`save_io.rs:1974-1977`) logs the orphans once, so the cause can be diagnosed.
- **Related**: CONC-D5-2026-10-09-01 (same root cause, different consequence), #5379, #5384, #5056, #3817, and #5310 (the reverse orphan direction, already fixed).
- **Suggested Fix**:
  - Have the purge despawn each pending root's full `Children` closure, or better, hand that closure to the caller's `release_entities` teardown as CONC-D5 suggests. Either way, no child is left pointing at a dead parent.
  - Extend `purge_despawns_pending_readoption_entities` with a child node, and assert that `validate_world` comes back empty after the purge.

#### SAVE-D2-2026-10-09-01: #5412's new `ActorValue.set_override` layer is folded into `current()` but missed by three readers that re-sum the layers by hand: the HUD bar fraction, the debug vitals, and the SDK actor-value projection

- **Severity**: MEDIUM
- **Dimension**: Format & Schema Discipline. This is a cross-domain finding, traced from the v33→v34 field. The read sites belong to `/audit-character`, `/audit-ui` and `/audit-tooling`, none of which runs in this suite.
- **Data-Loss Class**: none (the field round-trips correctly; this is read-side only)
- **Location**:
  - `byroredux/src/hud.rs:849` (`fraction()`, shared by the MenuXml and Scaleform HUD drivers): `let max = entry.base + entry.permanent_mod + entry.temporary_mod;`
  - `byroredux/src/inventory.rs:317` (`vitals_snapshot()`, the debug-UI vitals): same `max`, and `current: max - entry.damage`.
  - `byroredux/src/extensions/capture.rs:235-240` builds `ActorValueState::new(base, permanent_mod, temporary_mod, damage)`. `crates/sdk/src/actor_values.rs:14-48` has no override slot, and its `current()` is `base + permanent + temporary - damage`.
- **Status**: NEW. It is a regression introduced by the #5412 fix (`a614eb273`). Before #5412, the player `SetBase` value lived in `permanent_mod`, which all three readers include.
- **Trigger Conditions**: `setav <player> Health|AP <v>` (console `edit_av`), or an SDK `SetBase` on a player `PlayerOnly` derived pool. Only these two writers reach `set_override` (`extensions/commands.rs:446-460`).
- **Description**: #5412 added a fifth layer: `current = base + set_override + permanent + temporary − damage` (`crates/core/src/ecs/components/actor_values.rs:78`). The commit's own test shows the effect: after `setav Health 500`, the values are base 105, `set_override` 500, current 605. The three hand-composed readers then go wrong as follows:
  - **HUD**: the bar's max stays at 105, so `current/max` clamps to 1.0. The bar shows full health until damage exceeds 500 (the old ratio was 405/605 = 0.67 after 200 damage).
  - **Debug vitals**: they report 105 − damage, which goes negative while the player actually has 405.
  - **SDK**: `EntityProjection::actor_value(Health).current()` returns the pre-`SetBase` value. A mod reading back its own `SetBase` sees it as not applied.
- **Evidence**: The grep `permanent_mod\s*\+` finds exactly these sites plus `CharacterRuleset::actor_value` (`ruleset.rs:169`). That last one is correct as written, because `set_override` is written only on `PlayerOnly` pools, and that arm composes `ActorGeneral` formulas.
- **Impact**: The player's Health and AP bars, the debug vitals panel and the SDK projection are wrong after any console or SDK `SetBase` on a derived pool. The bar under-reports damage, the panel can show negative health, and the SDK value is stale. Nothing is lost from a save.
- **Related**: #5412, #5239, #4675.
- **Suggested Fix**:
  - Give `ActorValue` an undamaged-max helper (`base + set_override + permanent_mod + temporary_mod`) and use it in both HUD readers.
  - Add the layer to the SDK `ActorValueState` (or fold it into `base` at the projection boundary). Pin it with a projection test after a routed `SetBase`.

## Notes (not filed)

- **BODT (#5358) changed the meaning of saved `EquipmentSlots` occupancy for 10 Skyrim ARMOs** (creature skins, Draugr hair and beard) without a bump. No bump is needed, because the shape is unchanged. Only v35 saves written in the few hours before `479414ffe` carry Draugr hair/beard rows with no occupants. Through the `ReferenceState` park, those rows keep hiding the parts for that save lineage. This affects dev builds only.
- **The v35 bump was not strictly needed.** A new registered column already changes the schema fingerprint. The bump follows the ledger's v26–v28 precedent and is harmless.
- **`StoryEventFill.reference_forms` records a reference only if it has a `SceneAliasCandidate` or is the player.** A leveled spawn without an ACHR identity stays session-only, and its alias is unbound after a load. This is documented in the type's doc.
- **CONC-D7-2026-10-09-01 (detached streaming worker) has no save-side consequence.** The payload channel belongs to the dropped `WorldStreamingState`, and the reloaded session gets a fresh channel, so stale payloads cannot cross a load.

## Guards Verified

| Guard | State |
|---|---|
| `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` (+ the reverse stale-row check) | green. New rows spot-checked: `VoiceSoundCache`, `EditorPlacement`, `EatSleepState` (still #5458), and the removed `StoryEventAliasFill`. |
| `serde_default_on_saved_struct_requires_format_major_bump` | green. `serde(skip)` on `StoryEventFill.slots` is correctly exempt. |
| `saved_type_shape_changes_require_format_major_bump` | green. `BASELINE_MAJOR = 35 = FORMAT_MAJOR`. Both refreshes were made with their bump in the same commit (checked commit by commit). #5403's blind spot is unchanged. |
| `delta_columns_*` (2), `npc_spawn_stamped_components_are_saved_or_intentionally_rederived` | green |
| Refusal, ring and quickload gates (`command_queue_tests::*`, `a_save_with_an_index_but_no_context_is_refused_even_when_the_flag_is_stale`) | green |
| `story_event_alias_fill_survives_save_load_as_reference_forms` (new), `story_manager_node_state_survives_save_load_and_still_gates` | green |
| `crates/save` header/CRC/atomic/ring tests, `write_slot_has_no_clobber_fallback` | green |
| `purge_despawns_pending_readoption_entities` (new, #5379) | green, but blind to subtrees (SAVE-D4-2026-10-09-01) |

Commands run (toolchain 1.96.0, `TMPDIR=/mnt/data/tmp`; no engine binary, smoke script or GPU process was launched):
- `cargo test -q -p byroredux-save`: 41 + 15 passed.
- `cargo test -q -p byroredux --bin byroredux save_io`: 75 passed, 4 ignored. The ignored tests are the 4 real-master `consumable_tests`.
- Out-of-repo probe for SAVE-D4-2026-10-09-01 (`/tmp/audit/save/probe`). Its target dir was in `/mnt/data/tmp` and was removed afterwards.

## Summary per Dimension

| Dimension | Findings | Notes |
|---|---|---|
| 1 — Snapshot Completeness & the Two Lists | 0 new | #5394 is verified. No two-list drift. #5458 is still open. |
| 2 — Format & Schema Discipline | 1 MEDIUM (cross-domain) | v34 and v35 are sound. #5459 and #5403 are still open. |
| 3 — Container & Disk Durability | 0 | Unchanged; guards re-run. |
| 4 — Save-Side Gates | 1 MEDIUM | The purge's orphaned subtree soft-locks the gate. |
| 5 — Live Load-Apply & Frame Boundary | 0 new | #5379 is verified fixed. #5460 and #5060 are still open. |

Suggested next step: `/audit-publish docs/audits/AUDIT_SAVE_2026-10-09.md`. Labels:
- SAVE-D4-2026-10-09-01: `save-load` + `bug` + `game:skyrim`.
- SAVE-D2-2026-10-09-01: `character` + `ui` + `bug`. Flag the SDK half for `/audit-tooling`.
