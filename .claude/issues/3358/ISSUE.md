# SKY-2026-08-27-D6-01: `.btr` distant terrain drops the authored uniform scale on the height axis — every baked terrain quad renders at 1/`level` of its true elevation

Labels: high,terrain-exterior,bug,game:skyrim,legacy-compat

- **Severity**: HIGH
- **Confidence**: CONFIRMED (read the code + verified against shipped Skyrim SE data)
- **Location**: `byroredux/src/cell_loader/terrain_lod_btr.rs:124-129` (`btr_local_to_world`),
  premise stated at `byroredux/src/cell_loader/terrain_lod_btr.rs:48-62` and
  `byroredux/src/cell_loader/terrain_lod_btr.rs:188-223`, enshrined by the unit test at
  `byroredux/src/cell_loader/terrain_lod_btr.rs:427-446`
- **Description**:
  The module premise is that a `.btr` is *"a normalized quad-local mesh … at the origin
  with **identity transform** — only the heights differ"*, and that heights are
  *"absolute world heights and are not scaled"*. Both halves of that premise are false
  on real data. Every shipped `.btr` geometry block carries a **uniform**
  `NiAVObject.transform.scale == level` (the quad edge in cells). The loader
  reproduces that scale by hand on X and Z only:

  ```rust
  fn btr_local_to_world(local: [f32; 3], level: i32, qx: i32, qy: i32) -> [f32; 3] {
      let lvl = level as f32;
      let ox = qx as f32 * EXTERIOR_CELL_UNITS;
      let oz = qy as f32 * EXTERIOR_CELL_UNITS;
      [ox + local[0] * lvl, local[1], local[2] * lvl - oz]   // <-- local[1] unscaled
  }
  ```

  and explicitly discards the authored transform
  (`// The mesh's own translation/rotation/scale are identity for `.btr` and deliberately ignored.`).
  The horizontal footprint therefore comes out right by accident — hand-multiplying by
  `level` happens to equal applying the authored scale — while the height axis is left
  `level`× too small. The downstream anisotropic normal/tangent fix-ups
  (`normal ∝ (nx/level, ny, nz/level)`, `tangent ∝ (tx·level, ty, tz·level)`,
  lines 205-217) are consistent with that same wrong anisotropic mapping; under a
  correct uniform scale they are no-ops and must be removed too (a uniform scale
  preserves normal directions).
