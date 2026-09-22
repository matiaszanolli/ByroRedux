# Save / Load Subsystem Audit (M45 + M45.1) — 2026-09-22

**HEAD**: `ee6d3fb39` · **Baseline**: `docs/audits/AUDIT_SAVE_2026-09-11.md` (HEAD `b3db49fa`) ·
**Audited**: Dimensions 1, 2, 3, 4, 5 (all five — every dimension had commits
since baseline) · **Unchanged since baseline (skimmed)**: Dimension 3
(Container & Disk Durability — zero commits to `disk.rs`/`atomic_file.rs`/the
container header format this window; guard ledger re-run and confirmed green)

Run solo (no sub-agent fan-out, per this run's instructions), one dimension at
a time, against an 11-day, unusually heavy delta window: the prior cycle's
CRITICAL and both its HIGHs are now fixed; `FORMAT_MAJOR` moved 22→25; a new
`reference_state.rs` module (`PersistentReferenceStates`, the P3 pickup/loot
persistence layer) landed and introduces this cycle's one live-mechanism
finding, already independently caught by the same day's `/audit-gameplay`
run (`AUDIT_GAMEPLAY_2026-09-21.md`) — cited and extended here rather than
re-reported, per this run's brief.

## Executive Summary

`crates/save/src/lib.rs` / `snapshot.rs` docstring claims verified against live code:

| Claim | Status |
|---|---|
| Full ECS snapshot (curated game-state set) | **CODE-CONFIRMED for the mechanism, DRIFTED for one interaction.** The registry-completeness guard is green and independently re-verified (brace-matched rescan finds the same 332 candidate types, same 5 unclassified — no regression). But a new, correctly-registered resource (`PersistentReferenceStates`) has its saved content silently fail to reach the entities it describes on a save-load, due to an interaction with the pre-existing `without_parked_state` wrapper — see Findings. |
| Atomic write (tmp → fsync → read-back-verify → rename → dir-fsync) | **CODE-CONFIRMED**, unchanged — zero commits to `crates/save/src/disk.rs` / `crates/core/src/atomic_file.rs` this window. |
| Ring never clobbers the last good save | **CODE-CONFIRMED**, unchanged. |
| Validation gate refuses to persist an inconsistent world | **CODE-CONFIRMED, and its prior gap is now closed.** `PapyrusProviderContinuationQueue`'s unvalidated `EntityRef` hazard (SAVE-D4-2026-09-11-01) is fixed by a surgical post-reload purge (#4139) rather than a validation-gate addition — a different but sound mechanism. A fresh sweep of every `EntityId`-bearing saved type found no new uncovered reference field this window. |
| Typed-decode preflight rejects a bad snapshot before any teardown | **CODE-CONFIRMED**, unchanged in shape. |
| `FORMAT_MAJOR` bump is the only sanctioned schema evolution path | **CODE-CONFIRMED, and last cycle's precedent error is now corrected.** `FORMAT_MAJOR` is now **25** (was 22). All three new bumps (v23 `CinematicPresentationState`, v24 `Material.detail_neutral`, v25 `ReferenceState.picked_up`) are individually correct calls with same-commit baseline regeneration; v22's previously-wrong "discriminant shift" rationale (SAVE-D2-2026-09-11-01) is corrected and the fixed rule is correctly reused for every subsequent variant-insertion bump since. |
| Off-frame load, never inside the scheduler | **CODE-CONFIRMED**, unchanged; `restore_world`/`apply_deltas` still never run inside the scheduler, and `restore_world` still has zero production callers. |
| Additive-only overlay + explicit reconciler for removals | **CODE-CONFIRMED**, unchanged this window — no new production removal site of a `MUTABLE_DELTA_COLUMNS` type found without a reconciler/`NoReconcilerNeeded` disposition. |
| Saved resources are in force when the reloaded cell is built | **CODE-CONFIRMED for the fixed mechanism, DRIFTED for one new resource's interaction with it.** #4135 correctly narrowed the pre-reload restore to an explicit `PRE_RELOAD_RESOURCES` allowlist (`ReferenceEnableState`, `ReferenceLockState`), closing last cycle's CRITICAL cleanly — verified structurally, not by healing-masked test. But `PersistentReferenceStates`, added the same window, needs the *same* pre-reload timing its spawn-time consumer (`reference_state::restore()`) requires, and is **not** in that allowlist — and critically, simply adding it would not fix the gap, because it is separately wrapped by `without_parked_state`, which strips it for the whole reload window regardless of what was pre-installed. See Findings. |
| "Reproducible CRC across runs at equal state" | **DRIFTED, newly re-verified this cycle.** True at the component-column level (`registry.rs` sorts rows by entity id, the #1708 fix). False at the *resource* level: five saved resources (`Globals`, `QuestStageState`, `ReferenceEnableState`, and, new this window, `ReferenceLockState` and `PersistentReferenceStates`) serialize a `HashMap`/`HashSet` field in hash-iteration order with no sort. No data loss (nothing in the codebase diffs/hashes whole saves today), but the doc claim is unqualified and the affected set grew this window. |

**Findings this cycle: 3 total — 1 HIGH, 2 MEDIUM, 0 CRITICAL, 0 LOW newly
written up.** Of these, **2** (the HIGH and one MEDIUM) are the same
save-mechanism facts already reported by the same-day `/audit-gameplay` run
as `GAME-D7-2026-09-21-01` / `GAME-D7-2026-09-21-02` — cited as **Existing**
per this run's dedup instruction, with save-side-only extensions (not
re-derived from scratch, not double-counted). **1** (the resource-determinism
doc/contract mismatch) is genuinely **NEW** to any save audit report.

Eight prior-cycle findings (1 CRITICAL, 2 HIGH, 3 MEDIUM, 2 of 4 LOW spot-
checked) are confirmed **CLOSED** at HEAD, verified against live code rather
than taken on commit-message word: `SAVE-D1-2026-09-11-01` (CRITICAL, #4135),
`SAVE-D1-2026-09-11-02` (HIGH, #4136 — via a FormID-ledger redesign, not the
literal suggested fix), `SAVE-D5-2026-09-11-01` (HIGH, #4138),
`SAVE-D4-2026-09-11-01` (MEDIUM, #4139), `SAVE-D2-2026-09-11-01` (MEDIUM,
#4140), `SAVE-D2-2026-09-11-02` (MEDIUM, #4141), `SAVE-D2-2026-09-11-03` /
`-04` (LOW, #4142/#4143), `SAVE-D4-2026-09-11-02` (LOW, #4144).
`SAVE-D6-2026-09-11-01` (LOW, doc-integration gap) was not re-checked this
cycle — budget went to the two live findings — carried forward as
presumed-still-open.

## Data-Loss Class Matrix

| Finding | Class | Dimension | Severity | Status |
|---|---|---|---|---|
| SAVE-D5-2026-09-22-01 — `PersistentReferenceStates` pickup tombstone orphaned by `without_parked_state` on every save-load | silent-drop | 5 | **HIGH** | Existing: GAME-D7-2026-09-21-01 |
| SAVE-D1-2026-09-22-01 — registry-completeness guard truncates each file at its first `#[cfg(test)]`, 5 production types unclassified | latent (guard-coverage gap) | 1 | MEDIUM | Existing: GAME-D7-2026-09-21-02 |
| SAVE-D1-2026-09-22-02 — saved *resources* (not components) serialize in hash-iteration order; "reproducible CRC" doc claim unqualified | none (doc/contract mismatch) | 1 | MEDIUM | NEW |

## Completeness Ledger (delta from baseline only — see `AUDIT_SAVE_2026-09-11.md` for the full table)

Cross-checked against the SAVE-D1-12 guard's `NOT_SAVED_BY_DESIGN` allowlist
(green, independently re-verified via brace-matched rescan — see Dimension 1
findings) rather than re-derived from scratch.

| Column | Kind | Saved | Overlaid | Status |
|---|---|---|---|---|
| `ReferenceLockState` | Resource (new, #4136) | yes | n/a (`PRE_RELOAD_RESOURCES`) | Correctly wired: spawn-time consumer (`cell_loader/spawn.rs:506`) served by the pre-reload subset restore, no parking wrapper, no ordering hazard. Replaces `Locked`'s prior gap (SAVE-D1-2026-09-11-02, now closed). |
| `PersistentReferenceStates` | Resource (new, `d8255b2e2`) | yes | n/a (wholesale `restore_resources` only) | **NOT** in `PRE_RELOAD_RESOURCES`, and wrapped by `without_parked_state` which would defeat adding it there anyway. Saved tombstone rows (`picked_up`, plus dead/inventory/equipment/actor-value snapshots for nonresident placements) land too late for the reload's own spawn pass to consume them — SAVE-D5-2026-09-22-01 (HIGH). |
| `Locked` | Component | **NO** (by design, corrected) | n/a | Was the baseline's HIGH gap; now correctly rederived from XLOC *as overridden by* the registered `ReferenceLockState` ledger — same shape as `DoorTeleport`/`NavmeshTile` ("genuinely rederived — now that its override source is itself save state"). |
| `DraugrCombatAnim`, `DraugrCombatClips`, `ExposureTuning`, `NavmeshResidency`, `GlobalFormIdResolver` | Component/Resource (pre-existing, newly identified as guard-blind) | Neither registered nor allowlisted | n/a | SAVE-D1-2026-09-22-01. Per-type risk verdict (new this audit): 4 of 5 are false positives with the exclusion reason already present in their own doc comments (just never surfaced to the guard); 1 (`DraugrCombatAnim::death_played` specifically) is a real, already-tracked gap (`GAME-D4-2026-09-21-05`, LOW/latent). |
| `Globals`, `QuestStageState`, `ReferenceEnableState`, `ReferenceLockState`, `PersistentReferenceStates` | Resource | yes | n/a | All five serialize an internal `HashMap`/`HashSet` in hash-iteration order — SAVE-D1-2026-09-22-02 (MEDIUM, doc/contract mismatch only). |

Every other row of the baseline's Completeness Ledger (structural identity
columns, `AnimationPlayer`/`AnimationStack`, `FollowState`/`EscortState`/
`Seated`, the cinematic pair, `Material`, `ActorVitals`, the wholesale
resource set) re-spot-checked present and unchanged in `build_save_registry`.

## Findings

### HIGH

#### SAVE-D5-2026-09-22-01: `PersistentReferenceStates`'s pickup tombstone is orphaned by `without_parked_state` on every save-load, in both interior and exterior sessions, repeating on every subsequent load

- **Severity**: HIGH
- **Dimension**: 5 — Live Load-Apply & Frame Boundary (mechanism); the gameplay-visible symptom is Dimension 1's completeness/loot-persistence territory too, split per the skill's ownership note
- **Data-Loss Class**: silent-drop
- **Location**: `byroredux/src/save_io.rs:169` (`PRE_RELOAD_RESOURCES`, `PersistentReferenceStates` absent), `:1668-1721` (the restore-subset → `without_parked_state`-wrapped reload → wholesale-restore sequence), `:1684` (the `without_parked_state` call site); `byroredux/src/cell_loader/reference_state.rs:54-72` (`without_parked_state`), `:166-224` (`restore()`); `byroredux/src/cell_loader/references/attach.rs:246`, `synth_child.rs:97,861` (spawn-time `restore()` call sites); `byroredux/src/inventory.rs:877-925` (`PickedUp` marker + `pickup_loot`)
- **Status**: **Existing: GAME-D7-2026-09-21-01** (`docs/audits/AUDIT_GAMEPLAY_2026-09-21.md`, no GitHub issue filed yet as of this run). Confirmed still open at HEAD `ee6d3fb39` — `#4465`/`#4466` (this window's v25 `FORMAT_MAJOR` bump) fixed only the byte-level round-trip of the `picked_up` field; the load-apply ordering bug is untouched by any commit in this delta window.
- **Description**: `pickup_loot` stamps the transient `PickedUp` marker on a placement root (and its mesh descendants, per the already-fixed #4571) and immediately writes a durable `picked_up: true` row into `PersistentReferenceStates`, keyed by `FormIdPair` — this happens regardless of whether the cell is later evicted, so a save taken while the item's placement is still resident correctly captures the tombstone. The bug is entirely on the **load** side. `execute_pending_save_loads` restores only `ReferenceEnableState`/`ReferenceLockState` before the reload (`PRE_RELOAD_RESOURCES`); the reload itself runs inside `cell_loader::reference_state::without_parked_state`, which unconditionally removes whatever `PersistentReferenceStates` is currently installed for the duration of the reload closure — a deliberate design to stop the *outgoing* session's own rows from contaminating the load — and restores that same removed value afterward. Because the reload is a synchronous, from-scratch teardown+respawn of every placement in the cell/exterior radius (confirmed for both `reload_interior_session`'s direct `load_cell_with_masters` call and `reload_exterior_session`'s blocking `FullRadius`-mode `bootstrap_waiting` loop), every placement — including the previously-picked-up one — is spawned and attached (`reference_state::restore()` runs, finds no `PersistentReferenceStates` resource at all, returns `false`) strictly *before* the wholesale post-reload `restore_resources` call (`:1721`) ever installs the saved tombstone rows. Nothing re-triggers `restore()` for an already-spawned entity afterward, so the saved row sits in the resource, permanently unconsumed for that load. The freshly-spawned placement comes back fully interactive, while the player's inventory (correctly overlaid via `apply_deltas`) already holds the item — a duplicate.

  Critically, the established, already-correct fix pattern for this exact
  class of bug — add the resource to `PRE_RELOAD_RESOURCES`, exactly as
  `ReferenceEnableState` (#3789/#4135) and `ReferenceLockState` (#4136, same
  window) both received — **does not work here**, because
  `without_parked_state` strips `PersistentReferenceStates` for the whole
  reload window regardless of whether a pre-reload restore just installed
  the saved value or the live session's own value was still present. A fixer
  reaching for the pattern the codebase visibly already uses twice in the
  same file would ship a fix that still doesn't work, without a test to
  catch it.
- **Evidence**: Full call trace above, independently walked line-by-line
  against `save_io.rs`, `reference_state.rs`, `attach.rs`, `synth_child.rs`.
  No test exists that drives a pickup through the actual
  `execute_pending_save_loads` path (`container_and_corpse_loot_survive_
  encoded_live_overlay` covers containers/corpses only, per GAME-D7's own
  evidence). `capture()`'s `Option<ItemInstance>` is stored by value, never
  a raw `ItemInstanceId`/slot index, so there is no `ItemInstancePool`-swap
  corruption risk analogous to the now-fixed SAVE-D1-2026-09-11-01 — this is
  pure non-application, not corruption.
- **Impact**: Save → load in (or into) any cell containing a placement the
  player already picked up returns that placement fully interactive while
  the pickup already sits in inventory — a duplicate, reachable on any game
  with lootable world placements. Because the bug reproduces identically on
  every load of the same save (each load is an independent fresh
  teardown+respawn), a player can farm additional duplicates by repeatedly
  quickloading while resident in the affected cell — an unbounded item-
  duplication exploit, not a one-time desync. Applies equally to interior
  and exterior save-loads.
- **Related**: `#4571` (a different, already-fixed `PickedUp` render-skip
  bug — mesh descendants, unrelated to this ordering issue); `#4465`/`#4466`
  (fixed the serialization half only); `#4135`/`#4136` (the correct
  `PRE_RELOAD_RESOURCES` pattern this bug's resource cannot simply adopt);
  GAME-D2-2026-09-21-02 (pickup reachability, cited by GAME-D7 as a
  prerequisite).
- **Suggested Fix**: register a zero-field `PickedUp` delta column
  (mirrors the `Dead` reconciler pattern — `apply_deltas` runs after both
  resource restores and matches by FormId, sidestepping
  `without_parked_state` entirely), **or** add an explicit pass after the
  post-reload `restore_resources` call (`save_io.rs:1721`) that consumes
  `PersistentReferenceStates.picked_up` rows against now-resident,
  FormId-matched entities. Do not simply add the resource to
  `PRE_RELOAD_RESOURCES` — that alone will not fix it, per the mechanism
  above. Add a save → load round-trip test through
  `execute_pending_save_loads` itself (not `restore_world`), covering both
  interior and exterior contexts and a second consecutive load of the same
  slot to pin non-repeatability once fixed.

### MEDIUM

#### SAVE-D1-2026-09-22-01: the registry-completeness guard truncates each scanned file at its FIRST `#[cfg(test)]`, hiding 39 production types (5 unclassified) — independently re-verified, with a per-type risk verdict the original finding didn't give

- **Severity**: MEDIUM
- **Dimension**: 1 — Snapshot Completeness & the Two Lists
- **Data-Loss Class**: latent (guard-coverage gap) — see per-type verdict below for the one real-risk case
- **Location**: `byroredux/src/save_io/registry_completeness_tests.rs:521` (`let production_src = src.split("#[cfg(test)]").next().unwrap_or(&src);`); `byroredux/src/components.rs` (five separate `#[cfg(test)]` blocks at lines 663, 731, 889, 2209, 2249, interleaved with ~32 further production `impl Component`/`impl Resource` lines the guard never sees); one hit each in `cell_loader/load_order.rs`, `save_io.rs`, `extensions/systems.rs`, `scene_import_cache.rs`, `skinned_mesh.rs`
- **Status**: **Existing: GAME-D7-2026-09-21-02** (`docs/audits/AUDIT_GAMEPLAY_2026-09-21.md`, no GitHub issue filed yet). Confirmed accurate and **no regression** — independently reran a brace-matching (non-truncating) rescan of every `discover_scan_roots()` file (`/tmp/audit/save/hidden_types.py`, deleted with the rest of the scratch dir per this run's cleanup step); it finds the same 332 total `impl Component`/`impl Resource` occurrences and, after correcting two false-negatives in my own quick script (`FormIdComponent`'s `register_form_id_component("FormIdComponent")` has no turbofish; `PersistentReferenceStates`'s registration wraps its string literal onto a second source line — both **are** correctly registered), the identical 5 unclassified types GAME-D7-2026-09-21-02 names, and no others. No new unclassified type introduced by any commit in this delta window, including today's.
- **Description / new save-side fact this audit adds**: GAME-D7-2026-09-21-02 names the five hidden-and-unclassified types without individually judging each one's real risk. This audit's per-type verdict:

  | Type | Verdict | Basis |
  |---|---|---|
  | `DraugrCombatAnim` (`components.rs:2053`) | **Real gap, scoped to one field.** `take`/`take_remaining`/`captured`/`inserted_player` are transient one-shot take-playback scratch, correctly excludable; `death_played: bool` is a terminal per-NPC latch with a real reload-time consequence, already independently tracked at `GAME-D4-2026-09-21-05` (LOW, latent until a sibling bug lands — no severity change from this audit). |
  | `DraugrCombatClips` (`:2023`) | **False positive.** Own doc comment: "Resolved once beside the walk-clip installation" — decoded-once static clip durations, never mutated. Same posture as the already-allowlisted `SkyrimWalkClip`. |
  | `ExposureTuning` (`:1826`) | **False positive.** Own doc comment: mutated only by the `exposure`/`tonemap` console commands, boot state from CLI — identical posture to the already-allowlisted `LightTuning` one struct above it in the same file. |
  | `NavmeshResidency` (`:2134`) | **False positive, already self-documented.** Own doc comment states verbatim it is "Deliberately not save-registered, matching `NavPath` and `NavmeshTile`" — the reasoning exists in source, never reached the allowlist. |
  | `GlobalFormIdResolver` (`cell_loader/load_order.rs:293`) | **False positive.** Rebuilt wholesale from the ESM masters list + content/faction catalogs on every load — rederived load-order infrastructure, same class as the already-allowlisted `SaveRegistry`. |

  Net: 4 of 5 are pure guard-blindness with zero live risk (the exclusion
  reason already exists in each type's own doc comment, it just never
  reached `NOT_SAVED_BY_DESIGN`); 1 field on 1 type is a genuine latent gap,
  already tracked elsewhere at the correct severity. This does not change
  GAME-D7-2026-09-21-02's MEDIUM rating — the mechanism failure (silent
  blindness to whatever lands in that region *next*) is the real risk,
  independent of today's specific five.
- **Impact**: unchanged from GAME-D7-2026-09-21-02 — a guard whose entire
  purpose is catching exactly this class of gap is green while structurally
  unable to see roughly a tenth of the workspace's `impl Component`/
  `impl Resource` sites.
- **Related**: GAME-D4-2026-09-21-05 (the one real gap this hides); #3497/
  #4141 (the sibling `serde_default_guard_tests.rs` scan-root fix, a
  different mechanism, already closed this window — see Dimension 2).
- **Suggested Fix**: unchanged from GAME-D7-2026-09-21-02 — brace-match
  `#[cfg(test)]` stripping instead of a first-match string split; add the
  four false-positive allowlist rows with their existing doc-comment
  reasoning; track `death_played` under `GAME-D4-2026-09-21-05`'s existing
  fix.

#### SAVE-D1-2026-09-22-02: saved *resources* (as distinct from component columns) serialize `HashMap`/`HashSet` fields in hash-iteration order — the "reproducible CRC across runs at equal state" doc claim is unqualified, and the affected set grew this window

- **Severity**: MEDIUM
- **Dimension**: 1 — Snapshot Completeness & the Two Lists
- **Data-Loss Class**: none (doc/contract mismatch only)
- **Location**: `crates/save/src/snapshot.rs:227-229` (`Snapshot` doc: "`BTreeMap` keeps the JSON output deterministic... reproducible CRCs across runs at equal state"); `docs/engine/save-load-roundtrip.md:59` ("rows are sorted by entity id first for a reproducible CRC" — scoped to component rows only, doesn't disclaim resources); `crates/scripting/src/globals.rs:22` (`Globals(HashMap<u32,f32>)`); `crates/scripting/src/quest_stages.rs:76` (`QuestStageState { quests: HashMap<QuestFormId,_> }`); `crates/scripting/src/fragment/state.rs:17,66` (`ReferenceEnableState { disabled: HashSet<u32> }`, `ReferenceLockState { overrides: HashMap<u32,_> }`); `byroredux/src/cell_loader/reference_state.rs:41-44,72-84` (`PersistentReferenceStates { rows: HashMap<FormIdPair,_> }`, serialized via `pair_rows::serialize` as `rows.iter().collect::<Vec<_>>()` — a direct hash-iteration-order dump, no sort)
- **Status**: NEW. Not previously the subject of any numbered `AUDIT_SAVE_*.md` finding (searched all thirteen prior reports); the closed `#1708`/`SAVE-D1-01` (2026-06-23) covered a different thing — sparse **component** row order, fixed by sorting component rows by entity id (`registry.rs:130,288`), which does not touch resources.
- **Description**: `Snapshot.components`/`.resources` are `BTreeMap`s (column keys sorted) and component rows are entity-id-sorted, but each resource's own `Serialize` impl is whatever its `#[derive(Serialize)]` produces. For every `HashMap`/`HashSet`-backed resource that is Rust's per-process-randomized hash iteration order, not a content-sorted order — two saves of bit-identical game state can differ byte-for-byte, and therefore in CRC32, purely from hash-table iteration order. This window added two more `HashMap`-backed saved resources to the already-nondeterministic set (`ReferenceLockState`, `PersistentReferenceStates`), growing it from 3 to 5.
- **Impact**: No data loss — `decode`'s CRC check still correctly catches genuine corruption/truncation of whatever bytes were actually written, and nothing in the codebase currently diffs, dedups, or content-hashes whole save files (`SaveRing`/`slots_by_recency` key off mtime and slot number, not content hash). The risk is precedent-quality: the doc comment states a false invariant a future feature (save-sync, corruption telemetry, save-dedup-on-disk) could rely on without re-checking.
- **Related**: `#1708` (closed — the component-row half of the same general claim).
- **Suggested Fix**: narrow `snapshot.rs`'s doc claim to "component columns" explicitly (cheapest, and matches what's actually true today), or sort each affected resource's map/set at its own `Serialize` boundary (a `BTreeMap`/sorted-`Vec` shim per resource, mirroring `PersistentReferenceStates::pair_rows`'s existing custom row serializer, with a sort added). No test currently exercises this; a same-state-two-runs-equal-CRC test would need a controlled hasher to be meaningful and is probably not worth adding unless a consumer of the doc claim actually materializes.

## Guards Verified

| Guard | Location | Invariant pinned | State |
|---|---|---|---|
| `every_component_or_resource_impl_is_saved_or_explicitly_allowlisted` | `save_io/registry_completeness_tests.rs` | every discovered production `impl Component`/`impl Resource` is registered XOR allowlisted | **green**, but structurally blind to text after each file's first `#[cfg(test)]` — SAVE-D1-2026-09-22-01 |
| `saved_type_shape_changes_require_format_major_bump` / `serde_default_on_saved_struct_requires_format_major_bump` | `save_io/serde_default_guard_tests.rs` | any shape change or `#[serde(default)]` on a saved type requires a `FORMAT_MAJOR` bump | **green**, `BASELINE_MAJOR = 25` current, ~12 baseline refreshes this window all carry a dated justification comment, spot-checked 4 directly |
| `source_discovery_follows_registry_and_nested_save_modules`, `save_type_discovery_walks_every_workspace_crate` | `save_io/serde_default_guard_tests.rs` | scan roots derive from the live workspace, not a hardcoded list | **green**, new this window (#4141) — closes the baseline's SAVE-D2-2026-09-11-02 |
| `delta_columns_removed_at_runtime_have_a_load_reconciler`, `delta_columns_carry_only_session_stable_fields` | `save_io/round_trip_tests.rs` | every production removal of a delta column has a reconciler; no session-local field in `MUTABLE_DELTA_COLUMNS` | **green**, unchanged |
| `pre_reload_restore_must_not_install_saved_item_instance_pool_early` | `save_io/live_reload_tests.rs` | `PRE_RELOAD_RESOURCES` excludes `ItemInstancePool` | **green**, new this window (#4135) — structurally discriminates which pool is installed, not healing-masked |
| `the_save_drain_publishes_the_transition_flag_before_draining` | `app_step.rs` | `CellTransitionInFlight` published before the save-action drain in the same tick | **green**, new this window (#4138) |
| `a_save_taken_mid_cell_transition_is_refused_not_written`, `a_save_taken_while_chargen_disables_saving_is_refused_not_written` | `save_io/command_queue_tests.rs` | the two new save-side refusal gates | **green**, new this window (#4138, #4372) |
| Container gates (`rejects_bad_magic`/`_truncated`/`_payload_truncation`/`detects_crc_corruption`/`rejects_schema_mismatch`/`rejects_major_version_skew`) | `crates/save/src/snapshot.rs` | every header gate precedes `serde_json::from_slice`; CRC is payload-only | **green**, unchanged |
| `atomic_write_replaces_the_target_and_consumes_the_temp`, `atomic_write_fails_without_renaming_when_the_temp_cannot_be_created` | `crates/core/src/atomic_file.rs` | shared durable-write sequence | **green**, unchanged |
| `lock_effects_survive_save_load_round_trip_and_still_apply`, `reference_enable_state_survives_save_load_round_trip` | `save_io/round_trip_tests.rs` | the v22/v23 shape changes actually round-trip behaviorally | **green**, new this window (#4142/#4143) — closes SAVE-D2-2026-09-11-03/-04 |
| `set_in_chargen_renames_still_decode_v23_keys` | `save_io/serde_default_guard_tests.rs` | v23→#4322 field renames stay decodable via `serde(alias)` | **green** |
| `every_component_or_resource...` full suite + `crates/save` unit/integration tests | — | see full run below | **55/55** (`byroredux-save`), **67/67 passed, 4 correctly `#[ignore]`d** (`byroredux` binary `save_io`) |

Full test commands run this cycle: `cargo test -p byroredux-save -j 4` (55
passed), `cargo test -p byroredux --bin byroredux -j 4 save_io` (67 passed, 4
ignored — real-master `consumable_tests`, correctly gated), `cargo test -p
byroredux --bin byroredux -j 4 the_save_drain_publishes` (1 passed). No
workspace-wide or release build run, per this cycle's resource constraints.

## Summary Table (per dimension)

| Dimension | Findings | Notes |
|---|---|---|
| 1 — Snapshot Completeness & the Two Lists | **2** (2 MEDIUM; 1 of these matched to an existing cross-audit finding) | The pre-reload CRITICAL and `Locked` HIGH are both cleanly closed. New: the registry-completeness truncation bug (extended with a per-type risk verdict) and the resource-serialization determinism doc/contract mismatch (newly reported). |
| 2 — Format & Schema Discipline | **0** | All four prior-cycle findings (v22 rationale, scan-root staleness, two round-trip test gaps) confirmed closed. Three new `FORMAT_MAJOR` bumps and ~12 baseline refreshes all independently re-verified sound — the precedent bank in `serde_default_guard_tests.rs` is exemplary. |
| 3 — Container & Disk Durability | **0** | Unchanged since baseline — zero commits; guard ledger re-run and green. |
| 4 — Save-Side Gates | **0** | The mid-transition-save HIGH (moved here from Dim 5 at baseline, since its fix is a `SaveCommand::execute` gate) and the `PapyrusProviderContinuationQueue` MEDIUM are both cleanly closed. Fresh sweep of every `EntityId`-bearing saved type found no newly-registered type with an uncovered reference field. |
| 5 — Live Load-Apply & Frame Boundary | **1** (HIGH; matched to an existing cross-audit finding, not double-counted against Dim 1's tally) | The strict apply sequence is unchanged and correct end to end apart from one new resource's interaction with the pre-existing `without_parked_state` wrapper — a genuine, currently-live item-duplication bug, already caught by the same-day `/audit-gameplay` run and extended here with the save-side mechanism detail that explains *why* the obvious fix doesn't work. |

**Total: 3 findings this cycle — 0 CRITICAL, 1 HIGH, 2 MEDIUM, 0 LOW newly
written up.** 2 of the 3 (the HIGH and one MEDIUM) match existing findings
from today's `/audit-gameplay` run (no GitHub issue yet for either); 1
(MEDIUM) is genuinely new. 8 prior-cycle findings confirmed closed; 1
(`SAVE-D6-2026-09-11-01`, doc-integration gap) not re-checked this cycle,
carried forward as presumed open.
