# #5052 — SAVE-D1-2026-09-29-01: `ActorControlState` is additive and lazily inserted on the process-lifetime player — a restraint from the pre-load session survives loading a save that has no player row

**Labels**: high,save-load,gameplay,bug

**Source report**: `docs/audits/AUDIT_SAVE_2026-09-29.md`
**Severity**: HIGH
**Dimension**: Snapshot Completeness (two lists / replacing semantics) — data-loss class: corruption-on-load

## Location
- `byroredux/src/save_io.rs` — `"ActorControlState"` in `MUTABLE_DELTA_COLUMNS`; `.register_component::<ActorControlState>` (plain additive)
- `crates/scripting/src/fragment/effects.rs` — `Effect::SetPlayerRestrained` / `SetRestrained` inserts `ActorControlState` on first use
- `byroredux/src/systems/character.rs` — `player_accepts_movement_input`
- `crates/save/src/registry.rs` — `register_component` vs `register_replacing_component`

## Description
The player spawns without `ActorControlState`; no production site pre-stamps it. The first `SetRestrained` fragment inserts `{ restrained }` on the player. `save_world` omits an empty additive column, so a save from a session that never ran `SetRestrained` has no player row. The player entity survives the load's teardown, `apply_deltas` only writes rows that exist, and no reconciler touches the column — a live `restrained: true` stays on the player after loading that save.

## Evidence
The only production insert is the `query_mut::<ActorControlState>` insert in `effects.rs` (grep across `byroredux/src`, `crates/scripting/src`, `crates/core/src`; the `character.rs` hit is test code). The overlay is additive per `registry.rs`. The post-overlay reconcilers cover `Dead` NPCs and the equipped weapon only.

## Impact
After loading, the player cannot move: `player_accepts_movement_input` returns false until a later script calls `SetRestrained(false)`. If the loaded save's quest state never re-runs that fragment, the session cannot be recovered short of a restart. Trigger: a save from a session that never used `SetRestrained` (e.g. booted past Helgen), a restraining sequence in the current session (Skyrim MQ101 restrains the player), then load the earlier save.

## Related
GAME-D7-2026-09-29-01 (#5027) (`Dead`, same class); SAVE-D5-2026-09-29-02 (#5056), SAVE-D1-2026-09-29-02 (#5058), GAME-D1-2026-09-29-02 (#5034), SAVE-D5-2026-09-29-01 (#5054) (same save-load-teardown survival theme); #2292 (why the column is registered); #3488 (closed, same class for `EquippedWeapon`).

## Suggested Fix
Register `ActorControlState` with `register_replacing_component` (saved absence on a FormID-matched actor means "not restrained"; NPCs respawn without it anyway). Better: fix the class once for `Dead` and this column together. Test: live player restrained → load a save with no row → not restrained.

Validated at HEAD 9fcfdc3fc: `ActorControlState` uses plain `register_component` and is in `MUTABLE_DELTA_COLUMNS`; the only production insert is the fragment effect; no reconciler clears it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
