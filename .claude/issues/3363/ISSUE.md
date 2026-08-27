# SKY-2026-08-27-D6-02: the `.btr` `WATER` sub-mesh is welded into the opaque distant-terrain mesh instead of being excluded or routed to WATAL

Labels: medium,terrain-exterior,water,bug,game:skyrim,legacy-compat

- **Severity**: MEDIUM
- **Confidence**: CONFIRMED (read the code + verified against shipped Skyrim SE data)
- **Location**: `byroredux/src/cell_loader/terrain_lod_btr.rs:196-226`
- **Description**: `spawn_btr_block` iterates **every** sub-mesh the `.btr` imports and
  bakes them all into one vertex/index buffer uploaded as a single opaque
  `IsLodTerrain` draw sampling the per-quad land diffuse. Vanilla `.btr` are not a
  single surface: they ship a `chunk`/`land` sub-tree **and** a separate `WATER`
  `BSMultiBoundNode` carrying a flat water surface. Nothing in the loop filters it, so
  the distant water plate is rasterised as opaque ground geometry with the terrain
  diffuse bound and never reaches the water pass, while the engine *separately* draws
  a worldspace-wide LOD water frame over the same annulus
  (`byroredux/src/cell_loader/water.rs:735-868`, `spawn_lod_water_plane`, #2449).
- **Evidence**:
  ```
  === meshes\terrain\tamriel\tamriel.4.-72.32.btr   (imported)
    node[0] 'chunk' parent=None t=[0,0,-0] s=1
    node[1] 'WATER' parent=Some(0) t=[0,0,-0] s=1
    mesh[0] 'land' parent=Some(0) s=4 verts=1080
    mesh[1] None   parent=Some(1) s=4 verts=64      <-- welded into the same buffer
  ```
  Census over Tamriel: `btr files=3060 with WATER submesh=1937`
  (per level — `4: (2304 files, 1375 with water), 8: (576, 410), 16: (144, 118), 32: (36, 34)`).
  The loop that consumes them has no name/parent test:
  ```rust
  for mesh in &imported.meshes {
      if mesh.positions.is_empty() || mesh.indices.is_empty() { continue; }
      let base = vertices.len() as u32;
      ...
  }
  ```
- **Impact**: 63 % of Tamriel's baked terrain quads draw an extra opaque plate at the
  local water height, sampling the land atlas, covering the lake/sea bed underneath it
  and adding ~64 verts/quad of geometry that the renderer treats as ground. Once
  SKY-2026-08-27-D6-01 is fixed and those plates land at their true authored heights
  (-14000 for Tamriel's ocean, matching `spawn_lod_water_plane`'s `lod_height`), they
  become coplanar with the LOD water frame and will z-fight it across the whole
  distant seascape. It is also a WATAL boundary violation: a water surface that never
  reaches the water material path.
- **Suggested Fix**: Skip sub-meshes whose parent `ImportedNode` is named `WATER`
  (case-insensitive) when accumulating the land buffer — the discriminator is already
  in `ImportedScene.nodes` and needs no new parsing. Either drop them (the engine
  already owns distant water via `spawn_lod_water_plane`) or, better, hand them to
  WATAL as per-quad LOD water so lake surfaces above sea level are represented at all;
  the worldspace-wide frame is a single height and cannot express them.
- **Related**: #2449 / EXAL-01 (the LOD water frame this would collide with).
  No open issue covers the `.btr` sub-mesh split.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
