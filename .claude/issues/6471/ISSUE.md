# LC-2026-10-27-D2-01: Spawn plays a model's first NiControllerSequence instead of its idle, so animated doors and containers load open and activators load activated

Labels: high, legacy-compat, animation, bug, game:fnv, game:fo3, game:oblivion, game:skyrim
Source: docs/audits/AUDIT_LEGACY_COMPAT_2026-10-27.md

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-27.md` finding `LC-2026-10-27-D2-01` (Dimension 2, legacy subsystem coverage: `NiControllerManager` sequence activation) · Severity HIGH · Status NEW (no open or closed issue covers selection; #3602, #6253, #6254, #4128 are closed and kept this behaviour). Games: FNV, FO3, Oblivion, Skyrim SE. Static + raw-data probe; not observed in a running engine.

**Location (current)**: `byroredux/src/cell_loader/spawn.rs:953-975` (`first_sequence = handles.first().copied()` then `spawn_placement_player`); `byroredux/src/cell_loader/object_sequences.rs` (module doc "plays the first on spawn"; `named_sequences_for` keeps `import_kf` order); sequence order from `crates/nif/src/anim/entry.rs::import_kf` (follows `NiControllerManager::sequence_refs`).

## Description
A placed object's spawn starts whichever sequence the manager lists first. The legacy engines treat that list as a catalog:

- **Gamebryo.** `NiControllerManager` holds sequences "for future playback"; a sequence runs only when activated (`NiControllerManager_overview.htm`).
- **Bethesda.** An object's *idle* is what starts at load. The GECK Door/Activator form says "Random Anim Start: … this object kicks off its idle animation at a random start time". `Open` runs "when activated", and `Forward`/`Backward` are `PlayGroup` targets.
- **OpenMW.** Non-actors start in `CharState_Idle`, which plays the `idle` group.

In vanilla data the first sequence is usually an activation clip with Clamp cycle, so playing it at spawn leaves the object in its post-activation pose. (Note: the in-code comment records that #3602 deliberately plays the first sequence so Oblivion gates/machinery and FNV flags are not spawned frozen; the fix must keep those idle-driven cases working by selecting the idle, not by dropping spawn playback.)

## Evidence
Probe: `byroredux_nif::anim::import_kf` over the vanilla meshes BSAs.

| | FNV | FO3 | Oblivion | Skyrim SE |
|---|---:|---:|---:|---:|
| Meshes with sequences | 803 | 411 | 423 | 1,137 |
| First is neither `Idle` nor `SpecialIdle` | 543 | 270 | 314 | 1,033 |
| First is `Open:Clamp` | 143 | 148 | 86 | 245 |
| First is `Forward:Clamp` | 156 | 64 | 113 | — |
| First is not an idle, but an idle appears later | 31 | 3 | 4 | — |

Samples: FO3 `dungeons\vaultruined\doors\vdoorslidingvault106.nif` (`Open:Clamp:1.33`), `dungeons\enclave\accessories\lockerenclave01.nif`, `clutter\vendortrunk\vendortrunk01.nif`; FNV `dungeons\vaultruined\doors\vgeardoorr106.nif` (`Open:Clamp:25.00`), `clutter\clfence\clfencegate01.nif`, `trees\barrel_cactus\nv_barrelcactus.nif` and `landscape\plants\nvpinto_fruited.nif` (`Forward`, harvest); Oblivion `architecture\bravil\bravilloaddoorlowerint02.nif` (`Open`).

Code path (current):
```rust
let mut first_sequence = None;
...
    let handles: Vec<u32> = sequences.0.iter().map(|(_, handle)| *handle).collect();
    first_sequence = handles.first().copied();      // no name test
...
for handle in clip_handle.into_iter().chain(first_sequence) {
    crate::anim_convert::attach_clip_sinks(world, handle, ctx, tex_provider, placement_root);
    spawn_placement_player(world, placement_root, handle);
}
```
No system plays `open` / `close` / `forward` by name; `NamedSequences::handle` is called only by `obscript_anim::play_group`.

## Impact
- **Doors and containers.** Every animated door, gate, locker, safe, trunk and desk in FO3, FNV and Oblivion animates open on cell load and holds open. Collision stays where it was authored, so visual and collider disagree.
- **Activators.** `Forward`-first activators play their activation once and hold the end pose: traps, switches, harvestable plants (shown already harvested), slot machines, effect meshes like `hchambergroundfog`.
- **Idles.** Objects whose idle is not first lose their ambient loop.
- **Skyrim SE.** Graph-driven props also run this player, because the spawn path does not gate on `behavior_graph_file`.

## Related
#3602, #6253, #6254, #4128 (all closed); LC-2026-10-21-D2-01/-02. Interacts with `PlayGroup` flag 0: `cycle_in_progress` treats a clamped-finished `Open` as done, so the first scripted `PlayGroup` is not held back by it.

## Suggested Fix
Select the spawn sequence by name: `idle`, then `specialidle` (case-insensitive). Otherwise start no sequence player, and leave the bind (rest) pose plus the standalone controllers. Leave the activation clips (`Open`/`Close`/`Forward`/…) to their consumers: door/container activation, `PlayGroup`, and a "default open" door state. Pin with a test over a door-shaped fixture whose first sequence is `Open`.

## Completeness Checks
- [ ] **SIBLING**: Other callers that treat `handles.first()` / the first `NamedSequences` entry as the default (precombined spawn, NPC/creature spawn, save-restore) checked
- [ ] **CANONICAL-BOUNDARY**: Selection by name lives in the cell-loader/NIF-import boundary, not per-game branches at render time
- [ ] **TESTS**: A regression test pins a door-shaped fixture whose first sequence is `Open` (no player at spawn) and a model with a later `Idle` (idle plays)
- [ ] **REGRESSION-GUARD**: #3602's cases (Oblivion gates/machinery, FNV flags) still animate after the change

