**HEAD**: 35ab58d55 (audited) · **Baseline**: AUDIT_SAVE_2026-10-31.md · **Scope**: the 44 save/load commits since 10-31, plus every `Component` / `Resource` added since then, checked against the save registry and the `NOT_SAVED_BY_DESIGN` allowlist.

# Save/Load Audit: 2026-11-15

Read-only audit, then every finding was fixed the same night. Guards run at audit time: `save_io` (103 passed) and `byroredux-save` (40 unit + 15 round-trip passed).

## Findings

### SAVE-1 — MEDIUM — after an exterior live load, created references and relocated actors were not respawned
- **Where**: `byroredux/src/app_step.rs` (`step_created_references`, `step_actor_relocations`)
- **Introduced**: 56fc348b8 (#6125); the same gate was copied in 4b71155ca.
- **Problem**:
  - Both steps return early while their key is unchanged.
  - An exterior key is the loaded grid set plus the ledger, so a quickload onto the same grids with the same saved records leaves it equal.
  - The reload tears the spawned items and actors down. A dropped item or a relocated NPC then stayed missing until the grid set next changed.
- **Outcome**: c4d1ea3f2. `step_save_loads` forgets the created, relocation and enabled step keys after the drain.

### SAVE-2 — MEDIUM — perk Quest entries re-ran on a live load
- **Where**: `crates/scripting/src/perk_entry_points/bodies.rs` (`reconcile_perk_bodies`)
- **Introduced**: 18063a780 (PERK-BODY)
- **Problem**:
  - `PerkBodyState` lives on the surviving player, so the outgoing perks were compared against the saved set.
  - Every rank the save held beyond the outgoing perks re-ran its `StartQuest` / `SetStage`.
- **Outcome**: 734bcb1fa. The drain removes the player's state after the overlay, so the next pass takes the saved perks as its baseline. The allowlist reason is corrected.

### SAVE-3 — LOW — two same-tick queues survived the load
- **Where**: `PendingScriptStoryEvents` and `PendingNonArmorEquips`, missing from `discard_outgoing_session_requests`
- **Problem**: both are filled by Late producers and drained after the load, and both name the surviving player.
- **Outcome**: 734bcb1fa. Both are cleared, and the discard test covers both.

### SAVE-4 — LOW — a save taken in the tick of a scripted move missed the move
- **Where**: the frame order. `step_player_save_actions` runs before `step_actor_relocations`.
- **Introduced**: 4b71155ca
- **Outcome**: d72e1f45e. A queued quicksave waits while relocations or scripted moves are pending, for at most three ticks.

## Checked clean
- **Format discipline**: every `BASELINE_SHAPE_FINGERPRINT` change since 10-31 shares a commit with a `FORMAT_MAJOR` bump (v123–v129), or carries a stated reason (c6baf6c49, c72ecafad).
- **Unchanged since 10-31**: atomic write, the slot ring, the refusal gates.
- **Live-load order**: pinned by `post_reload_drain_steps_keep_their_order`.
- **Leftover `EntityId`s**: stale ids in session resources only leak, because ids are never recycled (`world.rs`).

## Not fixed (noted)
- **`Background` allowlist row**: it says "no live production mutator", which is no longer true (`player_identity_sync_system`, `sync_body_after_load` both write it). This is doc-rot only.
- **Cold `--load` variant of SAVE-2**: on a cold load the player spawns with empty `Perks`, and the first tick may set that empty set as the baseline before the load drain. Unverified whether `PerkEntryTable` is installed on that first tick.
