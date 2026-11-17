**HEAD**: 4079a9d33 (audited) · **Baseline**: AUDIT_SAVE_2026-11-15.md (35ab58d55) · **Scope**: the save/load commits since 11-15 — chiefly 278886579 (Oblivion actor-value identity, `FORMAT_MAJOR` 133, portable `FormIdPair` codecs) and the M65 sneak work (`Sneaking`, `SpeakingLine`, `not_shown_on_stealth_meter`; v130–v132) — plus every `Component` / `Resource` added since, checked against the registry and the `NOT_SAVED_BY_DESIGN` allowlist.

# Save/Load Audit: 2026-11-17

Read-only audit, five dimensions run as separate agents. Nothing was fixed. Guards run at audit time: `byroredux-save` (40 unit + 15 round-trip), `save_io` (110 passed, 4 ignored — the documented real-master tests), `serde_default_guard` (15 of 15), all green. Two findings were found independently by two dimensions and are merged below; the mechanisms of SAVE-1, SAVE-2 and SAVE-3 were re-read by the orchestrator, none was reproduced at runtime.

## Design claims (`crates/save/src/lib.rs`)

CODE-CONFIRMED: atomic write order, ring never clobbers the last good save, header gate order, typed preflight before teardown, `restore_world` unreachable from the load drain, no `#[serde(default)]` on a save type. DRIFTED: the "stable key" comments about `ActorVitals` (SAVE-1) and the SKILL's open `ActorRelocations` spawn-time question (resolved, see Doc rot).

## Findings

### SAVE-1 — HIGH — reference-break — the player's `ActorVitals.health` keeps the boot session's key after a live load
- **Where**: `byroredux/src/save_io/registry.rs:321-333` (the "stable key, never mutated" premise) and `MUTABLE_DELTA_COLUMNS`; `byroredux/src/scene.rs:1234` (`attach_to_player`, the only stamp).
- **Introduced**: 278886579 (v133). Before it a `u32` key was stable for the life of the process; after it the engine slot moves with the plugin set.
- **Problem**:
  - `ActorVitals` is registered with the actor-value codec but is not in `MUTABLE_DELTA_COLUMNS`, so a live load never overlays it.
  - The player body is created once, so its vitals are never re-stamped. A live load re-parses the masters stored in the save (`cell_ctx.masters`) and re-installs `LoadOrderIdentity`; on Oblivion the engine slot depends on that set.
  - After the overlay, `ActorValues` holds new-slot keys while `ActorVitals.health` holds the old one.
- **Effect** (when the slot differs):
  - `apply_health_damage` (`combat.rs:1289`) reads a missing value, so the first hit kills the player; `consume_item` refuses (`inventory/actions.rs:154`); the restoration system clears timed restorations.
  - The next save stores the stale key as a pair for whichever plugin now owns that slot, or aborts with `UnownedFormId` if none does.
- **Why no guard caught it**: `actor_value_save_survives_a_changed_engine_slot` checks vitals only through `restore_world`; its live-path leg overlays `ActorValues`, `TimedRestorations` and `ActiveMagicEffects` only.
- **Precondition**: Oblivion, and a live load whose plugin set differs from the running process. No saved data is lost.
- **Also false**: PRD `oblivion-actor-value-identity.md` §3.4 row 2 ("live reload re-derives it") and `charal.md:820`.
- **Fix**: overlay `ActorVitals` on a live load (add it to `MUTABLE_DELTA_COLUMNS` and the parked / `NOT_PARKED` partition), or re-stamp the player's vitals from `health_actor_value_key()` in the load drain; add the live-path assertion to the slot-move test.
- **Found by**: Dim 1 and Dim 5, independently.
- **Outcome**: 9d837aa3d. `ActorVitals` is a delta column, so the overlay re-keys the player's vitals; the changed-slot round trip now overlays the live path from a stale key and fails without the column. PRD §3.4 row 2 and `charal.md` corrected.

