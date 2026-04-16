# Investigation: #266 + #292

## #266 — Legacy compat LOW findings (3 items bundled)

### LC-06: Stale `ImportedSkin` doc comment
- [crates/nif/src/import/mod.rs:218-225](crates/nif/src/import/mod.rs#L218-L225) says BSTriShape weights are "currently not extracted"
- But [crates/nif/src/import/mesh.rs:504-505](crates/nif/src/import/mesh.rs#L504-L505) does extract them
- **Fix: 1 file, doc-only**

### LC-07: Per-secondary-slot UV transforms discarded
- `TexDesc.transform: Option<TexTransform>` is already parsed and contains translation/scale/rotation/center/transform_method
- [crates/nif/src/import/material.rs:402-408](crates/nif/src/import/material.rs#L402-L408) only reads the base-slot's translation+scale into MaterialInfo.uv_offset/uv_scale
- All other slots' transforms (detail, glow, gloss, dark) are dropped; all rotations/centers/methods dropped
- Current renderer uses a single uv_offset/uv_scale applied to every sampled texture — per-slot transforms aren't consumed downstream
- **Pragmatic fix**: capture transforms in MaterialInfo as `Option<TexTransform>` per slot so the data survives import. Defer downstream consumption (requires shader support).
- **Files: 1 (material.rs)** — internal type, no ECS propagation yet

### LC-08: NiAlphaProperty no-sorter flag (bit 13)
- Bit 13 (`0x2000`) disables depth sorting for a mesh
- Currently unextracted. With depth-sorted alpha blending (#241), this is now relevant.
- **Real fix** requires wiring through:
  - [crates/nif/src/import/material.rs](crates/nif/src/import/material.rs) — extract bit → `MaterialInfo.no_sort`
  - [crates/nif/src/import/mod.rs](crates/nif/src/import/mod.rs) — add `no_sort` to `ImportedMesh`
  - [crates/nif/src/import/mesh.rs](crates/nif/src/import/mesh.rs) — propagate
  - [crates/core/src/ecs/components/material.rs](crates/core/src/ecs/components/material.rs) — add field to `Material`
  - [byroredux/src/scene.rs](byroredux/src/scene.rs) — set `no_sort` on Material
  - [crates/renderer/src/vulkan/context/mod.rs](crates/renderer/src/vulkan/context/mod.rs) — `DrawCommand.no_sort`
  - [byroredux/src/render.rs](byroredux/src/render.rs) — use in sort key, populate in build_render_data
- **Files: 7**

### Total scope for #266: ~8 files, ~100-150 LOC

## #292 — Box<dyn NiObject> per-block allocation

Architectural issue. Audit itself categorizes:
- **Short-term**: no easy fix; internal `Vec::with_capacity` already used
- **Medium-term**: enum dispatch for top ~20 block types — major refactor across all 186 block types
- **Long-term**: `bumpalo` arena allocator — new dependency + ~entire NIF crate refactor

Any real fix requires introducing a new architectural pattern to the NIF crate. Not appropriate for a /fix-issue cycle. Options:
- (a) Close with explanatory comment; track as architectural debt
- (b) Keep open as long-term item
- (c) Add opportunistic small improvements (e.g., audit which block types dominate and shrink them via smaller `Box<dyn>` alternatives)

## Scope check

Total across both issues: 8+ files for #266 alone, architectural refactor for #292.
Pausing for user guidance before proceeding.
