# SCR-D7-2026-08-16-02: the trigger-volume spawn branch runs for non-primary synthetic children and builds each volume from the outer REFR's XPRM at the child's transform

Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-16.md` (Dimension 7 — Engine Attach & Trigger Wiring).

**Location**: `byroredux/src/cell_loader/references/synth_child.rs`:145-171

## Description

Inside `spawn_synth_child`'s invisible-trigger branch, `stamp_quest_reference` is correctly gated on `is_primary_synth` (:152-154), but **the entity spawn, the `TriggerVolume` insert, `attach_script_for_refr` and `accum.trigger_volumes += 1` are not**.

The volume is built from `placed_ref.primitive` — a property of the **outer** REFR — composed with the **child's** `(ref_pos, ref_rot, ref_scale)`.

`has_script` (:133-144) is satisfied for a non-primary child by its own base record's `base_record_script` / `base_record_script_instance`, so the branch is genuinely reachable with `is_primary_synth == false`.

## Evidence

```rust
if !has_mesh && has_script {
    if let Some(prim) = placed_ref.primitive.as_ref() {           // outer REFR's XPRM
        if let Some(volume) = trigger_volume_from_primitive(prim, ref_pos, ref_rot, ref_scale) {
            let entity = world.spawn();                            // ← every child
```

Re-verified 2026-08-17.

## Impact

Every synthetic child of a scripted, mesh-less REFR gets its own trigger volume, each sized from the outer REFR's primitive but positioned at the child's transform. One authored trigger becomes N differently-placed volumes, and `accum.trigger_volumes` over-counts by the same factor.

Trigger detection (`crates/scripting/src/trigger.rs`) then fires on volumes that were never authored.

## Suggested Fix

Decide the intended policy and apply it consistently — most likely gating the whole branch on `is_primary_synth`, matching the `stamp_quest_reference` call it already contains. Record the decision, because #3016 shows the same ambiguity across five sibling branches.

## Related

- #3016 (SCR-D7-2026-08-16-03 — the same gated-vs-ungated inconsistency across five branches)
- #2026 (the outer-REFR VMAD restriction this is orthogonal to)

## Completeness Checks
- [ ] **SIBLING**: Resolved together with #3016 as one policy, not two point fixes
- [ ] **COUNTER**: `accum.trigger_volumes` reflects authored volumes after the fix
- [ ] **TRANSFORM**: Volume extent and transform come from the same REFR, not two different ones
- [ ] **TESTS**: A regression test spawns a multi-child scripted mesh-less REFR and asserts the volume count