### SAVE-2 — MEDIUM — reference-break — process-lifetime rulesets keep the boot session's actor-value ids
- **Where**: `byroredux/src/cell_loader/references/mod.rs:366-411` (`CharacterRuleset`, `PoolRegenConfig`, `VatsModel`, `LimbEffects`, `MeleeDamageConfig`, each built once behind an `is_none()` guard).
- **Problem**: they hold ids resolved from the boot identity, and a live load that moves the engine slot does not rebuild them. `player_derived_stats_system` then writes the player's derived pools to old-slot keys every frame, and the next save stores them as pairs for the plugin that now owns the slot.
- **Same trigger and scope as SAVE-1.** The allowlist reasons for `PoolRegenConfig` and `VatsModel` ("re-derived from the current load order", `registry_completeness_tests.rs:458`, `:477`) are contradicted by the code. Oblivion's `PoolRegenConfig` and `VatsModel` are not built today (FO3/FNV lanes), so the reachable part is `CharacterRuleset` and `MeleeDamageConfig`; re-check when OAV Phase 3 lands.
- **Fix**: clear and rebuild these resources in the load drain after the identity is re-installed, or key them by `FormIdPair`.

### SAVE-3 — MEDIUM — corruption-on-load — a non-finite float saves successfully and the slot can never be loaded
- **Where**: `byroredux/src/save_io/commands.rs:277-292` (`SaveCommand::execute`), `crates/save/src/validate.rs:534` (the only finiteness gate, `Material`), `byroredux/src/commands/actor_value.rs:73`.
- **Problem**:
  - `serde_json` writes NaN / Inf `f32` as `null`, and decoding `null` into an `f32` fails. `setav` / `modav` parse `nan` and `inf` and `ActorValues` stores them unchecked.
  - `SaveCommand::execute` never decodes or type-validates the encoded bytes before `write_slot`; the read-back in `atomic_write` proves only that disk equals memory.
  - The load drain aborts with "save load ABORTED" and keeps the session. `quickload_latest` falls back past a corrupt newest slot only on a container-level decode failure, and `LoadCommand::execute` does not run `validate_snapshot_types` when it queues, so F9 never reaches the previous good slot.
- **Outcome**: older ring slots stay intact until the ring wraps (10 slots), so this is recoverable by hand, but the newest save is silently unusable.
- **Fix**: run `decode` + `validate_snapshot_types` on the encoded bytes before `write_slot` and refuse on failure; run typed validation per candidate in `quickload_latest`; reject non-finite input in `edit_av`.
- **Found by**: Dim 3 and Dim 4, independently.

### SAVE-4 — LOW — `setav` / `modav` accept any actor-value key, and a bad key makes every later save fail
- **Where**: `byroredux/src/commands/actor_value.rs`
- **Problem**: a key whose slot is not in the load order (including local id 0) enters `ActorValues`; `save_key` then returns `UnownedFormId` on every save until a reload. Normal paths cannot produce such a key, and the abort is loud. The intended refusal is pinned by a test, so this is a robustness note for a debug command (Dim 4 rated it MEDIUM; the orchestrator rated it LOW as console-only and self-clearing).

### SAVE-5 — LOW — no pre-write actor-value identity check, and the abort names `entity 0`
- **Where**: `crates/scripting/src/actor_value_save.rs`, `byroredux/src/cell_loader/reference_state.rs` (`codec::save_values(world, 0, ..)`), `SkillProgress`.
- **Problem**: unresolvable keys surface from inside `save_world`, after the gate block, and the error names entity 0 for parked `PersistentReferenceStates` rows and for `SkillProgress`; for parked rows it also names the wrong column. The abort itself is clean and writes nothing; no realistic path reaches it today beyond SAVE-1 and SAVE-4.
- **Fix**: a gate beside `validate_form_ids` that resolves every actor-value key and names the entity and column.

