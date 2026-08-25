# ByroRedux SDK

`byroredux-sdk` is the renderer- and UI-independent foundation for tools built
on ByroRedux. It deliberately contains contracts and algorithms, not Vulkan,
egui, archive IO, or the game executable.

The first module is `studio`:

- `AssetBounds`, `BoundSphere`, and `CornellFit` provide deterministic,
  testable scene fitting.
- `StudioSession` is persistent document state: source identity, editable ECS
  object IDs, selection, original transforms, and revision.
- `StudioSnapshot` and `ObjectSnapshot` are immutable projections for any UI.
- `StudioCommand` is the mutation protocol shared by GUI, CLI automation, and
  future plugins.
- `pick_spheres` is a host-neutral center-ray selector.

The `byroredux` binary is the first host adapter. It imports NIF/SPT assets from
loose files or the existing BSA/BA2 and game-profile providers, constructs the
room, turns ECS state into snapshots, and applies commands after UI rendering.
The egui crate never mutates the world directly.

Run the current tool with:

```bash
cargo run --release -- --studio path/to/asset.nif
cargo run --release -- --game skyrim_se --studio \
  --mesh 'meshes\\clutter\\ingredients\\sweetroll01.nif'
```

This is intentionally an initial SDK surface, not a stability promise for the
entire engine. New inspectors, gizmos, asset browsers, undo/redo, serialization,
and plugin bindings should extend the typed document/command boundary instead
of coupling to the executable or egui callbacks.
