# OBL-D4-H4: NiTexturingProperty decal slots (7+) never extracted from block

**Issue**: #400 — https://github.com/matiaszanolli/ByroRedux/issues/400
**Labels**: bug, nif-parser, renderer, high

---

## Finding

`NiTexturingProperty` parses the decal texture count at `crates/nif/src/blocks/properties.rs:275-285` and reads the raw `TexDesc` entries, but no `decal_0_texture` / `decal_textures[N]` field is ever added to the property struct. `rg 'decal_0_texture|decal_textures' crates/nif` returns zero hits in the blocks module.

The importer in `crates/nif/src/import/material.rs` has no extraction path for decal slots — only base through decal_0 (slot 6) is wired, and decal_0 itself is likely lost too (verify during fix).

## Impact on Oblivion

- **Blood splatters** (post-combat decals via scripted placement)
- **Wall paintings, map decals** (Imperial City signs, Anvil dock wear, dungeon cave paintings)
- **Faction symbols** (Dark Brotherhood, Thieves Guild markers)

All silently vanish from every NiTriShape that references them. Oblivion uses decal slots for content that persists in the world, not just for gameplay-triggered effects.

## Fix

1. Add `decal_textures: Vec<TexDesc>` to `NiTexturingProperty`.
2. Read all N decal entries in the parser (count is already captured).
3. Extend `MaterialInfo` with `decal_maps: Vec<TextureRef>` and populate in `extract_material_info`.
4. Extend `GpuInstance` with `decal_map_indices: [u32; N]` (fixed cap, e.g. N=4) — see OBL-D4-H3 for the broader texture-slot plumbing work.
5. Fragment shader: sample + alpha-blend each decal on top of base material.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Land this together with OBL-D4-H3 (3 slots already not reaching GPU) — same plumbing change.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Parse a NIF with a NiTexturingProperty that has `decal_count=2`; assert the 2 `TexDesc` entries are reachable from the property.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 4 H4-04.
