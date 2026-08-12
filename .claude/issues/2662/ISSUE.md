# #2662: SCR-D7-NEW11-01: Actor REFRs never run the script-attach path -- NPC_ base VMAD and ACHR-own VMAD are silently dropped for every spawned actor

**Severity**: MEDIUM
**Dimension**: Engine Attach & Trigger Wiring (Dimension 7)
**Untrusted-Input**: No
**Location**: `byroredux/src/cell_loader/references/mod.rs:539-606` (the actor branch, which `continue`s at `:605` after only `stamp_quest_reference`); `crates/plugin/src/esm/records/index.rs:615-620` (the `npcs`/`creatures` arms of `base_record_script_instance`); `byroredux/src/cell_loader/references/attach.rs:158-187` (`attach_script_for_refr`)
**Status**: NEW

## Description

`attach_script_for_refr` has exactly three call sites, all reachable only from `spawn_synth_child` (directly at `:1220` and `:1664`, and via `attach_quest_reference_script` at `:1144`).

The actor branch in `load_references_budgeted` intercepts any `child_form_id` present in `record_index.npcs` **before** `spawn_synth_child` is called, drives `NpcSpawnJob`, stamps the canonical reference identity on the resulting root (`:589-591`), and then `continue`s at `:605`. It never calls `attach_script_for_refr`, and no other code path does either.

Consequently the `npcs` arm of `base_record_script_instance` -- added specifically so scripted actors could attach -- is **unreachable from the live attach path**, and the placed actor's own `ACHR` `VMAD` (decoded into `PlacedRef.script_instance` by `crates/plugin/src/esm/cell/walkers.rs:683`) is likewise never consumed for actors.

## Evidence

Corpus census over real masters (temporary instrument `crates/plugin/examples/tmp_vmad_census.rs`, run then deleted; tree verified clean):

- `Skyrim.esm`: **805 / 5118 `NPC_`** and **822 / 10504 `ACHR`** records carry a `VMAD` (samples: `MQ304LostSoulSons3`, `dunTransmogrifyDremora`, `OgolRef`, `GularzobRef`).
- `Fallout4.esm`: **382 / 3015 `NPC_`** and **516 / 7615 `ACHR`**.

Static call graph: `grep -rn "attach_script_for_refr\|attach_vmad_scripts"` across `byroredux/src` and `crates/` returns only `references/{mod,attach}.rs` and their tests; `npc_spawn.rs` and `npc_spawn/` contain no script wiring at all. The only `continue` path out of the actor branch is `:604-605`; the only stamp is `:590`.

`CREA` REFRs are not affected the same way (they are absent from `record_index.npcs`, so they fall through to `spawn_synth_child` and do reach the `creatures` arm) -- but `CREA` is a pre-Skyrim record type that never carries `VMAD`, so that arm is dead in practice too.

## Impact

Every VMAD-scripted actor in Skyrim SE / FO4 / Starfield content loads with zero canonical script behaviour attached, and never contributes to the `M47.2 scripts: N REFRs recognized` counter -- so the smoke gate cannot observe the gap either.

Silent decline (no wrong game state), but it removes the single largest non-`ACTI` VMAD population from the recognizer chain's reach, and it blocks the M47.3 arc directly: the alias-fill runtime binds actor entities (`SceneActorBindings`) whose attached Papyrus behaviour can never fire.

Same class and severity as the item-family gap #2189, which was filed MEDIUM and is now fixed.

## Related

#2189 (closed -- the item-family half of the same structural omission, one layer down); #2567 (open -- creature placements never route through the actor spawn pipeline; distinct issue, same file region); SCR-D7-NEW11-02 (the other half of `base_record_script_instance`'s unreachable surface)

## Suggested Fix

In the `NpcSpawnProgress::Complete` arm, alongside the existing `synth_idx == 0` stamp, call `attach_quest_reference_script(world, root, child_form_id, record_index, refr_script_instance, &mut job.accum)` using the same `refr_script_instance_for_synth_child(synth_idx, placed_ref.script_instance.as_ref())` value `spawn_synth_child` receives, so actors go through the identical additive REFR-then-base VMAD merge and feed the same counter.

Add a test that a scripted `NPC_` REFR with a script archive present increments `scripts_recognized`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
