# #1341 — D3-05: GreyscaleLutHandle texture refcount leaked on cell unload

_Snapshot as filed from AUDIT_FNV_2026-05-30 (d3-05). GitHub is authoritative for live state — query `gh issue view 1341 --json state`._

**Severity**: HIGH · **Dimension**: Cell Loading · **Source**: AUDIT_FNV_2026-05-30 (D3-05)

**Location**: `byroredux/src/cell_loader/spawn.rs:910` (acquire) vs `byroredux/src/cell_loader/unload.rs:76-118` (victim walk omits it)

**Description**: `spawn_placed_instances` resolves a BSEffectShaderProperty greyscale LUT via `resolve_texture` (refcount bump) and attaches `GreyscaleLutHandle` (spawn.rs:910), but the unload victim walk queries MeshHandle/TextureHandle/NormalMapHandle/DarkMapHandle/ExtraTextureMaps/TerrainTileSlot (unload.rs:76-81) and **not `GreyscaleLutHandle`**, so the LUT texture is never handed to `drop_texture`.

**Evidence**: grep of `unload.rs` for `GreyscaleLutHandle` returns nothing. `GreyscaleLutHandle(pub u32)` is a real bindless handle consumed by the renderer as `greyscale_lut_index`. The acquire site is gated on `h != fallback()`, so every attached handle is a real, refcounted, never-released texture. Component was missed when added under #890.

**Impact**: One texture refcount leaked per distinct greyscale-LUT texture per unloaded cell, for the process lifetime — bindless slot + VkImage pinned. Engine-wide; fires on FNV cells using BSEffectShaderProperty greyscale remapping.

**Suggested Fix**: Add `GreyscaleLutHandle` to the unload victim walk — declare `let gq = world.query::<GreyscaleLutHandle>();` alongside the others, add the per-victim `if let Some(gh) = gq.get(eid) { push_tex_drop(gh.0, ...) }` block, and include `gq` in the `drop(...)` guard release. One-line-per-site, mirroring `NormalMapHandle`.

## Completeness Checks
- [ ] **SIBLING**: Audit every texture-handle component type for inclusion in the unload walk (cross-check the component list in `components.rs` against the queries in `unload.rs`).
- [ ] **DROP**: Confirm no double-drop when the LUT is shared with another consumer.
- [ ] **TESTS**: Regression test — spawn a cell with a greyscale-LUT effect mesh, unload, assert the LUT refcount returned to baseline.
