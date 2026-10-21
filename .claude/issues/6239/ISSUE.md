# #6239: SAVE-2026-10-21-03: Transient request queues straddle the load drain: a same-frame player `MoveTo` runs after the load, and Late-stage XP/skill grants land in the loaded session

**Labels**: medium,save-load,scripting,bug

**Source**: docs/audits/AUDIT_SAVE_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Live Load-Apply (frame boundary) / Snapshot Completeness (allowlist reasons) · **Location**: `byroredux/src/app_events.rs:1103`

- **Data-Loss Class**: corruption-on-load
- **Location**:
  - `byroredux/src/app_events.rs:1103` (`step_save_loads`) before `:1109` (`step_cell_transition`).
  - `byroredux/src/app_step.rs:1210-1213` (`step_cell_transition` drains `step_player_move_request` first, before its loading-screen gate) and `:952-1001`.
  - `byroredux/src/boot/schedule/late.rs:233`, `:247` (`scripted_experience_system`, `scripted_skill_system`) versus the Late fragment runners registered after them, `:441` (`papyrus_provider_system`), `:543` (dialogue selection), `:848` (`obscript_item_system`).
  - `crates/scripting/src/fragment/effects/deferred.rs:823`, `:853-861` (`PendingSkillAdvances` / `PendingPlayerXp` flush).
  - Allowlist rows `registry_completeness_tests.rs:517`, `:557`, `:711`.
- **Status**: NEW
- **Description**: The load drain runs between the scheduler and the rest of the App steppers. There are two leak paths.
  1. **`PlayerMoveRequest`** (`Option<u32>` placement FormID). A fragment that runs `Game.GetPlayer().MoveTo(x)` (Papyrus or ObScript `MoveTo player`) writes it during the scheduler run. It is consumed only by `step_player_move_request`, at the top of `step_cell_transition`, which runs **after** `step_save_loads` in the same `about_to_wait`. When a load drains in that frame (F9, console `load`, `EssentialDeathReload`, or the frame a loading cover finishes presenting), the outgoing session's FormID is resolved in the **loaded** world: the player is moved in place, or a transition to the outgoing session's destination is queued. That happens after `apply_player_pose` has just restored the saved pose. The allowlist reason ("drained … the frame it is written") is true only of a frame without a load.
  2. **`PendingPlayerXp` / `PendingSkillAdvances`** (amounts, skill names). The deferred-effect batch flushes them where the fragment runs. Their consumers are Late systems registered before three Late fragment runners. A grant raised by those runners waits for the next frame's consumer and is applied to the loaded player's saved `CharacterLevel` / `LevelUpPoints` / `SkillProgress` / `ActorValues`. The `PendingPlayerXp` row admits "the same or the next frame". The `PendingSkillAdvances` row ("drained the same frame") is false.
- **Evidence**:
  - `app_events.rs:1093-1109` order: `capture_player_pose` → `step_player_save_actions` → `step_save_loads` → `step_cell_transition`.
  - `step_player_move_request` takes `request.0` unconditionally, then resolves the FormID against the current world.
  - `execute_pending_save_loads` discards CLOC / story / package / unload state (`live_load.rs:608-620`) but no request queue.
- **Impact**:
  - The trigger window is one frame. The outcome is cross-timeline contamination that the next save persists: a teleport or cell change away from the loaded pose, or unearned XP or skill progress.
  - It is the same class as #5435 / #6083. Each of those was fixed one queue at a time, and no guard enumerates the rest. Other same-shape value-keyed queues (`PendingSleep`, `PendingMusic`, `PendingScriptMessages`, `MessageBoxQueue`) have the same exposure where their producer runs after their consumer.
- **Related**: #5435, #5864, #6083, SAVE-2026-10-21-02.
- **Suggested Fix**:
  - Add one `discard_outgoing_session_requests(world)` beside the existing discards. It should clear `PlayerMoveRequest`, `PendingPlayerXp`, `PendingSkillAdvances`, `PendingSleep` and the other FormID/value-keyed `Pending*` request resources.
  - Add a `live_reload_tests` source guard that lists every `NOT_SAVED_BY_DESIGN` row whose reason says "drained"/"consumed" and requires it to be either discarded in the drain or marked entity-keyed (stale ids never resolve, because ids are not recycled).
  - Correct the three allowlist reasons.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
