# SAVE-D1-08: TwoStateActivator + ScriptVariables — live script-driven per-object state — absent from build_save_registry

**Filed from**: `docs/audits/AUDIT_SAVE_2026-08-03.md`
**Labels**: high, ecs, bug

**Severity**: HIGH
**Dimension**: Snapshot Completeness & Determinism
**Data-Loss Class**: silent-drop
**Location**: `crates/scripting/src/vm_state.rs:20-58` (struct defs), `:129` (`two_state_activator_system`), `crates/scripting/src/translate/recognizers/two_state_activator.rs` (recognizer), `byroredux/src/boot.rs:681` (scheduler wiring), `byroredux/src/save_io.rs:188-249` (`build_save_registry`, neither type present)

## Description
`default2StateActivator` is a real, ubiquitous vanilla Skyrim Papyrus script class (levers, switches, portcullis triggers, puzzle doors). The M47.3 scripting expansion added a recognizer wired into the always-on dispatch table (`crates/scripting/src/translate/mod.rs:51`) that converts real ESM-authored instances into two ECS components — `TwoStateActivator` and `ScriptVariables` — and a system, `two_state_activator_system`, unconditionally registered in the default scheduler at `byroredux/src/boot.rs:681`. Neither type appears anywhere in `build_save_registry`. A player who pulls a lever, flips a switch, or opens a puzzle door driven by this script class, then saves and reloads, will find every such object silently reverted to its ESM-authored default state.

## Impact
Any interactable object recognized as a `default2StateActivator` instance loses its open/closed/animating state across every save/load cycle. Core, expected, always-visible gameplay state.

## Suggested Fix
`.register_component::<ScriptVariables>("ScriptVariables")` and `.register_component::<TwoStateActivator>("TwoStateActivator")` in `build_save_registry`; add both names to `MUTABLE_DELTA_COLUMNS`. Add a round-trip test analogous to `ai_procedure_state_and_terminal_markers_survive_save_load_round_trip`.

## Related
Same class as fixed `#1834`/`#1862`. Distinguished from the deliberately-excluded `FollowState`/`EscortState`/`Seated` pattern (`#1696`) — both new types are plain-data, delta-safe.

Classification at filing time: NEW, CONFIRMED against current HEAD (`1ae86f62`) via direct grep of `vm_state.rs`, `boot.rs:681`, and `save_io.rs`'s `build_save_registry` registration chain.
