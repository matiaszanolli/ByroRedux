# NIFAL-D3-NEW-01: Loose-NIF load path never extracts or spawns any of a mesh's authored lights

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2530
**Finding ID**: NIFAL-D3-NEW-01

**Severity**: HIGH
**Dimension**: Lights · **Tier Violated**: single-boundary / no-fabrication (the extraction call is *absent* on one of the two production load paths, not a bad translation of present data)
**Game Affected**: All (Oblivion → Starfield) — every loose-loaded NIF carrying an embedded `NiPointLight` / `NiSpotLight` / `NiAmbientLight` / `NiDirectionalLight`
**Location**: `byroredux/src/scene/nif_loader.rs` (entire file, 1165 lines — `parse_import_and_merge` / `load_nif_bytes_with_skeleton`)
**Status**: NEW — `gh issue list` search for "light"/"nif_loader" found only closed `#156`, which added the extraction+spawn path used by the cell loader only, not this one. Not a duplicate.

## Description
`byroredux_nif::import::import_nif_lights` — the sole function that walks a parsed `NifScene` and produces `Vec<ImportedLight>` — has exactly three call sites in the whole tree: `crates/nif/examples/import_probe.rs:47` (debug example), `byroredux/src/streaming.rs:895` (exterior grid pre-parse), and `byroredux/src/cell_loader/references/import.rs:116` (cell-loader ref import). `byroredux/src/scene/nif_loader.rs` — the module backing `cargo run -- path/to/mesh.nif` (documented in `CLAUDE.md`'s Quick Reference/Usage as a primary invocation, and the cache path behind *all* skeleton/body/hand NPC-part loading) — calls neither `import_nif_lights` nor a light-populating path. `grep -in light byroredux/src/scene/nif_loader.rs` returns zero matches across the full file, and `world.insert(entity, LightSource ...)` never appears in it — the only `LightSource` insertion site in the whole repo is `byroredux/src/cell_loader/spawn.rs:779`, unreachable from the loose loader.

## Evidence
```
$ grep -rn "import_nif_lights\b" --include='*.rs' crates/nif byroredux
crates/nif/src/import/mod.rs:483:pub fn import_nif_lights(scene: &NifScene) -> Vec<ImportedLight>
crates/nif/examples/import_probe.rs:47:    let lights = byroredux_nif::import::import_nif_lights(&scene);
byroredux/src/streaming.rs:895:        let lights = byroredux_nif::import::import_nif_lights(&scene);
byroredux/src/cell_loader/references/import.rs:116:    let lights = byroredux_nif::import::import_nif_lights(&scene);

$ grep -in "light" byroredux/src/scene/nif_loader.rs
(no output)
```

## Impact
A torch, candle, lantern, or streetlamp NIF loaded standalone (`cargo run -- <mesh>.nif`) renders its flame/bulb geometry but contributes zero light to the scene — visible content loss, not cosmetic. Since `load_nif_bytes_with_skeleton`'s cache path backs *every* skeleton/body/hand NPC-part load (not just the standalone entry point, per that function's own doc comment), the blast radius extends to normal cell-loaded NPC rendering wherever NPC-part NIFs carry lights, though the most directly observable case is the documented loose-load workflow.

## Related
Sibling gap to closed `#156` (which fixed the cell-loader path only). Not a duplicate of any open issue.

## Suggested Fix
Call `byroredux_nif::import::import_nif_lights(&scene)` in `parse_import_and_merge`, store the result on the loader's cache-entry struct, and add a light-spawn loop in `load_nif_bytes_with_skeleton` mirroring `cell_loader/spawn.rs::spawn_nif_lights` — widen `is_spawnable_nif_light`/`light_radius_or_default` to `pub(crate)`-shared (they already are `pub(crate)` in `spawn.rs`) or lift them to a shared helper rather than re-deriving the sanitization logic a third time.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The loose-loader's light spawn goes through the same `LightSource` construction shape as the cell-loader path, not a fourth divergent one
- [ ] **TESTS**: A regression test loads a standalone NIF with an embedded light and confirms a `LightSource` entity spawns
