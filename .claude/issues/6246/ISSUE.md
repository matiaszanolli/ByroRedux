# #6246: EXT-D6-2026-10-21-02: FO4 terrain-LOD boundary blocks are dropped when their base LTEX is MNAM-only (#6199 fixed full detail but not the LOD synth path)

**Labels**: medium,terrain-exterior,game:fo4,bug

**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Distant LOD and trees · **Location**: `byroredux/src/cell_loader/terrain_lod.rs (spawn_lod_block)`

- **Tier Violated**: single-boundary
- **Game Affected**: Fallout 4. Skyrim landscape TXSTs all author TX00. Oblivion, FO3 and FNV take the legacy translated-quad texture.
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:925-947` (`spawn_lod_block` base-texture fallback); `crates/plugin/src/esm/cell/index.rs:443-452` (`resolve_landscape_texture_sets`)
- **Status**: NEW (residual of closed #6199, `e62e258bd`)
- **Description**: `resolve_landscape_texture_sets` removes from `landscape_textures` every LTEX whose TXST has no TX00 diffuse. That is exactly the 32 of FO4's 128 LTEXs that author only `MNAM` (`LCWDefault01Grass`, the `LNFoothills*` family, `LCobblestone01`, …). `e62e258bd` added `LandscapeLayerTextures::texture_set`, which fills these LTEXs from the BGSM chain, but wired it only into full-detail `spawn_terrain_mesh`. `spawn_lod_block` still runs:
  ```rust
  Some(ltex_id) if ltex_id != 0 => match landscape_textures.get(&ltex_id) {
      Some(path) => (resolve_texture(..), Some(path.clone())),
      None => (0, None),
  },
  ...
  if tex_handle == 0 { return None; }   // "Small World / ocean-only quad"
  ```
  The caller records the result as a real miss (`missing_blocks.insert`). On Combined games (Skyrim, FO4), every boundary block (`mask != 0`) is synth-only by design: a baked `.btr` cannot be hole-masked, and `translated_lod` is `None` because `legacy_landscape_lod_supported` is false. So a boundary block whose first base LTEX is MNAM-only never spawns.
- **Evidence**: The `e62e258bd` diff touches `terrain.rs`, `exterior.rs` and `provider.rs` only. `TerrainSpawnCtx` has one construction site (`exterior.rs:2248`), and `terrain_lod.rs:663` still passes `&index.landscape_textures`. The commit's own measurement on Commonwealth 0,1 (37 → 7 unresolved layers) shows how common MNAM-only bases are.
- **Impact**: Around the full-detail ring on FO4, the non-resident cells of affected boundary blocks have no terrain at all. Sky shows through between the loaded area and the `.btr` ring. The default radius puts this about 12 cells out. Like any recorded miss, it is not retried while the mask is unchanged (see #6092).
- **Related**: #6199 (closed), #6092 (known-miss caching), #6053 / #6094 (other terrain-LOD coverage defects).
- **Suggested Fix**: Resolve the synth block's base diffuse through the same `LandscapeLayerTextures` resolver (thread the `MaterialProvider` into `reconcile_lod_rings`), or precompute a BGSM-filled `landscape_textures` at index build so both paths read one table.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