- **Evidence**:
  1. **Raw wire scale** — dumping `NiAVObject.transform` off the shipped blocks:
     ```
     === meshes\terrain\tamriel\tamriel.4.-72.32.btr
       [0] BSMultiBoundNode name=Some("chunk") trans=(0,0,0) scale=1
       [1] BSTriShape        name=Some("land")  trans=(0,0,0) scale=4
       [4] BSMultiBoundNode name=Some("WATER") trans=(0,0,0) scale=1
       [5] BSSubIndexTriShape name=None        trans=(0,0,0) scale=4
     === meshes\terrain\tamriel\tamriel.32.-96.32.btr
       [1] BSTriShape        name=Some("land")  trans=(0,0,0) scale=32
       [5] BSTriShape        name=None          trans=(0,0,0) scale=32
     ```
     Scale is uniform and equals the quad level; translation is zero; the parent
     `BSMultiBoundNode`s are identity.
  2. **Per-level height ranges over all 3060 Tamriel `.btr`** — the local Y range
     halves exactly as the level doubles, the signature of heights pre-divided by the
     authored scale:
     ```
     level   4: files= 2304 scales=[4.0]  local x[0.0,4096.0] y[-9726.0,9848.0] z[-4096.0,-0.0]
     level   8: files=  576 scales=[8.0]  local x[0.0,4096.0] y[-4965.0,4924.0] z[-4096.0,-0.0]
     level  16: files=  144 scales=[16.0] local x[0.0,4096.0] y[-2540.0,2462.0] z[-4096.0,-0.0]
     level  32: files=   36 scales=[32.0] local x[-4.0,4108.0] y[-1303.5,1227.0] z[-4096.0,0.0]
     ```
     `9848·4 = 4924·8 = 2462·16 = 39392`; `1227·32 = 39264`. All four bands converge on
     the same world height range **only** when the authored scale is applied to Y.
  3. **Shared-corner cross-check** — the SW corner cell (-72, 32) is covered by both a
     level-4 and a level-8 quad. Their local heights at that corner differ by exactly
     the level ratio, and agree after scaling:
     ```
     tamriel.4.-72.32.btr  level=4  SW-corner local y = [-5934.0, -6184.0, -3500.0]   × level = [-23736.0, -24736.0, -14000.0]
     tamriel.8.-72.32.btr  level=8  SW-corner local y = [-2967.0, -3217.0, -1750.0]   × level = [-23736.0, -25736.0, -14000.0]
     ```
  4. **Independent absolute check** — the `WATER` sub-mesh vertex heights, once
     multiplied by `level`, land on authored round water heights across all 1937
     water-bearing Tamriel quads; the smallest is exactly Tamriel's WRLD `DNAM`
     default water height, which this repo already documents at
     `byroredux/src/env_translate.rs:159` (*"Tamriel -14000"*):
     ```
     distinct WATER vertex heights × level (79): [-14000.0, -13670.0, -13400.0, -13300.0,
       -13000.0, -12750.0, -12450.0, -12350.0, -12150.0, -12000.0, -11801.0, -11800.0, …]
     ```
     Unscaled these would be -3500.0, -3417.5, -3350.0 … — no relationship to any
     authored height.
  5. **The sibling loader disagrees** — `.bto` object LOD reads the same authored
     transform and applies it uniformly:
     `byroredux/src/cell_loader/object_lod.rs:320-336` does
     `let scale = mesh.scale; … Transform::new(pos, rot, scale)`. Dumping a `.bto`
     confirms the same convention (`mesh 'Obj' t=[-65536,0,-65536] s=4`, parents
     identity, translation = the quad's SW world corner). So `.bto` is correct and
     `.btr` is not, for identical authored data.
- **Impact**: Every prebaked distant-terrain quad on Skyrim SE renders at 1/4 (finest
  baked band) to 1/32 (coarsest) of its true elevation — mountains flatten toward the
  worldspace floor and the horizon collapses into a near-planar sheet. Because the
  error scales with the band, adjacent bands sit at *different* wrong elevations, so
  every band boundary is a vertical discontinuity, and the `.btr` ring meets the
  full-detail LAND terrain at a cliff at the streaming boundary. This is the whole
  M35 `.btr` feature on Tamriel: 2304 level-4 + 576 level-8 + 144 level-16 + 36
  level-32 quads for Tamriel alone, plus every DLC worldspace. The same loader is
  gated `Skyrim | Fallout4` (`combined_lod_supported`), so FO4 is on the same code
  path (not measured here — FO4 `.btr` live in a BA2 and were out of scope for this
  probe). Also flattens the mesh normals at coarse bands: dividing `nx`/`nz` by 32
  drives every level-32 normal to near `(0,1,0)`, so distant terrain shades as a flat
  plane on top of being one.
- **Suggested Fix**: Apply the authored uniform scale on all three axes —
  `world_y = local[1] * level` in `btr_local_to_world` — and delete the anisotropic
  normal (`/lvl`) and tangent (`*lvl`) corrections at
  `terrain_lod_btr.rs:205-217`, since a uniform scale leaves both unchanged. Better
  still, stop hand-rolling the mapping and consume `mesh.scale` / `mesh.translation`
  the way `object_lod.rs` already does, so the two baked-LOD loaders share one
  convention. Update the module doc (lines 48-62), which asserts identity transform
  and unscaled heights, and the unit test at lines 427-446, whose
  `assert_eq!(sw, [0.0, 10.0, 4.0 * cell])` currently pins the bug. Add a regression
  assertion on the cross-band agreement (the level-4 vs level-8 shared-corner check
  above) so the two mappings cannot drift apart again.
- **Related**: Not covered by any open issue. Checked #3336 (btr has no `Material` —
  different defect, same file), #3306 (FO4 LAND height crack — full-detail terrain,
  not LOD), #3307 / #1731 (VWD culling, explicitly out of scope). the 300-issue dedup baseline (84 open, fetched 2026-08-27)
  has no `.btr`/height/terrain-LOD-placement entry.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
