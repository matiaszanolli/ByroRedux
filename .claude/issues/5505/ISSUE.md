# #5505: OBL-2026-10-09-D4-01: Oblivion child worldspaces draw no ground outside their own LAND footprint. The CS documents that a child uses its parent's landscape, and #5374 stamps the land and LOD inherit bits, but neither the parent's LAND nor its…

**Labels**: bug, game:oblivion, legacy-compat, medium, terrain-exterior

**Source**: `docs/audits/AUDIT_OBLIVION_2026-10-09.md` — finding `OBL-2026-10-09-D4-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM. Terrain and the horizon are missing around all 30 child worldspaces (every city, every Imperial City district, New Sheoth). The cities' own footprints are intact, and nothing crashes.
- **Dimension**: Exterior & Lighting Data (Tamriel). The mechanism is EXAL terrain / LOD (`/audit-exterior`).
- **Location**:
  - `byroredux/src/cell_loader/exterior.rs:2082-2100`: terrain spawns only from the child cell's own `cell.landscape`.
  - `byroredux/src/cell_loader/terrain_lod.rs:450-457`: the LOD textures are keyed on the child's own form ID ("Small worlds (AnvilWorld) ship none, so their synth blocks resolve no texture and are suppressed (#1745)"). They are holed against the child's cell map.
  - `crates/plugin/src/esm/cell/wrld.rs:228-235`: the parse stamp sets `INHERIT_LAND | INHERIT_LOD` for Oblivion children, "so the bit-gated walks resolve the parent's water/climate/LOD".
  - The only readers of those two bits are the `DNAM` water height and the `NAM3`/`NAM4` LOD water (`env_translate.rs:209-298`). Oblivion authors neither.
- **Status**: NEW.
  - `gh` all-states searches for "child worldspace terrain", "city worldspace", "parent landscape", "child worldspace LOD", "INHERIT_LAND" and "use land data" find only #5374 / #5388 (water and climate, closed), #2735 (`DNAM` land data) and #5335 (Oblivion LOD water, open; related but a different gap).
  - AUDIT_EXTERIOR_2026-10-09 does not cover it.
- **Description**:
  - **Source**: the CS wiki (`cs-uesp-wiki/Category/World Spaces.wiki`) says: "Parent Worldspace: If you select NONE, the worldspace will have its own, editable landscape. Otherwise, this worldspace will use the landscape of its parent." It adds that Climate / Water / Map / Usable Dimensions are "Sharable Data … only available if this worldspace has no Parent". That is also the first in-tree source for #5374/#5388's inherit-all rule.
  - **Data** (`tools/child_land.py`; the structural persistent CELL is excluded):
    - There are 30 children: 25 of Tamriel and 5 of SEWorld.
    - Between them they author 1,989 grid cells. 946 have their own LAND and **1,043 have none**. The parent has LAND at **1,040** of those 1,043.
    - The pattern is a city footprint with its own LAND, inside an empty, LAND-less ring that has no refs (`tools/gridmap.py`). Examples:
      - BrumaWorld: 13 / 43.
      - AnvilWorld: 22 / 35.
      - SkingradWorld: 21 / 34.
      - KvatchPlaza: 21 / 50.
      - TGTempleOfTheEmperorZero: 3 / 296.
    - `landscapelod\generated` ships for 18 root worldspaces (Tamriel 60, SEWorld 40728 in the SI archive, and 16 Oblivion planes) and for **0 of the 30 children**.
    - The children do ship their own object LOD, for example `distantlod\brumaworld_*.lod`. The placement ring already resolves that per worldspace key.
- **Evidence**:
  - A `grep` for any parent-LAND fallback in `cell_loader/`, `streaming/` and `crates/plugin/src/esm/cell/` finds none.
  - `stream_lod_blocks` uses `index.worldspaces[worldspace_key].form_id`, which is the child's.
- **Impact**:
  - From inside every Oblivion city or Imperial City district, nothing exists beyond the authored footprint: neither the parent's terrain nor its terrain LOD.
  - This is visible over the walls, from towers and hillside cities (Bruma, Skingrad, Chorrol, Kvatch), and at the horizon.
  - The stamped `INHERIT_LAND`/`INHERIT_LOD` bits suggest the inheritance is modelled when it is not.
  - **Unsourced**: whether vanilla fills the ring with the parent's full LAND or only with its LOD. Do not guess this. Capture it from the game before choosing.
- **Related**: #5374, #5388, EXT-D1-2026-10-09-01, #5335, #1745, #2735.
- **Suggested Fix**:
  - At the EXAL boundary, resolve a child's terrain source through the WNAM chain: where the child has no LAND, use the parent's LAND for that cell and/or the parent's terrain-LOD quads (form ID 60 / 40728), holed against the child's own LAND cells.
  - Let the stamped bits gate it, which retires the "dead" reading of `INHERIT_LAND`/`INHERIT_LOD` on Oblivion.
  - Pin it with a real-data test: every LAND-less child cell resolves a parent LAND.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
