# Starfield Memory Efficiency — Product Requirements

**Status**: PROPOSED (2026-11-04). Revision 2 folds in review round 1 (§12).
Statements about how an original engine, format, tool or library behaves
cite external sources; a claim that could not be verified is marked
**Phase 0: verify** instead of being stated as fact.

**Scope**: the RAM and VRAM a Starfield interior load costs. In scope: the
plugin record index and the plugin bytes held while it is built, the
collision synthesised from render geometry for every architecture placement,
texture residency, texture bytes held on the host, textures uploaded for
roles no shader reads, the session-wide tables built when a cell finishes
loading, and the memory report that makes each of these visible. The promise
is that the largest vanilla interior, `citycydoniamainlevel`, loads and
holds within the §4 budgets with what is drawn and what the player collides
with unchanged. Out of scope: decoding Starfield's packed Havok collision,
re-encoding textures, the texture pool budget and eviction, mesh level of
detail, Starfield exteriors, and load time except where a memory change
moves it (§4.2).

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Record index residency, shared parse | [Memory Budget § ESM Index](../engine/memory-budget.md#esm-index-cpu-side), [ESM Records](../engine/esm-records.md) |
| On-demand reference locator | [ESM Records](../engine/esm-records.md); Phase 0 writes its section beside the `locate_refr` reverse index |
| Starfield material index | [Memory Budget § Starfield Component Database](../engine/memory-budget.md#starfield-component-database-cpu-side) |
| Collision synthesised from render geometry | [PHYSAL](../engine/physal.md), [Physics](../engine/physics.md) |
| Shared fallback shapes | [Physics](../engine/physics.md); Phase 0 writes its section |
| Texture residency, floors, the streaming switch | [Texture Streaming](../engine/texture-streaming.md), [Texture Streaming PRD](texture-streaming.md) |
| Texture bytes held on the host | [Memory Budget § Texture Prefetch Store](../engine/memory-budget.md#texture-prefetch-store-cpu-side), [§ Texture Registry](../engine/memory-budget.md#texture-registry) |
| Material texture roles and parked slots | [NIFAL](../engine/nifal.md), [Material Abstraction](../engine/material-abstraction.md) |
| Memory ceilings, owner accounting, the `mem:` report | [Memory Budget](../engine/memory-budget.md); Phase 0 writes its "Memory report" section |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `SFM-` prefix.

---

## 1. Product thesis

The owner's framing (2026-11-03): *"We need to make Starfield rendering much
more efficient. We're eating up insane amounts of both RAM and VRAM for
interiors, which means we're loading textures we probably don't need because
of ray tracing doing the math for them."* And: *"everything that comes from
DX10 is guaranteed to be bloated as hell."*

The measurements in §3.2 confirm the symptom and move its causes. A vanilla
Starfield city interior cannot finish loading on the dev machine:
`citycydoniamainlevel` was killed by the out-of-memory daemon in its finish
phase. Most of that RAM is not textures. The largest measured owner is the
record index: the whole plugin's, held for one interior. Two owners that
grow with the cell are measured so far: the fallback collision, a copy of
render triangles per placement, and texture bytes held on the host, which
streaming already shrinks. In VRAM the owner that moves is texture mip
chains, full chains of DX10 records seen at a fraction of their resolution,
which streaming already cuts.

The ray-tracing reading holds for at most one Starfield role. Starfield
materials upload only colour, normal and emissive maps; roughness,
metalness, AO, height, opacity and transmissive maps are never uploaded.
The emissive colour is parked, so every emissive map uploaded is unread.
How many a scene uploads is not yet measured (§3.1 row 6).

The feature is for anyone who loads a Starfield interior on a machine near
the [Memory Budget](../engine/memory-budget.md) targets, and for every later
Starfield milestone that needs a city cell to load at all.

**Corollary:** memory is paid once per thing the player can reach, at the
resolution it is seen at. Never once per placement for data placements
share, never for the whole plugin when one cell is loaded, and never for a
texture no shader reads. The engine will not trade away what is drawn or
what the player collides with to get there.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **RSS** | The engine process's resident set (`VmRSS`); **peak RSS** is `VmHWM` over a run |
| **Max sampled RSS** | The highest `VmRSS` of samples taken every 250 ms; it can miss a short peak, so it reads at or below peak RSS |
| **Process VRAM** | Device memory the engine process holds, as NVML reports it per process |
| **Memory owner** | One of the closed set in §3.1 that every attributed byte is charged to |
| **Fallback collider** | A static trimesh body built from a placement's render geometry because the placement carries no decoded collision (PHYSAL's architecture fallback) |
| **Shape key** | The pair (source mesh, placement scale) that fully determines a fallback collider's shape |
| **Record index** | `EsmIndex` and the per-cell placed-reference tables built from a plugin set ([Memory Budget § ESM Index](../engine/memory-budget.md#esm-index-cpu-side)) |
| **Locator** | What replaces other cells' placed references in an interior session: enough to load a cell's references, or one reference, on demand |
| **Host texture bytes** | Texture data held in host memory: the prefetch store, the pending-upload queue and the retained staging pool |
| **Dead role** | A texture role a material resolves and uploads although no shader reads it under that material's flags |
| **Floor** | A texture's lowest resident levels, as [Texture Streaming](../engine/texture-streaming.md) defines it |
| **Dev machine** | The reference host: RTX 4070 Ti (12 GB), 30 GB RAM, the vanilla Steam install of Starfield |

---

## 3. Core model

### 3.1 Memory owners

Every byte the memory report (SFM-TEL) attributes belongs to one owner,
and each owner has a lifetime. The rule the requirements apply: data that
placements share is held once per shape key or asset, data a cell needs is
held per loaded cell, and only the record index's base records are held per
session.

The inventory below is the state at HEAD on 2026-11-04 (`3918ebe0d`; no
code has changed since `93a0071b6`, where §3.2 was measured).

| # | Owner | Lifetime today | Today | Target |
|---|---|---|---|---|
| 1 | Record index | session | Every record of the plugin set, including every worldspace's exterior cells, for an interior-only session; retained as `LoadedCellIndex` (`cell_loader/load.rs`, `InteriorCellApplyJob::finish`). Size per master in [Memory Budget § ESM Index](../engine/memory-budget.md#esm-index-cpu-side) | base records per session; placed references per loaded cell (Q3) |
| 2 | Plugin bytes during the parse | parse | Each plugin is read whole into a buffer (`std::fs::read`, `cell_loader/load_order.rs:819` in `parse_record_indexes_in_load_order_with_archive`) | not held whole (SFM-IDX-003) |
| 3 | Starfield material index | session | ~115 MB per CDB ([Memory Budget](../engine/memory-budget.md#starfield-component-database-cpu-side)) | unchanged; reported on its own (SFM-TEL-001) |
| 4 | Fallback colliders | per placement | `synthesize_static_trimesh` copies the mesh's positions and indices into a `CollisionShape::TriMesh` per placement (`cell_loader/spawn.rs:391-427`, called from `spawn/mesh_instance.rs:1530`); the physics crate builds a parry trimesh per body with `FIX_INTERNAL_EDGES \| ORIENTED` (`crates/physics/src/config.rs:49`, `convert.rs:366`) | one shape per shape key (Q1) |
| 5 | Texture levels | per handle | Every level of every texture is uploaded unless `render.texture_streaming` is on; it defaults to off (`crates/core/src/settings/builtin.rs:199-206`) | floors plus wanted levels (Q4) |
| 6 | Dead roles | per handle | Starfield emissive maps are filled from the material database's emissive slot (`asset_provider/material/merge.rs:309-315`) while the emissive colour is parked (`merge.rs:404-428`, #5283 / #5503); how many a scene uploads is unmeasured, and the parking comment says the arm fires on 0 referenced materials (`merge.rs:420-423`). FO4 lighting and wrinkle maps are uploaded and documented as deliberately unsampled (`crates/renderer/src/vulkan/material.rs:316-329`). The greyscale lookup map is uploaded whatever the palette flags say and sampled only under them (`triangle.frag:1164-1167`) | not uploaded until read (Q5); bytes per role on the `mem:` line |
| 7 | Cell-finish tables | session | Scene, package and linked-route tables walk every placed reference of the plugin when a cell finishes (`asset_provider/script.rs:1462-1470`, called from `cell_loader/load.rs:764`; `script.rs:1484` logs 151,654 routes across 193,311 refs for `Starfield.esm`) | sized on the `mem:` line; built per cell above a threshold (SFM-IDX-005) |
| 8 | Geometry and acceleration structures | per unique mesh | Vertex and index buffers and BLAS, accounted in [Memory Budget](../engine/memory-budget.md); not separated in a per-cell report | reported (SFM-TEL-001) |
| 9 | Host texture bytes | per load; staging pool per session | The prefetch store holds extracted DDS bytes for streamed cells, capped at 256 MiB of ready bytes ([Memory Budget § Texture Prefetch Store](../engine/memory-budget.md#texture-prefetch-store-cpu-side)); the pending-upload queue holds them until the cell's flush; the staging pool keeps 128 MB after a flush ([§ Texture Registry](../engine/memory-budget.md#texture-registry)). `CityAkilaCityCoeEstate`'s RSS after load is 567 MB higher with streaming off (§3.2), and no owner here explains it yet | reported (SFM-TEL-001); Phase 1 attributes the streaming gap |

### 3.2 Measurements that motivate this PRD

Measured 2026-11-03 on the dev machine. The budgets in §4 are absolute so a
different host state does not move them. Phase 0 re-measures every value
here with a checked-in harness, reading peak RSS as `VmHWM`; until then they
are not published numbers.

| Condition | Value |
|---|---|
| Build | release, `93a0071b6` |
| Plugins | `Starfield.esm` alone (no DLC masters), vanilla meshes, materials and texture archives |
| Streaming switch | isolated per run with `BYROREDUX_SETTINGS_PATH` |
| RSS | `VmRSS` sampled every 250 ms; `VmHWM` read once where noted |
| VRAM | `nvidia-smi` per process |
| Host RAM held by a tmpfs during the runs | about 13 GB, which is why Cydonia reached the out-of-memory daemon |

| Scene | REFRs | Streaming | Max sampled RSS | RSS after load | Process VRAM | Textures resident | Fixed colliders (spawn probe) |
|---|---|---|---|---|---|---|---|
| `aaaMarkers` | 113 | off | 4,456 MB (`VmHWM` 4,613 MB) | 3,697 MB | — | 71.0 MiB | 843 |
| `CityAkilaCityCoeEstate` | 2,442 | off | 4,737 MB | 4,729 MB | 2,252 MB | 863.2 MiB | 5,551 |
| `CityAkilaCityCoeEstate` | 2,442 | on | 4,511 MB | 4,162 MB | 1,340 MB | 98.9 MiB | 5,551 |
| `CityAkilaCityCoeEstate`, fallback colliders disabled | 2,442 | on | 4,501 MB | 3,846 MB | 1,268 MB | — | none built |
| `citycydoniamainlevel` | 27,898 | on | killed at 6,240 MB in the finish phase | — | 1,683 MB at the kill | — | — |

In `CityAkilaCityCoeEstate` the temporary counter below read the same
fallback count as the spawn probe's fixed-collider total, so every fixed
collider there is a fallback. `aaaMarkers` was not counted, so its fallback
share is unknown.

The fallback colliders of `CityAkilaCityCoeEstate`, from a temporary counter
that was not committed:

| Measure | Value |
|---|---|
| Fallback colliders | 5,551 |
| Distinct source meshes | 1,959 |
| Distinct shape keys | 2,370 |
| Positions and indices copied, all colliders | 46.4 MB |
| … for distinct shape keys only | 22.3 MB |
| RSS cost of the colliders (with − without, streaming on) | 345 MB |
| RSS cost ÷ copied bytes | ≈ 7.4 |
| RSS cost per placed reference | ≈ 141 KB |

The RSS cost is several times the copied bytes. That is consistent with
each body building its own parry trimesh: its vertices and indices, its
BVH, and the pseudo-normals parry computes under `ORIENTED` or
`FIX_INTERNAL_EDGES` (parry3d 0.31.1, `TriMesh::set_flags`,
`src/shape/trimesh.rs:811-813`). The split between the engine's copy and
the physics build needs the `mem:` line; Phase 1 measures it (SFM-TEL-003).

The `citycydoniamainlevel` trace, streaming on:

| Point in the trace | RSS |
|---|---|
| Reference walk starts | 3,144 MB |
| Last reference-walk log line | 5,947 MB |
| Growth over the walk | ≈ 2.8 GB |
| Killed in the finish phase | 6,240 MB |
| Growth in the finish phase before the kill | ≈ 0.3 GB |
| Coe Estate's collider cost per reference × Cydonia's references | ≈ 3.9 GB |

The Coe Estate rate over-predicts Cydonia's whole walk growth, so the
collider cost per reference differs between the cells, and the growth
cannot yet be split between colliders and other owners. Phase 1 attributes
it on a scene that fits.

The `aaaMarkers` record parse, streaming off:

| Measure | Value |
|---|---|
| Max sampled RSS during the parse | 4,456 MB |
| `VmHWM` at the end of the run | 4,613 MB |
| RSS fall when the parse ended and the plugin buffer was released | ≈ 1.2 GB |
| Material index, retained / peak (design figures, not re-measured) | ~115 MB / ~247 MB ([Memory Budget § Starfield Component Database](../engine/memory-budget.md#starfield-component-database-cpu-side)) |

The material index builds on a background thread during the parse, so the
trace cannot separate the two. The design's figures stand until Phase 1
reports the material index as its own owner.

Texture streaming on BA2 scenes, measured with the `tex-stream:` bench line:

| Scene | Source of the figures | Textures resident, off → on | Deficits / churn (on) | Settle after a camera stop |
|---|---|---|---|---|
| FO4 `MedTekResearch01` | Texture Streaming Phase 2 runs, 2026-11-03 (`93a0071b6`), to be published under #6629 | 766 → 136 MiB | 0 / 0 | 0.99–1.05 s |
| Starfield `CityAkilaCityCoeEstate` | the runs above | 863.2 → 98.9 MiB | 0 / 0 | 775 ms |

---

## 4. Goals and non-goals

### 4.1 Goals

- **SFM-G-001 — Cydonia loads.** `citycydoniamainlevel` loads, runs a
  300-frame bench and holds under `--bench-hold` on the dev machine with
  peak RSS at or under the Q6 RAM budget.
- **SFM-G-002 — Interior VRAM within the engine budget.** The same run's
  process VRAM stays at or under the Q6 VRAM budget.
- **SFM-G-003 — Collision is paid per shape, not per placement.** Fallback
  collider bytes in `CityAkilaCityCoeEstate` are at most 1.2 times the bytes
  of its distinct shape keys.
- **SFM-G-004 — An interior session does not hold the world.** `aaaMarkers`'
  RSS after load drops by at least the record-index share that Phase 0
  attributes to other cells' placed references.
- **SFM-G-005 — Nothing seen or touched changes.** Each acceptance scene's
  image A/B and collision probe match the HEAD control (§7.2).

### 4.2 Non-goals

- **SFM-NG-001.** Decoding Starfield's packed Havok collision, which PHYSAL
  classifies as a tagfile and keeps out of scope (#6302); this PRD makes the
  existing fallback cheaper instead.
- **SFM-NG-002.** Re-encoding uncompressed or oversized DX10 records into
  block-compressed formats, at load or offline. That is the
  [Asset Preprocessing PRD](asset-preprocessing.md)'s work (M82). Phase 0
  measures how much of a Starfield interior's texture bytes such records
  are, so that PRD can weigh it.
- **SFM-NG-003.** The texture pool budget, eviction and per-frame promotion
  cap. They are Texture Streaming Phase 3 (TXS-BUD); this PRD only asks when
  the streaming switch is on by default.
- **SFM-NG-004.** Mesh level of detail and geometry compression, owned by the
  [Virtual Geometry PRD](virtual-geometry.md).
- **SFM-NG-005.** Skipping environment cubemaps. Under ray tracing they
  still serve beyond ray reach and for eyes, and their presence gates which
  surfaces trace reflection rays (`triangle.frag:3083-3096`); they are a
  few hundred KiB each.
- **SFM-NG-006.** Starfield exteriors and planets: the engine loads no
  Starfield exterior today
  ([Game Compatibility § Tier 2](../engine/game-compatibility.md#tier-2-walkable-interior-bring-up)).
- **SFM-NG-007.** Load time, except the texture floor-ready time Texture
  Streaming already reports.

---

## 5. Requirements by area

### 5.1 Memory report (`SFM-TEL`)

- **SFM-TEL-001.** A per-session bench line (new, `mem:`) MUST carry RSS,
  peak RSS, process VRAM, and the bytes charged to each §3.1 owner, with
  the texture-level and dead-role owners split by role. Today no line
  exists; `tex-stream:` carries texture bytes only
  (`texture_streamer::tex_stream_bench_line`).
- **SFM-TEL-002.** Each cell load MUST log its RSS and process VRAM deltas
  beside the existing `Cell load phases` line (`cell_loader/load.rs`).
- **SFM-TEL-003.** The fallback-collider owner MUST report its collider
  count, distinct shape keys, and bytes split between the engine's copied
  geometry and the physics crate's built trimeshes, on the `mem:` line and
  in a console command. Parry reports a trimesh's own bytes through
  `TriMesh::heap_memory_size` (parry3d 0.31.1, `src/shape/trimesh.rs`).
- **SFM-TEL-004.** The sum of the attributed owners SHOULD come within
  15 % of RSS after load on every §7.2 scene; the unattributed remainder is
  printed as its own field.

### 5.2 Fallback collision (`SFM-COL`)

- **SFM-COL-001.** Fallback colliders whose shape keys are equal MUST share
  one shape: one copy of positions and indices and one physics-side
  trimesh. A Vulkan-free test (new,
  `fallback_colliders_share_one_shape_per_key`) spawns two placements of one
  mesh at one scale and asserts both bodies reference the same shape.
  Today each placement holds its own copy (§3.1 row 4).
- **SFM-COL-002.** `CollisionShape::TriMesh` MUST NOT own a per-placement
  copy of shared geometry; the component refers to the shared shape.
- **SFM-COL-003.** A shared shape MUST be released when its last body is
  removed, so the collider bytes return to the pre-load value after a cell
  unloads. `ownership_sample.rs` MUST sample the collider bytes beside
  `physics_bodies`, the owner the `m-exteriors.sh` soak already gates.
- **SFM-COL-004.** The flags fallback colliders are built with MUST be the
  ones decision Q1 names, as the SFM-TEL-003 command prints them.
- **SFM-COL-005.** The collision the player meets MUST be unchanged: the
  `spawn-probe:` result, collider count and ground height for each §7.2
  scene equal the HEAD control, and the existing character-controller tests
  pass.

### 5.3 Record index (`SFM-IDX`)

- **SFM-IDX-001.** Phase 0 MUST publish the record index's bytes by
  category for `Starfield.esm`, separating placed references by cell kind
  (the loaded interior, other interiors, exterior cells, persistent
  references), by extending `crates/plugin/examples/esm_dim8_bench`.
- **SFM-IDX-002.** In a session that loads only interiors, the record index
  MUST NOT hold the placed references of cells other than the loaded ones
  and the persistent references; the locator stands in for them (decision
  Q3). A test (new, `interior_session_index_holds_no_other_cells_references`)
  parses a plugin with two interiors and an exterior cell, loads one
  interior, and asserts the index holds no placed reference of the other two
  beyond the persistent ones.
- **SFM-IDX-003.** The record parse MUST NOT hold a plugin's bytes whole: on
  `Starfield.esm`, the parse's peak RSS minus its RSS after the parse MUST be
  at most a quarter of the file size, as `esm_dim8_bench` reports it. Today
  the whole file is read into a buffer (§3.1 row 2).
- **SFM-IDX-004.** A lookup that needs a reference outside the loaded cells
  (a quest alias, a package target, a linked route, a door's destination)
  MUST resolve it through the locator. A test (new,
  `unloaded_cell_reference_resolves_through_the_locator`) resolves one of
  each kind with the referenced cell unloaded.
- **SFM-IDX-005.** Any cell-finish table (§3.1 row 7) the `mem:` line sizes
  above 50 MB on `Starfield.esm` MUST be built per loaded cell or on demand.
- **SFM-IDX-006.** The SFM-IDX-003 figure MUST be published per master in
  [Memory Budget § ESM Index](../engine/memory-budget.md#esm-index-cpu-side).
- **SFM-IDX-007.** Resolving one reference through the locator MUST take at
  most 1 ms on the dev machine. An `esm_dim8_bench` mode (new) resolves
  every placed reference of `Starfield.esm` through it and prints the
  maximum.

### 5.4 Textures (`SFM-TEX`)

- **SFM-TEX-001.** The streaming switch's default for textures read from
  BA2 DX10 records MUST follow decision Q4, decided by texture source kind
  and never by game. Today it is off for every source (§3.1 row 5).
- **SFM-TEX-002.** A material MUST NOT resolve or upload a texture role no
  shader reads under its flags. A test (new, `dead_roles_are_not_resolved`)
  covers each §3.1 row 6 role. When a shader starts reading a role, the
  same change turns its upload on, and the test fails if one side moves
  without the other.
- **SFM-TEX-003.** Starfield emissive maps MUST follow decision Q5: not
  uploaded while the emissive colour stays parked, or uploaded with the
  colour wired.
- **SFM-TEX-004.** Phase 0 MUST publish, for `CityAkilaCityCoeEstate` and
  `citycydoniamainlevel`, the loaded textures' bytes by DXGI format and by
  streamability (floor-readable, single-mip **whole**, chain within the
  floor), from an example over the `tex.loaded` paths and the BA2 DX10
  record headers, so SFM-NG-002's owner can weigh re-encoding against
  measured bytes. Bytes by role come from the `mem:` line (SFM-TEL-001).

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | **Phase 0: verify** — the original collides against the Havok bodies authored in each model's tagfile; settled by a Havok tagfile or Creation Kit collision-authoring reference | Architecture collides against its render triangles, now shared per shape key (SFM-COL-001) | The per-placement copies | A shared shape mutated per placement would move every sharer; shapes are immutable after build → SFM-COL-002, SFM-COL-005 |
| 2 | **Phase 0: verify** — whether the original keeps every cell's references of a plugin resident in an interior session; settled by Creation Kit or UESP documentation of cell loading | Other cells' references load on demand through the locator (SFM-IDX-002) | Holding the world for one interior | A quest, package, scene or door that reads an unloaded reference resolves nothing → SFM-IDX-004 |
| 3 | **Phase 0: verify** — whether the original streams texture levels; settled by a Creation Engine 2 rendering reference or its INI texture settings documented on UESP | Floors first, finer levels when wanted (Texture Streaming), on by default for BA2 DX10 sources per Q4 | Full chains for surfaces seen small | Visible sharpening during the settle after a load or a camera stop (§3.2 settle column) → TXS-WANT requirements, SFM-G-005 |
| 4 | — | Dead roles are not uploaded (SFM-TEX-002) | Uploads no shader reads | A later shader change that reads a role but forgets the upload samples the fallback → SFM-TEX-002's paired test |
| 5 | — | A lookup into an unloaded cell reads the plugin through the locator (SFM-IDX-004) | — | A quest, package or door that touches an unloaded cell reads the plugin on the main thread mid-frame and hitches → SFM-IDX-007 |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | no | Collision is authored `bhk` and decoded; its index is ~1.2 GB or less ([Memory Budget § ESM Index](../engine/memory-budget.md#esm-index-cpu-side)); BSA textures are Texture Streaming Phase 4 |
| Fallout 3 | no | As Oblivion |
| Fallout: New Vegas | no | As Oblivion |
| Skyrim (LE / SE / AE) | no | As Oblivion |
| Fallout 4 | yes, wave 2 | Shares the fallback collider path for packed-Havok architecture (PHYSAL), the BA2 DX10 streaming default (Q4) and the lighting / wrinkle / lookup dead roles; acceptance `MedTekResearch01`. The shared mechanisms carry no per-game branch |
| Fallout 76 | no | No cell loader ([Game Compatibility](../engine/game-compatibility.md)), so no load to budget |
| Starfield | yes, wave 1 | The owner's target; Tier 2 walkable interiors |

### 7.2 Acceptance

Every scenario runs the release build with `--bench-frames 300
--bench-hold`, the streaming switch isolated through
`BYROREDUX_SETTINGS_PATH`, `BYRO_DEBUG_SERVER=1` for `byro-dbg`, and
`VmHWM` read at `bench-hold` for peak RSS. It reads the `mem:`,
`tex-stream:` and `bench:` lines. Phase 0 checks the driver in as
`docs/smoke-tests/sf-memory.sh` and indexes it in the smoke-test README.
Starfield runs use `Starfield.esm` alone with the vanilla meshes, materials
and texture archives; no game data is checked in.

| # | Scenario | Data | Passes when |
|---|---|---|---|
| A1 | Cydonia memory | Starfield `citycydoniamainlevel` | the run reaches `bench-hold`; peak RSS and process VRAM at or under the Q6 budgets (SFM-G-001, SFM-G-002) |
| A2 | Shared fallback collision | Starfield `CityAkilaCityCoeEstate` | collider bytes ≤ 1.2 × distinct-key bytes; collider count and `spawn-probe:` equal the HEAD control (SFM-G-003, SFM-COL-005) |
| A3 | Interior session index | Starfield `aaaMarkers` | RSS after load drops by at least the Phase 0 share of other cells' references (SFM-G-004) |
| A4 | Unloaded references | `cargo test` `interior_session_index_holds_no_other_cells_references` and `unloaded_cell_reference_resolves_through_the_locator`; the `esm_dim8_bench` locator mode on `Starfield.esm` | both tests pass; the bench's maximum lookup is at most 1 ms (SFM-IDX-002, SFM-IDX-004, SFM-IDX-007) |
| A5 | Dead roles | `cargo test` `dead_roles_are_not_resolved` | passes (SFM-TEX-002) |
| A6 | Image A/B | Starfield `CityAkilaCityCoeEstate`, FO4 `MedTekResearch01`, pinned pose, `screenshot` on the change and on the HEAD control | pixel delta within the control's run-to-run noise (SFM-G-005) |
| A7 | FO4 parity | FO4 `MedTekResearch01` | collider count and `spawn-probe:` equal the HEAD control; the `mem:` line's collider bytes fall (SFM-COL-005) |
| A8 | Unload returns memory | `docs/smoke-tests/m-exteriors.sh fo4 soak`, with the collider-bytes owner Phase 2 adds | collider bytes return to baseline each cycle (SFM-COL-003) |
| A9 | Parse peak | `esm_dim8_bench` on `Starfield.esm` | peak minus after-parse RSS is at most a quarter of the file size, and the figure is in Memory Budget (SFM-IDX-003, SFM-IDX-006) |

---

## 8. Metrics

The per-scene numbers go in a Starfield interior memory table in
[Memory Budget](../engine/memory-budget.md), created by Phase 0's "Memory
report" section and measured per
[ROADMAP § How to run a bench](../../ROADMAP.md#how-to-run-a-bench), with
their repro command beside them. ROADMAP adds no rows: its Starfield
compatibility-matrix row gains a link to that table, because ROADMAP sits
within a few lines of the 800-line ceiling the #5111 hygiene test enforces
(`byroredux/src/workspace_hygiene_tests.rs:933`). The ceilings stay in
[Memory Budget](../engine/memory-budget.md).

| Metric | Definition | Granularity |
|---|---|---|
| Peak RSS | `VmHWM` over the run | per scene |
| RSS after load | `VmRSS` at `bench-hold` | per scene |
| Process VRAM | NVML per-process used memory at `bench-hold` | per scene |
| Owner bytes | bytes charged to each §3.1 owner on the `mem:` line | per scene, per owner |
| Unattributed share | (RSS after load − Σ owner bytes) ÷ RSS after load | per scene |
| Collider sharing ratio | fallback collider bytes ÷ distinct-key bytes | per scene |
| Index share of other cells | record-index bytes for references outside the loaded cells ÷ record-index bytes | per master |
| Parse peak | peak RSS − RSS after the parse, ÷ file size | per master |
| Locator lookup | maximum time to resolve one reference through the locator | per master |
| Texture bytes by class | loaded texture bytes by DXGI format, role and streamability | per scene |

---

## 9. Delivery sequence

M39 Texture Streaming Phase 2 must exit before Q4 can flip the switch's
default; nothing else here waits on another milestone. Phases run in the
order that unblocks SFM-G-001 soonest: the report first (nothing is
attributed without it), then collision (Cydonia's largest measured cost
that grows with the cell), then textures, then the record index (the
largest change).

**Phase 0 — Attribute and decide (no engine code).**
- Check in `docs/smoke-tests/sf-memory.sh`: the RSS and VRAM trace,
  `VmHWM` at `bench-hold`, and the bench lines, for every §7.2 scene.
- Re-measure §3.2 with it, on a host without the tmpfs load.
- Publish SFM-IDX-001 (`esm_dim8_bench` extended, including the after-parse
  RSS SFM-IDX-003 reads) and SFM-TEX-004 (an example over the `tex.loaded`
  paths and the BA2 DX10 record headers).
- Close every **Phase 0: verify** item; the owner answers §10.
- Once §10 is decided, write the three missing design sections: "Memory
  report" in Memory Budget (the `mem:` line, its owners, the Starfield
  interior memory table), shared fallback shapes in Physics, and the
  reference locator in ESM Records.

Exit: the script, the two tables and the three sections are checked in,
§3.2 is replaced by the re-measured table with its command, and every §10
item is **Decided**.

**Phase 1 — Memory report.** SFM-TEL-001…004. Exit: the `mem:` line on
every §7.2 scene, with the unattributed share at or under 15 % on
`CityAkilaCityCoeEstate` and `MedTekResearch01`, and these figures
published in Memory Budget: the fallback collider bytes split between the
engine's copy and the physics build, the cell-finish table sizes, the
material index on its own, the host texture bytes behind Coe Estate's
streaming gap, and the dead-role bytes per role, Starfield emissive maps
included.

**Phase 2 — Shared fallback collision.** SFM-COL-001…005. Exit: A2, A7 and
A8 pass, and A1 is run and its peak RSS published whether or not it passes.

**Phase 3 — Textures.** SFM-TEX-001…003. Exit: A5 and A6 pass, and the
process VRAM of `CityAkilaCityCoeEstate` is published against the HEAD
control.

**Phase 4 — Record index.** SFM-IDX-002…007. Exit: A1, A3, A4 and A9 pass,
and the Starfield interior memory table is published (§8).

---

## 10. Decisions required

1. **Q1 — How cheap may the fallback collider be?** Recommended: share one
   shape per shape key and keep today's build flags. Collision stays
   identical, and Coe Estate's copied triangles halve (§3.2).
   Alternative (a): also drop `ORIENTED` and `FIX_INTERNAL_EDGES` for
   fallback colliders. Parry builds pseudo-normals under either flag
   (parry3d 0.31.1, `src/shape/trimesh.rs:811-813`), so only dropping both
   saves them; SFM-TEL-003's split shows how much that is. The cost: parry
   documents `FIX_INTERNAL_EDGES` as the fix for contact normals that cause
   bumps, especially on flat surfaces (`src/shape/trimesh.rs:495-501`), and
   `ORIENTED` as treating the mesh as outward-facing (`:473-476`). The
   engine sets both so collision matches what rapier 0.22 built
   (`crates/physics/src/config.rs:43-49`).
   Alternative (b): build fallback colliders from a coarser level of the
   source mesh. There are fewer triangles, but surfaces the player stands on
   move away from what is drawn.
2. **Q2 — Build fallback colliders only near the player?** Recommended: no,
   unless Phase 2 shows SFM-G-001 still fails with shared shapes. This
   matches today: every placement in a loaded cell collides. Alternative:
   build them within a radius of the player and of each active actor or
   dynamic body. Memory then scales with what is near rather than with the
   cell, but distant physics (a thrown object, an NPC across the city)
   needs its own radius, and a body crossing the radius edge needs its
   collider in time.
3. **Q3 — What the record index holds in an interior session.**
   Recommended: base records whole for the session; placed references only
   for the loaded cells plus persistent references, with other cells'
   references loaded on demand through a locator of plugin offsets. The locator extends the
   existing lazy reverse index `locate_refr`
   (`crates/plugin/src/esm/cell/index.rs:834`, #6125) rather than adding a
   second one. This is the largest attributable RAM saving.
   Alternative (a): keep the index whole and stop holding the plugin bytes
   during the parse (SFM-IDX-003 only). This is simpler, but only the parse
   peak drops.
   Alternative (b): keep today's index and rely on Phases 2–3. This is the
   smallest change, but SFM-G-004 and SFM-IDX-002 are withdrawn.
4. **Q4 — When the streaming switch defaults on for BA2 DX10 textures.**
   Recommended: on for textures read from BA2 DX10 records once Texture
   Streaming Phase 2 exits (its A6 and A7 pass on its FO4 and Starfield
   scenes), leaving BSA and loose textures to the Phase 4 plan. Both BA2
   scenes measured so far hold a fraction of the resident bytes with zero
   deficits and churn (§3.2). This amends the
   [Texture Streaming PRD](texture-streaming.md)'s Phase 4 exit, which flips
   the default for every source at once, so that PRD takes a revision with
   it. Alternative: keep that plan. BA2 sources wait for the BSA wave, and
   Cydonia's VRAM goal waits with them.
5. **Q5 — Textures uploaded for roles no shader reads.** Recommended: a role
   is resolved and uploaded only while a shader reads it. For Starfield
   emissive maps, that means not uploading them while the emissive colour
   stays parked (#5283 / #5503), and uploading them in the same change that
   wires it. For the FO4 lighting and wrinkle maps, it revises the upload
   decision recorded at `crates/renderer/src/vulkan/material.rs:316-329`.
   How many Starfield emissive maps a scene uploads is unmeasured (§3.1
   row 6). If Phase 1 counts none, the Starfield half of this answer saves
   nothing and only the FO4 roles move.
   Alternative: wire the Starfield emissive colour now. Glowing Starfield
   surfaces then light up, but that is a look change with its own
   calibration problem (the parked luminance is 100×–10,000× the other
   games' range, `merge.rs:404-428`), outside a memory PRD.
6. **Q6 — The budgets.** Recommended: peak RSS ≤ 8 GB and process VRAM ≤
   4 GB for `citycydoniamainlevel` at the default 1080p/FSR configuration.
   The VRAM figure is the engine budget in
   [Memory Budget](../engine/memory-budget.md); the RSS figure leaves a
   16 GB machine room for the OS and other programs. Alternative: peak RSS
   ≤ 6 GB, which likely forces Q3's recommendation and Q2.

---

## 11. Document map

| Need | Go to |
|---|---|
| Record index sizes per master, the shared parse | [Memory Budget § ESM Index](../engine/memory-budget.md#esm-index-cpu-side) |
| The reverse index the locator extends | [ESM Records](../engine/esm-records.md) |
| Host texture bytes: prefetch store, staging pool | [Memory Budget § Texture Prefetch Store](../engine/memory-budget.md#texture-prefetch-store-cpu-side), [§ Texture Registry](../engine/memory-budget.md#texture-registry) |
| The fallback collision policy and the packed-Havok scope | [PHYSAL](../engine/physal.md) |
| Rigid bodies, colliders and the physics sync | [Physics](../engine/physics.md) |
| Texture floors, the wanted-mip rule, the BA2 census | [Texture Streaming](../engine/texture-streaming.md) |
| The streaming promise, its phases and acceptance | [Texture Streaming PRD](texture-streaming.md) |
| Parked material roles | [NIFAL](../engine/nifal.md) |
| Re-encoding textures offline | [Asset Preprocessing PRD](asset-preprocessing.md) |
| Starfield's tier | [Game Compatibility](../engine/game-compatibility.md#tier-2-walkable-interior-bring-up) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-03 | First draft | — |
| 2 | 2026-11-04 | Review round 1 folded in: HEAD citations (config line, settle-time source, Cydonia growth, peak-RSS definition, collider column), emissive premise, parry and tagfile sources (the parry check also corrected Q1 (a): pseudo-normals are built under `ORIENTED` too), IDX-002/003 and COL-004 observables, host texture-bytes owner, missing designs, Phase 0 scope, ROADMAP ceiling, locator risk (new SFM-IDX-006/007, A9), source-kind default, numbers in prose | claude: revise |
