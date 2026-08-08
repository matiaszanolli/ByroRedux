# SAVE-D6-01: QuestAliasInjectionState.inventory_grants embeds session-local EntityIds in a Resource the M45.1 live-load path never remaps -- every load re-grants already-owned quest-alias inventory items

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2534
**Finding ID**: SAVE-D6-01

**Severity**: HIGH
**Dimension**: 6 — M45.1 Live Load-Apply (also independently surfaced as a Validation-Gates coverage gap; merged into one finding since both describe the identical root cause and field)
**Data-Loss Class**: reference-break (manifests as item duplication, not loss)
**Location**: `crates/scripting/src/scene.rs:162-174` (struct def — `inventory_grants` field, no `#[serde(skip)]`, unlike sibling `factions`), `:668-708` (`apply_alias_injections`'s dedup-by-tuple grant loop), `byroredux/src/save_io.rs:328-332` (`register_resource::<QuestAliasInjectionState>`), `crates/save/src/driver.rs:148-167` (`restore_resources` — wholesale verbatim resource replace by design, no remap parameter exists for resources)
**Status**: NEW

## Description
`QuestAliasInjectionState` (added this cycle by the quest-lifecycle commit `a844c26b`) is registered as a save `Resource`:
```rust
pub struct QuestAliasInjectionState {
    #[cfg_attr(feature = "save", serde(skip, default))]
    factions: HashMap<(EntityId, u32), InjectedFactionMembership>,
    inventory_grants: HashSet<(QuestFormId, i32, EntityId, u32, u32)>,
}
```
`factions` is correctly `serde(skip, default)`'d, with its doc comment explaining why: "reconstructed from the immutable alias definitions on load." `inventory_grants` has no such skip, and **does** serialize a raw session-local `EntityId` (a `u32` ECS index) inside every tuple — the same hazard class `#1696` excluded `AnimationPlayer.root_entity` for. The difference: components with this hazard stay off `MUTABLE_DELTA_COLUMNS`, which genuinely protects them from being overlaid with stale ids. `QuestAliasInjectionState` is a *resource*, and **resources have no remap mechanism at all** — `restore_resources` calls `load(world, value.clone())` verbatim for every registered resource, on the explicit design assumption "resources aren't entity-keyed, so they're replaced outright rather than remapped". That assumption is false for this one resource.

`apply_alias_injections` dedups by inserting the full tuple `(quest, alias, entity, item, count)` into the set and skipping the grant if the insert reports "already present." On a live load: (1) `restore_resources` installs the *saved* `inventory_grants`, keyed by the *previous* session's entity ids. (2) `load_cell_with_masters` respawns every entity fresh. `EntityId` allocation is monotonic and never reclaimed, and no `set_next_entity` call exists anywhere in the live-load path (that's a `restore_world`-only operation, never called from the live binary — confirmed via full-tree grep). The reloaded cell's entities get ids continuing from wherever this session's counter currently sits — structurally guaranteed **not** to match the saved ids. (3) The unconditionally-scheduled `quest_alias_refresh_system` runs `apply_alias_injections` on the next tick. Every alias resolves to its **new** entity id, so the dedup `.insert((quest, alias, NEW_entity, item, count))` finds no match against the restored (old-entity-id) set — returns "not a duplicate" — and the item is granted again. This repeats on every subsequent `load`, with no convergence, because each reload assigns yet another fresh id.

## Evidence
```rust
// crates/scripting/src/scene.rs:677-688 — apply_alias_injections
for (quest, alias, entity, item, count) in desired_inventory {
    if !next_grants.insert((quest, alias, entity, item, count)) {
        continue;   // "already granted" — but `entity` never matches post-reload
    }
    if let Some(inventory) = inventories.get_mut(entity) {
        inventory.push(ItemStack::new(item, count));   // re-granted
    }
}
```
The existing coverage test, `quest_alias_inventory_grant_ledger_survives_snapshot_round_trip`, does **not** exercise this bug: it spawns exactly one actor in a fresh `World` before doing anything else and asserts `restored_actor == actor` — the one scenario (identical spawn ordering, identical starting `next_entity`) where ids coincidentally line up, which the real M45.1 path (reloading into a `next_entity` counter that has already advanced past the save's) never produces.

## Impact
Quest-alias-injected permanent inventory (narrative/reward items from vanilla `SetStage`/alias-fill quests) duplicates on every live `load` of a save where such a grant has already resolved — an exploitable item-dupe path reachable through the ordinary `load <slot>` console command, not an edge case, and a direct violation of `QuestAliasInjectionState`'s own stated idempotency purpose.

## Suggested Fix
Either (a) thread a remap parameter into `restore_resources` for this one resource (a shape change — resources currently assume no entity keys — reusing the same `HashMap<u32,u32>` `build_form_id_remap` already produces for components), or (b), simpler and consistent with `factions`' own precedent: `serde(skip, default)` the `inventory_grants` field too, and re-derive "already granted" from the entity's live `Inventory` contents on the first post-load `apply_alias_injections` pass instead of a saved entity-id ledger — a live cell reload always respawns authored REFRs fresh, so the ledger doesn't need entity-id continuity if it's keyed off content instead of identity. Add a regression test that goes through the actual `execute_pending_save_loads` shape (spawn unrelated entities before the alias actor so `next_entity` has already advanced, the way a real reload does) rather than the current same-session, same-entity-id test.

## Completeness Checks
- [ ] **TESTS**: A regression test spawns unrelated entities before the alias actor (advancing `next_entity` past the save's counter) and confirms no duplicate grant on reload
- [ ] **CANONICAL-BOUNDARY**: If the fix touches the resource-restore path, the "resources aren't entity-keyed" assumption is either upheld (option b) or the remap mechanism is documented as a deliberate, narrow exception (option a)
