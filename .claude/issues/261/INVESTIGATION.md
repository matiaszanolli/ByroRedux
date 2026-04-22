# #261 Investigation

## Premise confirmed

Every `controller_ref` assignment across the import tree was
`BlockRef::NULL` (22 sites: `walk.rs`, `material.rs`, `mesh.rs`, `mod.rs`).
The NIF parser and block registry had the controller types already
(`NiVisController`, `NiAlphaController`, `NiTextureTransformController`,
`NiMaterialColorController`, `NiUVController`, `BSShader*Controller`)
— but the import pipeline never followed `NiObjectNET.controller_ref`,
so mesh-embedded ambient animations (UV scroll, visibility flicker,
alpha fade, material-colour pulse) were silently discarded.

`NiFlipController` (texture flipbook) has no parser — left as a
separate follow-up; the traversal framework accommodates it once the
block type lands.

## Architecture landing

Refactor the existing KF-import helpers so the core "interpolator
index → keys" logic is independent of `ControlledBlock`:

- `extract_float_channel_at(scene, interp_idx, target)`
- `extract_bool_channel_at(scene, interp_idx)`
- `resolve_color_keys_at(scene, interp_idx)`

These are reused from both the KF sequence path and the new
`import_embedded_animations(scene)` entry point.

`import_embedded_animations` walks every `NiObjectNET`-bearing block
(via a downcast table covering `NiNode`, `NiTriShape`, `BsTriShape`,
`NiCamera`, `NiMaterialProperty`, `NiTexturingProperty`,
`BSLightingShaderProperty`, `BSEffectShaderProperty`), follows the
`next_controller_ref` chain with a 64-hop cycle guard, and dispatches
each controller against the KF-supported type set. Channels accumulate
into a single looping `AnimationClip` named `"embedded"` with
`cycle_type = Loop` and `frequency = 1.0`. Returns `None` when no
supported controllers are found.

## ECS wiring

`ImportedScene.embedded_clip: Option<AnimationClip>` carries the
result from `import_nif_scene`. `load_nif_bytes` (byroredux/src/scene.rs)
registers the clip in `AnimationClipRegistry`, spawns an
`AnimationPlayer` scoped to the NIF root. The KF path is unchanged;
embedded and sequence clips can coexist on the same entity via the
stack.

## Cell-loader deferral

The cell-load spawn path (`byroredux/src/cell_loader.rs::spawn_placed_instances`)
flattens NIFs into individual mesh entities with no parent/child links
and no `Name` components — the `AnimationStack`'s subtree-name lookup
can't anchor against that layout. `CachedNifImport.embedded_clip` is
populated but currently unused on this path; a one-line `log::debug!`
notes the capture so the capability is discoverable. Wiring the
cell-load path requires a placement-root refactor (add a parent entity
per REFR, parent meshes under it, attach `Name` from the NIF root
node) — filed as a follow-up in the commit body.

## Tests

Two new regression tests in `crates/nif/src/anim.rs::tests`:

1. `import_embedded_animations_captures_texture_transform_controller`
   — synthetic 4-block scene (NiFloatData + NiFloatInterpolator +
   NiTextureTransformController + NiNode). Asserts the produced clip
   has exactly one `FloatChannel` keyed by the node name and targeting
   `FloatTarget::UvOffsetU`.
2. `import_embedded_animations_returns_none_when_no_controllers` —
   null controller_ref → `None`.

Full workspace: 1017 tests pass. The pre-existing Oblivion
`marker_radius.nif` parse-rate failure (`318 MB allocation exceeds
hard cap`) is unrelated to this change — verified via stash-diff.

## Files touched

- `crates/nif/src/anim.rs` — refactor + new `import_embedded_animations`
  + 2 regression tests.
- `crates/nif/src/import/mod.rs` — add `ImportedScene.embedded_clip`
  field, populate in `import_nif_scene`.
- `byroredux/src/scene.rs` — register clip + spawn AnimationPlayer
  in `load_nif_bytes`.
- `byroredux/src/cell_loader.rs` — capture `embedded_clip` on
  `CachedNifImport` + log; full wiring deferred.
