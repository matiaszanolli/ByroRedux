# SF2D2-05: Four import_nif/import_nif_scene call sites pass no MeshResolver — external-geometry BSGeometry there imports to zero meshes

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2362
**Labels**: bug,nif-parser,low,legacy-compat

---

**Severity**: LOW (currently unreachable for Starfield; becomes MEDIUM once Starfield distant-object LOD is wired)
**Location**: `byroredux/src/cell_loader/object_lod.rs:262`, `placement_lod.rs:469`, `terrain_lod_btr.rs:137` (all call `import_nif_scene`), `byroredux/examples/dump_nif.rs:151` (calls `import_nif`, which delegates to `import_nif_impl(scene, pool, None)` — same no-resolver pattern)
**Status**: NEW, CONFIRMED against current code

## Description

These call the no-resolver overloads even though `object_lod.rs`/`placement_lod.rs` already hold a `tex_provider: &TextureProvider` (which *is* a `MeshResolver`) in scope at the call site.

## Evidence

- Confirmed via direct read: `object_lod.rs:262` and `placement_lod.rs:469` call `byroredux_nif::import::import_nif_scene(&scene, &mut pool)` (the no-resolver overload), while `tex_provider` is already bound in the same function (`object_lod.rs:160`, `placement_lod.rs:317`) and used for `extract_mesh`/`resolve_texture` calls nearby.
- `terrain_lod_btr.rs:137` likewise calls the no-resolver `import_nif_scene`.
- `dump_nif.rs:151` calls `import_nif(&scene, &mut pool)`, which is `import_nif_impl(scene, pool, None)` per `crates/nif/src/import/mod.rs:386-388` — functionally the same no-resolver gap, in a debug/dump example tool rather than production code.

## Impact

Not Starfield-reachable today (`object_lod` is `.bto`-keyed and Starfield's `LODMeshes.ba2` has zero `.bto`; `placement_lod` is Oblivion-gated) — but Starfield ships 19,535 `meshes\lod\generated\..._lod_N.nif` files that a future distant-object-LOD arc would hit, inheriting a silent 100% drop if it reuses either helper.

## Suggested Fix

Thread the already-in-scope `tex_provider` through as `Some(tex_provider)` at the two cell_loader call sites (`import_nif_scene_with_resolver`); for `dump_nif.rs`, thread a resolver through if/when the example needs one, or note the gap in a comment.

## Completeness Checks
- [ ] **SIBLING**: Confirmed 4 call sites total across the codebase with this pattern (3 production `import_nif_scene`, 1 example-tool `import_nif`)
- [ ] **TESTS**: A regression test pins that a future distant-LOD spawn path using `object_lod.rs`/`placement_lod.rs` with an external-geometry BSGeometry mesh resolves correctly, not silently to zero meshes
