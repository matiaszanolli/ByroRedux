# #3415 — FNV-2026-08-27-D1-01: `LoadedCellIndex` is installed only by the interior loader, so every door in an FNV exterior worldspace fails to transition

**Labels**: high, bug, esm-plugin, terrain-exterior, game:fnv, legacy-compat

**Filed**: 2026-08-27 · from `docs/audits/AUDIT_FNV_2026-08-27.md`

---

**Source**: `docs/audits/AUDIT_FNV_2026-08-27.md` — finding `FNV-2026-08-27-D1-01` (HEAD `969d81c8`)

- **Severity**: HIGH
- **Dimension**: 1 — Cell Loading End-to-End
- **Location**: `byroredux/src/cell_loader/load.rs:568` (sole producer) · `byroredux/src/cell_loader/transition.rs:271-281` (consumer) · `byroredux/src/scene.rs:816-858` (the two boot arms)

## Description

`queue_door_transition` — the single producer used by both player activation (`interaction.rs:989`) and the `door.teleport` console command (`commands/scene.rs:451`) — resolves a door's XTEL destination REFR to its parent cell through the `LoadedCellIndex` World resource. That resource is inserted at exactly one site in the whole tree, and that site is inside `load_cell_with_masters`, the **interior** entry point. `scene.rs`'s exterior arm (`--esm <path> --grid <x>,<y>`, i.e. the WastelandNV route) drives `WorldStreamingState` instead and never calls it, so on an exterior boot the resource is absent for the whole session.

## Evidence

The only insert in the tree:

```
$ grep -rn "LoadedCellIndex" byroredux/src/
byroredux/src/cell_loader/index.rs:37:pub struct LoadedCellIndex(pub Arc<EsmCellIndex>);
byroredux/src/cell_loader/load.rs:568:    world.insert_resource(super::LoadedCellIndex(std::sync::Arc::new(index.cells)));
```

`byroredux/src/cell_loader/transition.rs:271-278`:

```rust
let index = world
    .try_resource::<super::index::LoadedCellIndex>()
    .ok_or(QueueDoorTransitionError::MissingCellIndex)?;
```

and its error text, `transition.rs:235-238`:

```rust
Self::MissingCellIndex => write!(
    formatter,
    "no LoadedCellIndex resource; an ESM-driven cell load is required"
),
```

Doors are attached on the **shared** REFR spawn path (`byroredux/src/cell_loader/spawn.rs:815-828` stamps `DoorTeleport` whenever a REFR carries an XTEL destination), so exterior doors do become interaction candidates (`interaction.rs:877-882`) and reach `activate_target`. The failure is swallowed as a `log::warn!` (`interaction.rs:998-1005`).

Independently corroborated by `4dcbd187`'s own commit body: *"the exterior -> interior leg could not be driven, because `LoadedCellIndex` is installed only by `load_cell_with_masters` and never by the exterior streaming path, so activating an exterior door on a `--wrld/--grid` boot logs 'no LoadedCellIndex resource; an ESM-driven cell load is required' and no transition occurs."* It says the gap is *"worth its own issue"*; no issue existed.

## Impact

On FNV's reference open-world route the player can walk up to any building, saloon, vault or metro door and activate it, and nothing happens beyond a warning line. Every exterior→interior traversal on the reference title is unreachable, which also means the whole door-walk transition orchestrator (`transition.rs`), its cell-swap unload path, and #3323's new interior window-portal sky lane are exercised only from an interior boot. `LoadedPluginSet` — the *other* prerequisite the same function needs — **is** inserted on both arms (`scene.rs:803-806`), so this is a single missing insert, not a design gap: the exterior context already holds the full parsed index as `wctx.record_index.cells`.

## Related

`4dcbd187` (#3323); `byroredux/src/cell_loader/transition.rs:653-672` (`queue_door_transition_reports_missing_prerequisites` already asserts the `MissingCellIndex` arm, so the error is tested but the production gap is not).

## Suggested Fix

Insert `LoadedCellIndex` from the exterior boot as well — the `EsmCellIndex` is already resident as `wctx.record_index.cells` and is `Arc`-shared, so it is a clone of an `Arc`, not a re-parse. Pin it with a test that asserts the resource is present after *both* boot arms, keyed on the boot path rather than on this one resource name.

## Repro

```bash
cd "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data"
cargo run --release --manifest-path /mnt/data/src/gamebyro-redux/Cargo.toml -- \
  --esm FalloutNV.esm --wrld WastelandNV --grid 0,0 --radius 3 \
  --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa"
# walk to any building door, activate → "door queue failed: no LoadedCellIndex resource"
```

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other World resources the transition path needs — `LoadedPluginSet`, transition slots — are installed on both boot arms)
- [ ] **TESTS**: A regression test pins this specific fix
