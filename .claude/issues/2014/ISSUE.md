# SAVE-D1-NEW-01: Seven M42 AI-procedure runtime-state components are absent from the save registry

**Labels**: high, ecs, bug

**Severity**: HIGH
**Dimension**: Snapshot Completeness & Determinism
**Source**: `docs/audits/AUDIT_SAVE_2026-07-16.md`

## Location
`crates/core/src/ecs/components/{wander,travel,follow,escort,guard,patrol,sandbox}.rs`; `byroredux/src/save_io.rs:162-208` (`build_save_registry`)

## Description
The seven M42 AI-package procedure runtimes (Wander/Travel/Follow/Escort/Guard/Patrol/Sandbox), all landed after this audit's 2026-07-03 cutoff, each pair a spawn-time `*Behavior` marker (correctly rederived from ESM `PACK` data, analogous to the existing `REDERIVED_NOT_SAVED` allowlist) with a runtime `*State`/terminal-marker component that the owning system mutates every tick or on completion. None of `WanderState`, `PatrolState`, `GuardState`, `FollowState`, `EscortState`, `TravelState`, `Traveled`, `Escorted`, `Seated` appear in `build_save_registry`. The existing `#1835` structural guard (`npc_spawn_stamped_components_are_saved_or_intentionally_rederived`) does not catch this: it only audits components `spawn_npc_entity` itself stamps, and these types are inserted lazily by their own systems on a later tick.

Verified current: `build_save_registry` (byroredux/src/save_io.rs:162-208) registers Transform, Name, Parent, Children, Inventory, EquipmentSlots, LightSource, LightFlicker, AnimationPlayer, AnimationStack, ScriptTimer, ActorValues, FormIdComponent, and 5 resources — none of the seven AI-procedure runtime-state components appear.

## Evidence
```rust
// crates/core/src/ecs/components/travel.rs
pub struct Traveled; // terminal one-shot: NPC has arrived, travel_system should stop
// byroredux/src/systems/travel.rs:194
tq.insert(d.entity, Traveled);
```

## Impact
Continuously-updated state (`WanderState`/`PatrolState`/`GuardState`) is self-correcting on reload (cosmetic AI-continuity reset). The sharper edge is the terminal one-shot completion markers (`Traveled`/`Escorted`/`Seated`): losing these on save→load makes an NPC that has *already finished* its Travel/Escort/Seat behavior silently redo it — an arrived Travel NPC walks to its destination again, a completed Escort NPC restarts the collect+lead sequence. Blast radius is bounded today: all seven procedures are gated one-per-env-var, none in the default scheduler — but rated HIGH per "impact, not likelihood" since it's a real, non-recoverable regression the moment any flag is set, on a shipped user-facing feature.

## Related
Sibling failure class to closed `#1834`/`#1835` (ActorValues), but not caught by that guard because these are system-inserted, not spawn-inserted.

## Suggested Fix
Register the terminal markers and position/phase-only state (all plain `Vec3`/enum/`u32`, no `EntityId`) in `build_save_registry` and add the delta-safe ones to `MUTABLE_DELTA_COLUMNS`. Do **not** add `FollowState`/`EscortState`/`Seated` to `MUTABLE_DELTA_COLUMNS` — they carry `EntityId` fields (`target_entity`, `furniture`) with the same session-local-reference hazard `#1696` already excluded `AnimationPlayer.root_entity` for; they can still ride full `register_component` (`restore_world` preserves entity ids verbatim) but not the live delta overlay. Extend `delta_columns_carry_only_session_stable_fields`'s audited list deliberately, per its existing discipline.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **SAVE-REGISTRY**: Component added to `build_save_registry` AND to the `#1714` regression-guard's file-scan list (`SAVE_TYPE_SOURCES`)
- [ ] **TESTS**: A regression test pins this specific fix
