# Per-REFR Geometry-Defect Triage Workflow

**Status**: workflow doc — #1281 Workstream B (geometry/transform).
**Audience**: ByroRedux engine devs investigating a specific mesh that
renders at the wrong place, wrong scale, or wrong orientation in a
loaded cell.

## When to use this

You're staring at a screenshot of a cell where some mesh is visibly
misplaced — a wall is 90° off, a prop is floating, a column is
oversized. The systemic translation-layer questions (NIF transform
fidelity, REFR Euler convention, material/lighting) have been ruled
out by the per-game survey ([per-game-translation-survey.md](./per-game-translation-survey.md)
§4.2 found 0/10 837 FNV architecture matrices carry non-uniform scale).

What remains: **per-REFR placement detective work**. This doc is the
shortest path from "screenshot of a bug" to "the specific REFR + its
authored data."

## Pre-flight: static analysis before launching the engine

Before launching anything, narrow the suspect set with the static
analyzer. Cheap, no Vulkan / GPU required.

```bash
cargo run -p byroredux-plugin --example cell_refr_outliers -- \
    <ESM_PATH> <CELL_EDID> [top_n]
```

Surfaces unusual REFRs along three axes:

- **Scale outliers** (|scale − 1.0| ≥ 0.001) — a `scale = 100` from a
  troll plugin or a `scale = 0.06` cone are immediately visible here.
- **Non-axis-aligned rotation** — Bethesda authors most REFRs at
  multiples of π/2. Anything off-grid is the multi-axis class
  [cell_rot_sweep.rs](../../crates/plugin/examples/cell_rot_sweep.rs)
  pins as mode-sensitive.
- **Position outliers** (> 3σ from cell mean) — editor-marker leaks,
  skybox planes at extreme coordinates, misplaced architecture.

The `Distribution` block at the top tells you what's "normal" for this
cell (typical mean position, stddev, fraction of REFRs at scale 1.0,
fraction axis-aligned).

If the misplaced mesh you're investigating appears in any of the three
outlier tables, you have a strong candidate REFR + base form id.

## Live triage: identify the specific REFR in the running engine

If the static pass didn't surface the mesh (or you want to confirm
which REFR specifically), launch the engine on the suspect cell with
the debug-hold flag:

```bash
cargo run --release -- \
    --esm <ESM> --cell <CELL_EDID> \
    --bsa <Meshes.bsa> --textures-bsa <Textures.bsa> \
    --bench-frames 240 --bench-hold
```

`--bench-hold` keeps the engine alive after the 240-frame warmup so
`byro-dbg` can attach (port 9876).

In another terminal:

```bash
cargo run -p byro-dbg
```

### Sequence to identify the broken mesh

1. **Position the camera near the bug.** Fly to roughly the position
   the broken mesh occupies — eyeball from the screenshot.

   ```
   byro> cam.where               # what's the current camera at?
   byro> cam.pos <x> <y> <z>     # teleport to specific world coords
   ```

   Position outliers from the static analyzer give you `(x, y, z)`
   in cell-local coordinates (Y-up engine space).

2. **Pick the mesh with a ray cast.** Aim the camera at the broken
   surface and pick. This sets `SelectedRef` to the picked entity id.

   ```
   byro> pick                    # raycast from camera, set SelectedRef
   byro> prid                    # echo selected
   ```

3. **Dump material + transform for the picked entity.** `mesh.info`
   prints the full Material shape including `texture_path`,
   `material_path`, `material_kind`, alpha state, `emissive_mult`,
   marker components.

   ```
   byro> mesh.info               # uses SelectedRef
   byro> inspect                 # all components on the entity
   ```

4. **Find nearby refs** for context:

   ```
   byro> near 1000               # list refs within 1000 units of camera
   ```

