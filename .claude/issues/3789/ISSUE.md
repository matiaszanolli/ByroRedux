# #3789 — SAVE-D6-2026-08-30-01: the cell reload consults ReferenceEnableState before restore_resources installs the saved one

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: high, save-load, bug

---

**Audit**: `/audit-save` — `docs/audits/AUDIT_SAVE_2026-08-30.md` (Dimension 6 — M45.1 Live Load-Apply; originates in Dimension 1 resource-restore ordering), HEAD `64f64480`
**Finding ID**: `SAVE-D6-2026-08-30-01`

- **Severity**: HIGH
- **Status**: NEW
- **Data-Loss Class**: corruption-on-load

## Location

- `byroredux/src/save_io.rs:1391-1394` (the reload) vs `:1411` (`restore_resources`)
- `byroredux/src/cell_loader/spawn.rs:444-460` (`placement_is_disabled`) and `:631` (the gate)
- `byroredux/src/cell_loader/references/synth_child.rs:647` + `byroredux/src/cell_loader/precombined.rs:391` (the two `spawn_placed_instances` call sites)
- `crates/scripting/src/fragment.rs:1535` + `byroredux/src/boot.rs:670` (the sole, boot-time installer)
- `crates/scripting/src/translate/effects.rs:803-811` (`prim_disable`)
- `docs/engine/save-load-roundtrip.md:222-224` (the claim this disproves)

Made possible by `265f0c9b` (#3278), which landed after the 2026-08-27 audit and gave a **saved resource** its first load-time consumer.

## Description

`ReferenceEnableState` is registered as a save resource (`save_io.rs:488`) and is the FormID-keyed ledger a Papyrus `Disable()` writes to. Until #3278 nothing read it, so it was pure round-trip state. #3278 added the runtime consumer:

```rust
pub(crate) fn placement_is_disabled(
    world: &World,
    placement_fid: Option<byroredux_core::form_id::FormId>,
) -> bool {
    let Some(fid) = placement_fid else { return false };
    let Some(local) = world
        .try_resource::<FormIdPool>()
        .and_then(|pool| pool.resolve(fid).map(|pair| pair.local.0))
    else { return false };
    world
        .try_resource::<byroredux_scripting::ReferenceEnableState>()
        .is_some_and(|state| !state.is_enabled(local))
}
```

(`cell_loader/spawn.rs:444-460`), consulted per placed REFR at `spawn.rs:631`, *after* the placement root and *before* any mesh, collider or light — so one check suppresses all three.

`execute_pending_save_loads` reloads the cell at `save_io.rs:1392` (interior) / `:1394` (exterior) and calls `restore_resources` only at `:1411`. `byroredux_scripting::register` — the one installer of this resource — runs at boot (`boot.rs:670`), not per cell load, and nothing under `cell_loader/` resets it.

**So the reload's spawn decisions are taken against the live session's ledger, and the saved ledger arrives after every one of them.** `apply_deltas`, which follows, is additive-only by contract and can neither spawn nor despawn.

Two symmetric failures, the first on the *primary* load path:

- **Fresh session, `--load N` or quickload after a restart.** The live ledger is `ReferenceEnableState::default()` — everything enabled. **Every reference the save recorded as disabled respawns with full renderable and collidable content.** The saved fact lands in the resource a moment later, but nothing re-reads it until the *next* cell load, which for a player who just loaded into that cell is not going to happen this session.
- **Same-session load after further `Disable()`s.** A reference disabled *after* the save spawns content-less even though the save says it is enabled, and stays that way for the whole of that cell's residency.

This is not a hypothetical path. `prim_disable` recognises `X.Disable()` straight out of decompiled vanilla Papyrus (`translate/effects.rs:803-811`), and `DeferredEffects::apply` commits it via `state.set_enabled` (`fragment.rs:594-600`). The exterior branch has the identical exposure — `assemble_exterior_streaming` reaches the same `spawn_placed_instances`.

## Evidence

Re-verified at HEAD: `save_io.rs:1391-1394` (the `reload_interior_session` / `reload_exterior_session` branch) and `:1411` (`byroredux_save::restore_resources`) are 17 lines apart with the reload in between, and the comment at `:1409-1410` reads "Restore saved resources (ItemInstancePool) so inventory instance ids resolve, then overlay the form-id-keyed mutable deltas."

`grep -rn "ReferenceEnableState" byroredux/src/cell_loader/` returns only `spawn.rs` — no reset during unload. `grep -rn "byroredux_scripting::register"` returns one production site, `boot.rs:670`.

`docs/engine/save-load-roundtrip.md:222-224` states:

> "Reference visibility is no longer part of that gap: scripted `Disable()` records the stable FormID in the saved `ReferenceEnableState` resource, and **reload**/spawn/render consumers reapply it"

— the reload consumer is real, but it runs first, so the doc asserts precisely the guarantee the ordering does not deliver.

## Impact

The subsystem's whole thesis is that the loaded world equals the persisted world. Here it does not, **on the most common load in the game** (start the engine, load a save). Quest-critical scenery, markers and blockers a quest disabled reappear solid and interactive; nothing logs it, because from the loader's point of view it correctly honoured the ledger it was shown.

The ledger itself round-trips intact, so this is not permanent data loss — but the session the player is handed contradicts their save file, and **the next quicksave re-records the contradicted state as truth**.

## Suggested Fix

**Do NOT simply hoist `restore_resources` ahead of the teardown.** `unload_current_interior`'s inventory sweep (`cell_loader/unload.rs:500-519`) releases `ItemInstanceId`s into whichever `ItemInstancePool` is installed, and doing that to the freshly restored arena would corrupt it — that ordering constraint is *why* `restore_resources` sits where it does.

Two clean options:

- **(a)** Split the restore: install the resources the *spawn path* consults (`ReferenceEnableState`, and any future sibling) immediately after the teardown and before the reload, leaving the rest where they are.
- **(b)** Keep the ordering and re-run the disable gate over the reloaded cell after `restore_resources`, as a reconciler in the `apply_deltas` tail alongside `reconcile_dead_actor_runtime_state` — which is exactly the marker-plus-reconciler contract `apply_deltas`' doc comment prescribes for a persisted fact whose runtime consequence the overlay cannot express.

Option (a) is cheaper and avoids the mid-frame despawn the #3278 comment already flags as out of scope.

Either way: correct `docs/engine/save-load-roundtrip.md:222-224`, and add a guard asserting that **every resource read by `placement_is_disabled`'s call chain is restored before the reload**.

## Related

- #3278 (the consumer that created the ordering requirement)
- #3489 (`Effect::Disable` has no `Enable` counterpart — adjacent one-way-door concern over the same resource, distinct defect)
- #1847 / SAVE-04 (`apply_deltas` additive-only, the reason the overlay cannot compensate)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — any other saved resource the cell-reload spawn path consults (`FormIdPool` is already installed at boot; check future additions), and the exterior branch at `:1394`
- [ ] **LOCK_ORDER**: If a RwLock scope changes around `restore_resources` or the spawn gate, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix — a save recording a disabled REFR, loaded into a fresh world, must not spawn that REFR's mesh/collider/light
