# Issues 2298, 2299, 2300, 2301

## #2298 — NIFAL-D2-01: de-strip dedup incomplete
**Severity**: LOW · **Labels**: bug, nif-parser, low, tech-debt
**Location**: `crates/nif/src/import/collision/shape.rs` (`resolve_compressed_mesh`'s chunk-strip walk) vs. `crates/nif/src/blocks/skin.rs:300-318` (`NiSkinPartition` inline destrip) vs. `resolve_tri_strips_data_refs` (already unified to call `NiTriStripsData::to_triangles()` per #2193).

`resolve_compressed_mesh` and the `NiSkinPartition` destrip still hand-copy
the even/odd-index CCW winding + degenerate-skip conversion instead of
calling a shared helper. All three currently agree (latent risk only), but
`resolve_compressed_mesh`'s copy silently diverged once before (fixed
incidentally by `3b9227341`).

**Fix**: Extract a shared de-strip helper and call it from all three sites.

## #2299 — NIFAL-D4-03: nifal.md passthrough table stale re: BSFurnitureMarker
**Severity**: LOW · **Labels**: documentation, nif-parser, low
**Location**: `docs/engine/nifal.md:309`

Combined `BSFurnitureMarker` / `BSInvMarker` passthrough row is half-stale:
`BSFurnitureMarker` has been consumed since #2010/M41.5 Phase B via
`byroredux/src/systems/sandbox.rs`'s `furniture_component()`. `BSInvMarker`
is still passthrough-only.

**Fix**: Split the row — mark `BSFurnitureMarker` consumed (cite
`furniture_component` / M41.5), keep `BSInvMarker` passthrough-only.

## #2300 — NIFAL-D5-01: particle emitter override folding copy-pasted
**Severity**: LOW · **Labels**: bug, import-pipeline, low, tech-debt
**Location**: `byroredux/src/scene/nif_loader.rs:520-528` and
`byroredux/src/cell_loader/spawn.rs:649-657` (identical 9-line block, outside
`apply_emitter_overlays` in `byroredux/src/systems/particle.rs`)

`texture_path`/`src_blend`/`dst_blend` override-folding block is
copy-pasted verbatim at both the loose-NIF load path and the cell-load path,
outside the declared `apply_emitter_overlays` boundary.

**Fix**: Extract the shared override-folding block into a helper (or fold
into `apply_emitter_overlays`) and call from both load sites.

## #2301 — NIFAL-D6-06: docs still cite import/collision.rs
**Severity**: LOW · **Labels**: documentation, nif-parser, low
**Location**: `docs/engine/nifal.md:202,216`, `docs/engine/nif-parser.md:528`,
`docs/engine/architecture.md`

Docs cite `import/collision.rs::resolve_shape`, which moved to
`import/collision/shape.rs` post-#1876 split; limitations table moved to
`import/collision/mod.rs`.

**Fix**: Update all three docs to point at the correct paths.
