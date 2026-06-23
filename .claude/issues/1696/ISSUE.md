# SAVE-D6-01: apply_deltas remaps the row key but not EntityId/handle fields inside the component value

Labels: bug import-pipeline high 

- **Severity**: HIGH
- **Dimension**: M45.1 Live Load-Apply
- **Data-Loss Class**: reference-break
- **Location**: `crates/save/src/registry.rs:104-121` (the component `ApplyFn`); component fields at `crates/core/src/animation/player.rs` (`root_entity`, `clip_handle`), `crates/core/src/animation/stack.rs` (`root_entity`, `clip_handle`); overwrite confirmed against `byroredux/src/cell_loader/spawn.rs`

## Description
The live load overlays each `MUTABLE_DELTA_COLUMNS` column via the `ApplyFn`, which does `filter_map(|(old, comp)| remap.get(&old).map(|&live| (live, comp)))` — it remaps the row **key** (`old` saved id → `live` id) but moves `comp` **verbatim**. Components whose serialized value embeds an `EntityId` or a session-local registry handle are therefore applied with stale references:

- `AnimationPlayer.root_entity` / `AnimationStack.root_entity` (`Option<EntityId>`) hold a SAVED-session entity id, meaningless in the freshly reloaded cell (fresh ids).
- `AnimationPlayer.clip_handle` / `AnimationLayer.clip_handle` (`u32`) index the `AnimationClipRegistry`, which is session-local and not guaranteed stable across a reload.

The cell loader sets the *correct* fresh `root_entity` when it respawns the entity; the saved delta then **overwrites** that correct value with the stale one (`insert_bulk` is last-writer-wins on the remapped id).

## Evidence
`ApplyFn` body moves `comp` unchanged; `AnimationPlayer`/`AnimationStack` both carry `root_entity: Option<EntityId>` + `clip_handle: u32`; both are in `MUTABLE_DELTA_COLUMNS`; the animation system scopes name lookups to `root_entity`'s descendants.

## Impact
Any reloaded animated actor/object that had a non-`None` `root_entity` or a meaningful `clip_handle` at save time gets its animation broken on a live `load`: name-scoped channel lookups target a wrong/absent subtree, or the clip resolves to nothing/another clip. Blast radius = every animated entity overlaid by a live load. The crate's `delta_apply_reroutes_by_form_id_after_cell_reload` test only covers `Transform`/`Inventory` (no embedded refs), so this is unguarded.

## Suggested Fix
Either (a) exclude `AnimationPlayer`/`AnimationStack` from `MUTABLE_DELTA_COLUMNS` and let the reloaded cell own them (their post-spawn animation state is largely transient), or (b) teach the `ApplyFn`/`register_component` path to also remap declared inner `EntityId` fields (a per-type "remap hook"), and re-resolve `clip_handle` by clip identity rather than index. (a) is the low-risk fix; (b) is the general one.

## Completeness Checks
- [ ] **SIBLING**: Same un-remapped-inner-field hazard checked for any other delta column carrying an `EntityId`/session handle
- [ ] **TESTS**: A regression test exercises a live `apply_deltas` of `AnimationPlayer`/`AnimationStack` and asserts the reloaded subtree's `root_entity` survives
