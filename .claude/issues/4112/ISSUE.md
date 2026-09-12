# SCR-D7-2026-09-11-01: Persistent-worldspace logical-actor stubs never route through the script-attach path

URL: https://github.com/matiaszanolli/ByroRedux/issues/4112
Labels: bug, medium, scripting

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring (audit-scripting)
- **Location**: `byroredux/src/cell_loader/exterior.rs:318-345` (`PersistentCellApplyJob::apply`'s logical-stub loop), `:366-408` (`prepare_logical_actor_stubs`), `:1138-1146` (`begin_worldspace_persistent_cell`)
- **Status**: NEW

**Description**

`PersistentCellApplyJob` builds `logical_stub_refs` from `remote_actor_refs` (persistent-CELL actor placements outside the initial streaming radius) plus the subset of `local_refs` that are actors but didn't spawn as a "live" `SceneAliasCandidate`. Every entry is materialized with exactly one call — `spawn_logical_quest_reference` — which stamps `Transform`/`GlobalTransform`/`SceneAliasCandidate` and nothing else. There is no call to `attach_script_for_refr`/`attach_quest_reference_script` anywhere in `exterior.rs` (confirmed by grepping the whole file for `"script"`). Contrast with the ordinary actor path in `references/mod.rs:719-729`, which always follows `stamp_quest_reference` with `attach_quest_reference_script` for the same "no full spawn root" shape. This is a whole code path that never reaches the attach function at all, not a per-lookup decline — it produces zero log output naming the decision, so it's invisible to the "M47.2 scripts:" counter and to the path's own one summary line.

**Evidence**

```rust
// exterior.rs:318-345
while self.next_logical_stub < self.logical_stub_refs.len() {
    let placed = &self.logical_stub_refs[self.next_logical_stub];
    super::references::spawn_logical_quest_reference(
        world, placed, &wctx.load_order,
        super::transition::position_zup_to_yup(placed.position),
        super::transition::rotation_zup_to_yup_quat(placed.rotation),
        placed.scale,
    );
    self.next_logical_stub += 1;
    budget.complete_unit();
}
```

**Impact**

Any vanilla persistent quest actor (Bethesda marks an NPC/CREA "Persistent" specifically so it survives outside normal cell streaming — quest-critical companions, ambush spawners, radiant-quest targets, actors referenced by a quest alias from a remote worldspace location) that also carries its own scripted behavior has that behavior silently absent for the entire time the actor is only a logical stub — which for an actor outside every session's streaming radius, or whose home cell never streams in, is effectively its whole lifetime. Blast radius is every ObScript/Papyrus game with worldspace-persistent-cell content whose persistent actors carry their own scripts; the same population #2664 already identified for the identity/alias-ranking half.

**Related**

Sibling of the now-closed #2664 (identity/alias-ranking half of this same stub); same "recognized architecture, missing wiring at one call site" shape as the now-fixed SCR-D7-2026-09-06-01 (statics-family `SCRI` gap), three sessions apart.

**Suggested Fix**

After `spawn_logical_quest_reference` in the `logical_stub_refs` loop, call `attach_quest_reference_script` (or `attach_script_for_refr` directly) with `placed.base_form_id` and `placed.script_instance.as_ref()`, incrementing a counter surfaced in the existing summary line so a fix here is observable without a game-data run.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other actor-identity-stamping call site in the engine has the same missing-attach gap (the ordinary-actor path in `references/mod.rs:719-729` is the reference to match)
- [ ] **TESTS**: A regression test pins that every `logical_stub_refs` entry with a script instance gets attached

Source: `docs/audits/AUDIT_SCRIPTING_2026-09-11.md`
