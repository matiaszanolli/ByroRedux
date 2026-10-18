# PHYS-EXT-D1-2026-10-18-01: Terrain colliders ignore the authored LTEX Havok data (material, friction, restitution); every exterior ground tile is `surface_material` 0 / friction 0.5 / restitution 0.3

Labels: low,physics,terrain-exterior,esm-plugin,bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-18.md

- **Severity**: LOW
- **Dimension**: Shape Translation (collider producer) / Queries (footstep probe consumer)
- **Location**: `byroredux/src/cell_loader/terrain.rs:1306-1317` (ghost spawn), `byroredux/src/cell_loader/spawn.rs:397-421` and `:353-355` (`synthesize_static_trimesh` returns `RigidBodyData::static_body()`), `crates/core/src/ecs/components/collision.rs:173-185` (`friction: 0.5, restitution: 0.3, surface_material: 0`); LTEX parse `crates/plugin/src/esm/cell/support.rs:517-571` (reads only `EDID`, `TNAM`, `GNAM`, `ICON`)
- **Status**: NEW
- **Description**: LTEX records author Havok surface data. Oblivion: `HNAM` = material type (ubyte, 0 Stone, 2 Dirt, 4 Grass, 8 Water, ...), friction (ubyte, default 30), restitution (ubyte, default 30) (`uesp-wiki/Oblivion Mod/Mod File Format/LTEX.wiki`). Skyrim: `MNAM` = MATT form reference, `HNAM` = friction/restitution (`uesp-wiki/Skyrim Mod/Mod File Format/LTEX.wiki`). FO3/FNV layout was not verified from a local source (the GECK wiki dump has no LTEX page); do not assume it equals Oblivion's. `parse_ltex_group_inner` drops all of these, and the terrain collider is built through the architecture-static default. The result is that dirt, grass, snow, sand and rock tiles are indistinguishable to physics.
- **Evidence**: `rg -n "HNAM|MNAM"` over `crates/plugin/src/esm/cell/` finds no LTEX use; the match arm in `support.rs` ends with `_ => {}`. `PhysicsWorld::surface_material_under` returns `colliders.get(c)?.user_data as u32` (`world/queries.rs:253`), which for a terrain tile is the part material packed by `register_newcomers` from `RigidBodyData::surface_material` = 0.
- **Impact**: Today none visible: `FootstepConfig.surface_sounds` and the impact table are empty until IPDS decode lands (`audio.rs` doc: "the map is empty until IPDS decoding lands"), so every footstep takes the default sound. Once IPDS lands, exterior footsteps and impacts on terrain will always key on material 0, which is `STONE` on Oblivion/FO3/FNV and `SKY_HAV_MAT_NONE` on Skyrim, regardless of the ground texture; rigid props sliding or rolling on terrain use the flat 0.5/0.3 pair (geometric-mean combined) instead of the authored per-texture values. Placed statics and architecture (which carry authored `bhk` materials) will differ from the ground they stand on.
- **Trigger Conditions**: any exterior cell with LAND, with a consumer of surface material or terrain friction (future IPDS footsteps; any dynamic prop on terrain today for friction).
- **Related**: #5545 (surface probe contract), #5542 (u32 surface material), `docs/engine/physal.md` surface-material section; handoff for the LTEX decode half to `/audit-esm`.
- **Suggested Fix**: decode LTEX `HNAM`/`MNAM` at the parse boundary into the existing LTEX table, reduce per tile to the dominant painted layer (the splat layers are already resolved per cell in `build_cell_splat_layers`), and give the terrain ghost a `RigidBodyData` carrying that material and friction/restitution. Per-vertex material would need per-triangle part materials (the part sequence machinery exists: `part_surface_materials`). Keep it a translation-boundary change, no per-game branch in `crates/physics`.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the parse / `translate_*` boundary, never in the renderer or re-derived at render time
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, other callers)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
