# #545 NiFlipController (texture flipbook) has no channel emission

**State**: OPEN  • **Severity**: low  • **Domain**: nif → import-pipeline

## Summary

`NiFlipController` (Morrowind → Oblivion-era flipbook driver — fire,
smoke, water cycles, explosion cross-strips) is parsed by the NIF
crate (#394) but the import pipeline drops it on the floor:

- `walk_controller_chain` (anim.rs:386–403) has no `downcast_ref` arm
  for it — the chain terminates at the first FlipController node.
- `import_embedded_animations` and `import_sequence` both hit the
  `_ => debug!("Skipping unsupported embedded controller type …")`
  branch.

Result: 31 vanilla Oblivion NIFs + the FO3/FNV fire/smoke/explosion
flipbooks deliver static textures.

## Plan (parser-side; renderer integration deferred)

1. Add `TextureFlipChannel` to `crates/core/src/animation/types.rs`
   carrying `texture_slot: u32`, resolved `source_paths: Vec<Arc<str>>`,
   and the float-typed flipbook keys.
2. Add `texture_flip_channels` field to `AnimationClip`.
3. In `crates/nif/src/anim.rs`:
   - extend `walk_controller_chain` with a `NiFlipController` arm so
     the chain advances past it,
   - add a case in `import_embedded_animations` that resolves the
     `sources: Vec<BlockRef>` to `NiSourceTexture` filenames,
   - same in `import_sequence` for KF-driven flipbooks.
4. Initialise the new field everywhere `AnimationClip { … }` is
   constructed (~13 sites — uniformly inserted between
   `bool_channels` and `text_keys`).
5. Regression test: synthesise a NIF with a `NiFlipController`
   chain, run `import_embedded_animations`, assert the resulting
   clip carries one `TextureFlipChannel` with the expected source
   path list.

Renderer-side (sample → bind animated texture handle into
`GpuInstance.albedo_texture`) is **out of scope** for this issue —
follows the `MorphWeight` precedent (channel data captured, GPU
plumbing follows in a later milestone). Skyrim+ ditched
`NiFlipController` for `BSEffectShader` UV scrolling, so this is
strictly Oblivion / FO3 / FNV long-tail compat.
