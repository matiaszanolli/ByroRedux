# SAVE-D1-09: Player-control-lock state (PlayerControlState/ActorControlState) absent from build_save_registry

**Filed from**: `docs/audits/AUDIT_SAVE_2026-08-03.md`
**Labels**: medium, ecs, bug

**Severity**: MEDIUM
**Dimension**: Snapshot Completeness & Determinism
**Data-Loss Class**: silent-drop
**Location**: `crates/scripting/src/player_control.rs:44-56` (`PlayerControlState`, a `Resource`), `:110-117` (`ActorControlState`, a `Component`), `crates/scripting/src/translate/effects.rs:589-606` (`prim_player_controls`), `byroredux/src/save_io.rs:188-249` (absent from registry)

## Description
Skyrim quest-intro scripts routinely call `Game.DisablePlayerControls(...)`/`Game.EnablePlayerControls(...)` and `Actor.SetRestrained(...)`. `translate/effects.rs` recognizes both call families (live via `fragment.rs`'s `lower_fragment`) and writes them into `PlayerControlState` (Resource, default all-enabled) and `ActorControlState` (per-actor component, default `restrained: false`). Neither is in `build_save_registry`. A save taken mid-scripted-sequence reloads with both silently reset to defaults.

## Impact
Narrower than SAVE-D1-08 (#2291) — window is only "mid-cutscene, controls locked." Self-correcting in the common case, but a save taken in the exact locked window loses the lock/restrain flag.

## Suggested Fix
`.register_resource::<PlayerControlState>("PlayerControlState")` and `.register_component::<ActorControlState>("ActorControlState")`; both delta-safe for `MUTABLE_DELTA_COLUMNS` too.

Classification at filing time: NEW, CONFIRMED against current HEAD via direct grep of `player_control.rs` struct defs and `save_io.rs`'s registration chain.
