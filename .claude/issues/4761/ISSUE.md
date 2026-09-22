# TOOL-D6-2026-09-22-01: texture-upscale manifest validation and preflight never dedupe by derived output path, so two sources can collide on one .png

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4761

## Description
`output_png_path` (`tools/texture-upscale/src/lib.rs:201-210`) replaces whatever extension a source path had with `.png` via `.set_extension("png")`, so two source paths identical except for extension collapse to one output file. `Manifest::validate`'s dedup (`lib.rs:76-104`) compares normalized *source* strings including extension: the "map path duplicates the reference" check (`map_path == normalize_asset_path(&set.reference)`) and the per-set `map_paths` `BTreeSet` both operate on extension-preserving strings, so e.g. `reference: "textures/wood.dds"` + `maps: [{path: "textures/wood.tga"}]` in the **same** `TextureSet` passes validation despite both resolving to `textures/wood.png`. `preflight_outputs` (`pipeline.rs:214-234`) checks each computed output against on-disk existence independently (`!options.overwrite && reference.exists()` / `output.exists()`) but never accumulates outputs into a shared collection to check against each other, so cross-set collisions pass too when neither destination pre-exists yet.

## Evidence
```rust
// lib.rs:201-208 — extension always replaced with .png
pub fn output_png_path(root: &Path, asset_path: &str) -> Result<PathBuf> {
    ...
    output.set_extension("png");
```
```rust
// lib.rs:92-98 — compares source strings including extension
if map_path == normalize_asset_path(&set.reference) { bail!(...) }  // "wood.tga" != "wood.dds"
```
```rust
// pipeline.rs:214-234 — each output checked independently, never against siblings
for set in &manifest.sets {
    let reference = output_png_path(options.output_root, &set.reference)?;
    if !options.dry_run && !options.overwrite && reference.exists() { bail!(...) }
    for map in &set.maps {
        let output = output_png_path(options.output_root, &map.path)?;
        if !options.dry_run && !options.overwrite && output.exists() { bail!(...) }
    }
}
```

## Impact
Default (`--overwrite` off): the first colliding write succeeds; the second fails mid-batch via a fresh `path.exists()` check, aborting the whole run *after* the expensive external upscaler already ran for earlier, unrelated sets. `--overwrite` on: the second colliding write silently replaces the first with no warning and no `RunReport` entry — one of two logically distinct upscaled textures is silently lost.

## Related
None found.

## Suggested Fix
Accumulate every `output_png_path` result into a `HashSet` in `preflight_outputs` (or `Manifest::validate`) and `bail!` on a collision before any work begins, independent of `--overwrite` and on-disk state.

Source: docs/audits/AUDIT_TOOLING_2026-09-22.md (TOOL-D6-2026-09-22-01)

## Completeness Checks
- [ ] **TESTS**: A regression test constructs a manifest with two source paths differing only by extension in the same `TextureSet` and asserts validation/preflight rejects it before any upscaler work begins