### SAVE-6 — LOW — `PendingDetectionEvents` and `PlayerActionNoise` straddle a load, and one allowlist reason is false
- **Where**: `discard_outgoing_session_requests` (`byroredux/src/save_io/live_load.rs`), `detection/tick.rs:366`, `registry_completeness_tests.rs`.
- **Problem**: producers run after the detection tick, which drains them next frame, so an F9 in between applies the outgoing session's events to the loaded world's first tick (same class as 11-15 SAVE-3). The `NOT_SAVED_BY_DESIGN` reason for `PendingDetectionEvents` ("none exists yet") has been false since 995ed089b.
- **Found by**: Dim 4 and Dim 5.

### SAVE-7 — LOW — the console `save` can still save in the tick of a scripted move
- **Where**: `DebugDrainSystem` (Late stage) versus `step_actor_relocations`.
- **Problem**: the 11-15 SAVE-4 deferral lives only in the F5 / pause-menu drain; console `save` and `save <slot>` run before the relocation step. Residual of the earlier fix.

### SAVE-8 — LOW — ring resume tie-break differs from `slots_by_recency`
- **Where**: `crates/save/src/disk.rs` (`cursor_after_newest` uses `max_by_key(mtime)`; `slots_by_recency` breaks ties by higher slot number).
- **Problem**: only with identical mtimes (FAT / exFAT or an mtime-normalizing copy), where the resumed cursor can pick the wrong slot to overwrite next. Writes stay atomic, so nothing is torn.

### SAVE-9 — LOW — the `Background` allowlist reason is still false
- **Where**: `registry_completeness_tests.rs` (`Background`: "no live production mutator").
- **Problem**: `player_identity_sync_system` and `player_body.rs:286` write it from the saved `CharacterIdentity`. Noted but not fixed on 11-15. A garbled comment at `registry.rs:66-70` rides along.

### Format discipline (Dim 2) — three LOW, two INFO
- **D2-01 (LOW)**: `registered_type_names` (`serde_default_guard_tests.rs:10-23`) matches only the turbofish register forms, so the six codec-registered columns (`ActorValues`, `ActorVitals`, `TimedRestorations`, `ActiveMagicEffects`, `SkillProgress`, `PersistentReferenceStates`) are visible only through four hand-listed files. The next codec wire type in an unlisted file escapes both serde guards, and a shape change there would not force a `FORMAT_MAJOR` bump.
- **D2-02 (LOW)**: `ReferenceState` / `PersistentReferenceStates` still derive serde and carry "pre-vN rejected" comments, but the wire shape is now `SavedReferenceState`. Shape-hash noise, not a data path.
- **D2-03 (LOW, latent)**: the `apply` closure in `register_component_codec` builds its `present` set from rows that survived `decode`, so a saved row that decodes to `None` reads as a saved absence and removes the live component. The two current replacing codecs always return `Some`.
- **D2-04 (INFO)**: the v133 baseline refresh also absorbs the `declaration_terminator` scanner change (it moves the hash of every tuple struct followed by an `impl`); the justification comment names only the bump.
- **D2-05 (INFO)**: the skill text says "eight explicit files" (now twelve) and "129", and omits the two new codec entry points and `UnownedFormId`.

## Data-Loss Class matrix

| Class | Findings |
|---|---|
| reference-break | SAVE-1, SAVE-2 |
| corruption-on-load | SAVE-3 |
| irrecoverable-write | none |
| silent-drop | none |
| none | SAVE-4 to SAVE-9, D2-01 to D2-05 |

## Completeness ledger

Computed by script over the registry and its `register_saved` delegates: 48 component and 55 resource rows. All 35 delta columns are registered. The 13 registered components outside `MUTABLE_DELTA_COLUMNS` match the documented set, apart from `ActorVitals` (SAVE-1). Every delta column sits in exactly one of PARKED, STREAM_SNAPSHOT or NOT_PARKED. `Sneaking` and `SpeakingLine` are replacing delta columns with true NOT_PARKED reasons. All 13 new `Component` / `Resource` impls are classified. 591 allowlist rows parsed, no duplicates. The removed-from-allowlist set (`CharacterController`, `Material`, `RumbleOnActivate`, `FragmentExecutionQueue`, the cinematic trio) is still registered.

## Checked clean