5. **Cross-reference the form id** against the ESM:

   ```bash
   cargo run -p byroredux-plugin --example probe_form -- <ESM> <FORM_ID>
   cargo run -p byroredux-plugin --example dump_cell_refs -- <ESM> <CELL_EDID> \
       | grep '<form_id>'
   ```

   This gives you the base form id, authored position, rotation, scale.
   Compare those to what the engine actually rendered at.

## Common geometric-defect classes

### 1. Wrong base mesh placed at the right location

Looks like: a wall mesh appears where a different mesh should. Often
plugin load-order conflicts — the same form id is redefined by a
later plugin with different content.

**Triage**: `probe_form` against each plugin in the load order. If
the form id resolves to different STATs across plugins, you've found
the conflict.

### 2. Scale collapse / explosion

Looks like: a mesh is microscopic (scale ≈ 0) or building-sized
(scale > 10).

**Triage**: surfaces in `cell_refr_outliers`'s scale-outlier table.
If the scale is authored that way in the cell, it's an authoring bug
in the plugin. If the scale is 1.0 in the ESM but the engine
renders it wrong, there's a transform-composition bug.

### 3. 90° / 180° rotation flip

Looks like: a wall is on its side, a door is rotated 90° from where
it should be.

**Triage**: surfaces in `cell_refr_outliers`'s rotation-outlier
table. Confirm against `cell_rot_sweep.rs` whether the REFR is
mode-sensitive (XYZ vs ZYX). The 2026-05-26 ZYX-OpenMW fix shipped
in commit `20074410` was the canonical mode flip for FNV — pre-fix
content rendered with mode 0 would show this class.

### 4. Floating-mesh / fall-through-floor

Looks like: a chair is hovering 200 units above the floor; the player
falls through.

**Triage**: for floating-mesh, often a transform-propagation issue
where a parent's translation didn't compose into the child's world
transform. For fall-through, it's almost always missing collision —
particularly the FO4 `bhkNPCollisionObject` class which #1277 Task 1
made visible via `examine_collision_kind`. The render-geometry
trimesh fallback at `cell_loader/spawn.rs::synthesize_static_trimesh`
(commit `15016ee0`) is the safety net for FO4 Architecture meshes.

## What this workflow does NOT solve

- **Per-NIF child-node placement** — if a multi-mesh wall NIF has
  internal children at the wrong relative position, the cell-level
  static analyzer can't see it. Use `dump_transforms` on the specific
  NIF instead.

  ```bash
  cargo run -p byroredux-nif --example dump_transforms -- <BSA_PATH> <NIF_PATH>
  ```

- **Visual rendering defects** (chrome materials, missing textures,
  fog leaks, wrong shadows) — those are render-side / material
  defects. Use `tex.missing`, `light.dump`, `mesh.cache failed` in
  `byro-dbg`. See [material-abstraction.md](./material-abstraction.md)
  for the canonical-material work.

## References

- Parent epic: [#1277](https://github.com/matiaszanolli/ByroRedux/issues/1277)
- This workstream: [#1281](https://github.com/matiaszanolli/ByroRedux/issues/1281)
- Survey that ruled out systemic transform-translation loss:
  [docs/engine/per-game-translation-survey.md §4.2](./per-game-translation-survey.md)
  finding 8 (FNV architecture matrices clean, falsification table in
  [docs/engine/nif-engine-translation-layer.md §3 Axis 2](./nif-engine-translation-layer.md))
- Diagnostic tools cited:
  - [crates/plugin/examples/cell_refr_outliers.rs](../../crates/plugin/examples/cell_refr_outliers.rs)
  - [crates/plugin/examples/cell_rot_sweep.rs](../../crates/plugin/examples/cell_rot_sweep.rs)
  - [crates/plugin/examples/probe_form.rs](../../crates/plugin/examples/probe_form.rs)
  - [crates/plugin/examples/dump_cell_refs.rs](../../crates/plugin/examples/dump_cell_refs.rs)
  - [crates/nif/examples/dump_transforms.rs](../../crates/nif/examples/dump_transforms.rs)