- **Format**: v129 to v133 ledger contiguous; every bump changed `BASELINE_MAJOR` and `BASELINE_SHAPE_FINGERPRINT` in the same commit with a justification; the new wire types in `actor_value_save.rs` are scanned and free of `serde(default)` / skip / alias. A v132 save on a v133 engine fails with `UnsupportedVersion` before any payload decode. `ValidateFn` parity holds for all seven registration variants. The schema-fingerprint pin is valid (the codec swap changes no key or policy).
- **Disk**: `disk.rs`, `atomic_file.rs` and `crates/save/tests` unchanged since 35ab58d55; atomic write order, temp removal, no clobber fallback, ring resume, header gate order and `BYROREDUX_SAVE_DIR` all hold. The pair codecs add roughly 10–15 KB per Oblivion actor (an estimate, not measured), under no size cap.
- **Gates**: `SaveCommand::execute` order as specified, the only caller of `save_world` / `write_slot`; a refused or aborted save never advances the ring (the only `advance()` is `commands.rs:298`). A stale actor-value key cannot abort a save in a fixed-plugin-set session or after a live load; the only routes are SAVE-4 and a mid-session `dbgload` with different masters (below).
- **Live load**: `LoadOrderIdentity` with the engine entry is installed on both reload arms before anything spawns or decodes. An unresolvable saved pair is dropped with a named warning and never attached to another record, and never aborts the load, for `ActorValues` (including aliases), `TimedRestorations`, `ActiveMagicEffects`, `SkillProgress`, vitals and parked rows. `apply_deltas` failures return before pose restore. 11-15 SAVE-1 to SAVE-4 are all still in place and pinned. `post_reload_drain_steps_keep_their_order` still matches the code. `Sneaking`, `SpeakingLine` and `not_shown_on_stealth_meter` are handled in the overlay and discard sweeps.
- **Not re-reported**: raw `u32` FormIDs in `SavedTimedRestoration.source_form_id` and `SavedActiveEffect.{source, mgef, caster, script}` are still load-order-global; OAI-SAVE-002 scopes AVIF references only, and changed-order loads for other saved FormIDs belong to the approved `portable-save-references.md` PRD.
- **Debug-only route**: stale keys after a mid-session plugin-set change are reachable through the debug-server `dbgload` with different masters; traced, not run (it shares SAVE-1's root cause).

## Doc rot

- The SKILL's open "Spawn-time ledger check" is resolved: 7250362df put `ActorRelocations` in `PRE_RELOAD_RESOURCES`, `LiveReloadLedgers` reinstates it on abort, and `SaveReloadTeardown` holds off the only teardown writer. No reference-break.
- `registry.rs:321-333`, PRD `oblivion-actor-value-identity.md` §3.4 row 2 and `charal.md:820` ("re-derived on live reload") are false for the player (SAVE-1).
- The `execute_pending_player_save_actions` doc comment sits above the `MAX_SAVE_DEFERRALS` const.

## Guards verified live

`serde_default_guard` 15/15, `registry_completeness_tests` (allowlist, delta-column partition, schema-fingerprint pin), `the_bump_ledger_is_contiguous_and_current`, `post_reload_drain_steps_keep_their_order`, `rejects_major_version_skew`, `write_slot_has_no_clobber_fallback`, `command_queue_tests` (refusal gates, ring cursor), `the_save_drain_publishes_the_transition_flag_before_draining`, and the OAI slot-move tests. Nothing is `#[ignore]`d outside the four real-master tests. Roughly 15 allowlist and NOT_PARKED reasons were spot-checked; the false ones are SAVE-6, SAVE-9 and the two named in SAVE-2.

## Effect on #7021

The changed-slot round trip passed in test, but OAI-SAVE-004's "every vital identical by identity" was proven only through `restore_world`; on the live-load path (the only one a player reaches) the vitals were not re-keyed (SAVE-1). #7021's boxes were ticked on the test evidence before this audit. SAVE-1 is fixed in 9d837aa3d and the live leg is now tested, so OAI-SAVE-004 holds for both paths; #7021 carries a correction comment.
