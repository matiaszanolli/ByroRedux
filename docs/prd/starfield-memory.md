# Starfield Memory Efficiency — Product Requirements

**Status**: PROPOSED (2026-11-03). First draft, written from the owner's
request and the measurements in §3.2. Statements about how an original
engine, format or tool behaves cite external sources; a claim that could not
be verified is marked **Phase 0: verify** instead of being stated as fact.

**Scope**: the RAM and VRAM a Starfield interior load costs. In scope: the
plugin record index and the plugin bytes held while it is built, the
collision synthesised from render geometry for every architecture placement,
texture residency, textures uploaded for roles no shader reads, the
session-wide tables built when a cell finishes loading, and the memory
report that makes each of these visible. The promise is that the largest
vanilla interior, `citycydoniamainlevel`, loads and holds within the §4
budgets with what is drawn and what the player collides with unchanged.
Out of scope: decoding Starfield's Havok tagfiles, re-encoding textures,
the texture pool budget and eviction, mesh level of detail, Starfield
exteriors, and load time except where a memory change moves it (§4.2).

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Record index residency, shared parse | [Memory Budget § ESM Index](../engine/memory-budget.md#esm-index-cpu-side), [ESM Records](../engine/esm-records.md) |
| Starfield material index | [Memory Budget § Starfield Component Database](../engine/memory-budget.md#starfield-component-database-cpu-side) |
| Collision synthesised from render geometry | [PHYSAL](../engine/physal.md), [Physics](../engine/physics.md) |
| Texture residency, floors, the streaming switch | [Texture Streaming](../engine/texture-streaming.md), [Texture Streaming PRD](texture-streaming.md) |
| Material texture roles and parked slots | [NIFAL](../engine/nifal.md), [Material Abstraction](../engine/material-abstraction.md) |
| Memory ceilings and owner accounting | [Memory Budget](../engine/memory-budget.md) |

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
Starfield city interior cannot finish loading on the 30 GB dev machine:
`citycydoniamainlevel` was killed by the out-of-memory daemon at 6.2 GB of
resident memory, still in its finish phase. Most of that RAM is not
textures. It is the whole plugin's record index, held for one interior, and
a full copy of render triangles per placement for the fallback collision.
Textures dominate VRAM: full mip chains of DX10 records seen at a fraction
of their resolution. The ray-tracing reading holds for one role only.
Starfield materials already upload just colour, normal and emissive, and
emissive contributes nothing today (§3.1 row 6). Roughness, metalness, AO,
height, opacity and transmissive maps are never uploaded.

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
| **Process VRAM** | Device memory the engine process holds, as NVML reports it per process |
| **Memory owner** | One of the closed set in §3.1 that every attributed byte is charged to |
| **Fallback collider** | A static trimesh body built from a placement's render geometry because the placement carries no decoded collision (PHYSAL's architecture fallback) |
| **Shape key** | The pair (source mesh, placement scale) that fully determines a fallback collider's shape |
| **Record index** | `EsmIndex` and the per-cell placed-reference tables built from a plugin set ([Memory Budget § ESM Index](../engine/memory-budget.md#esm-index-cpu-side)) |
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

The inventory below is the state at HEAD on 2026-11-03 (`93a0071b6`).

| # | Owner | Lifetime today | Today | Target |
|---|---|---|---|---|
| 1 | Record index | session | Every record of the plugin set, including every worldspace's exterior cells, for an interior-only session; retained as `LoadedCellIndex` (`cell_loader/load.rs`, `InteriorCellApplyJob::finish`). Size per master in [Memory Budget § ESM Index](../engine/memory-budget.md#esm-index-cpu-side) | base records per session; placed references per loaded cell (Q3) |
| 2 | Plugin bytes during the parse | parse | Each plugin is read whole into a buffer (`std::fs::read`, `cell_loader/load_order.rs` in `parse_record_indexes_in_load_order_with_archive`) | not held whole (SFM-IDX-003) |
| 3 | Starfield material index | session | ~115 MB per CDB ([Memory Budget](../engine/memory-budget.md#starfield-component-database-cpu-side)) | unchanged; not a target |
| 4 | Fallback colliders | per placement | `synthesize_static_trimesh` copies the mesh's positions and indices into a `CollisionShape::TriMesh` per placement (`cell_loader/spawn.rs:391-427`, called from `spawn/mesh_instance.rs:1530`); the physics crate builds a parry trimesh per body with `FIX_INTERNAL_EDGES \| ORIENTED` (`crates/physics/src/config.rs:51`, `convert.rs:366`) | one shape per shape key (Q1) |
| 5 | Texture levels | per handle | Every level of every texture is uploaded unless `render.texture_streaming` is on; it defaults to off (`crates/core/src/settings/builtin.rs:199-206`) | floors plus wanted levels (Q4) |
| 6 | Dead roles | per handle | Starfield emissive maps are uploaded (`asset_provider/material/merge.rs:309-315`) while the emissive colour is parked (`merge.rs:404-428`, #5283 / #5503); FO4 lighting and wrinkle maps are uploaded and documented as deliberately unsampled (`crates/renderer/src/vulkan/material.rs:316-329`); the greyscale lookup map is uploaded whatever the palette flags say and sampled only under them (`triangle.frag:1164-1167`) | not uploaded until read (Q5) |
| 7 | Cell-finish tables | session | Scene, package and linked-route tables walk every placed reference of the plugin when a cell finishes (`asset_provider/script.rs:1484` logs 151,654 routes across 193,311 refs for `Starfield.esm`) | **Phase 0: verify** their size |
| 8 | Geometry and acceleration structures | per unique mesh | Vertex and index buffers and BLAS, accounted in [Memory Budget](../engine/memory-budget.md); not separated in a per-cell report | reported (SFM-TEL-001) |

### 3.2 Measurements that motivate this PRD

Measured 2026-11-03 on the dev machine at `93a0071b6`, release build,
`Starfield.esm` alone (no DLC masters), the vanilla meshes, materials and
texture archives, streaming switch isolated with `BYROREDUX_SETTINGS_PATH`.
RSS was sampled every 250 ms and VRAM read from `nvidia-smi` per process.
The host had about 13 GB of its RAM held by a tmpfs at the time, which is
why Cydonia hit the out-of-memory daemon. The budgets in §4 are absolute so
that a different host state does not move them. Phase 0 re-measures these
values with a checked-in harness; until then they are not ROADMAP numbers.

| Scene | REFRs | Streaming | Peak RSS | RSS after load | Process VRAM | Textures resident | Fallback colliders |
|---|---|---|---|---|---|---|---|
| `aaaMarkers` | 113 | off | 4,456 MB | 3,697 MB | — | 71.0 MiB | 843 |
| `CityAkilaCityCoeEstate` | 2,442 | off | 4,737 MB | 4,729 MB | 2,252 MB | 863.2 MiB | 5,551 |
| `CityAkilaCityCoeEstate` | 2,442 | on | 4,511 MB | 4,162 MB | 1,340 MB | 98.9 MiB | 5,551 |
| `CityAkilaCityCoeEstate`, fallback colliders disabled | 2,442 | on | 4,501 MB | 3,846 MB | 1,268 MB | — | 0 |
| `citycydoniamainlevel` | 27,898 | on | killed at 6,240 MB in the finish phase | — | 1,683 MB at the kill | — | — |

The fallback colliders of `CityAkilaCityCoeEstate`, from a temporary counter
that was not committed:

| Measure | Value |
|---|---|
| Fallback colliders | 5,551 |
| Distinct source meshes | 1,959 |
| Distinct shape keys | 2,370 |
| Positions and indices copied, all colliders | 46.4 MB |
| … for distinct shape keys only | 22.3 MB |
| RSS cost of the colliders (with − without) | 345 MB |

The RSS cost is about 7.4 times the copied triangles, which is consistent
with each body's own parry trimesh, its BVH, and the topology and
pseudo-normals that `FIX_INTERNAL_EDGES` builds. **Phase 0: verify** the
split between the engine's copy and the physics crate's build. The
Cydonia trace grew about 3.1 GB over its reference walk, roughly linear in
references. At the Coe Estate rate of about 141 KB per reference, the
fallback colliders account for most of it. **Phase 0: verify** this
attribution on a scene that fits.

In the `aaaMarkers` trace the record parse peaked at 4,456 MB and RSS fell
by about 1.2 GB once the plugin buffer was released. The material index
builds on a background thread at the same time, so the trace cannot
separate it from the parse. The design's own measurement (~115 MB retained,
~247 MB peak) stands until Phase 0 re-measures it.

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

- **SFM-NG-001.** Decoding Starfield's Havok tagfile collision. PHYSAL
  records it as out of scope (#6302); this PRD makes the existing fallback
  cheaper instead.
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

- **SFM-TEL-001.** A per-session bench line (new, `mem:`) MUST carry RSS
  and peak RSS, process VRAM, and the bytes charged to each §3.1 owner.
  Today no line exists; `tex-stream:` carries texture bytes only
  (`texture_streamer::tex_stream_bench_line`).
- **SFM-TEL-002.** Each cell load MUST log its RSS and process VRAM deltas
  beside the existing `Cell load phases` line (`cell_loader/load.rs`).
- **SFM-TEL-003.** The fallback-collider owner MUST report its collider
  count, distinct shape keys and bytes, on the `mem:` line and in a console
  command.
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
- **SFM-COL-004.** The fallback's trimesh build flags MUST follow decision
  Q1, and the flags in use MUST be printed by the SFM-TEL-003 command.
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
  MUST NOT hold the placed references of cells other than those loaded and
  the persistent references, beyond a locator that can load a cell's
  references on demand (decision Q3).
- **SFM-IDX-003.** The record parse SHOULD NOT hold a plugin's bytes whole;
  its peak RSS minus RSS after the parse MUST be published per master in
  [Memory Budget § ESM Index](../engine/memory-budget.md#esm-index-cpu-side).
  Today the whole file is read into a buffer (§3.1 row 2).
- **SFM-IDX-004.** A lookup that needs a reference outside the loaded cells
  (a quest alias, a package target, a linked route, a door's destination)
  MUST resolve it through the SFM-IDX-002 locator. A test (new,
  `unloaded_cell_reference_resolves_through_the_locator`) resolves one of
  each kind with the referenced cell unloaded.
- **SFM-IDX-005.** The cell-finish tables (§3.1 row 7) MUST be measured in
  Phase 0; any table over 50 MB on `Starfield.esm` MUST be built per loaded
  cell or on demand.

### 5.4 Textures (`SFM-TEX`)

- **SFM-TEX-001.** The streaming switch's default for BA2 games MUST follow
  decision Q4. Today it is off for every game (§3.1 row 5).
- **SFM-TEX-002.** A material MUST NOT resolve or upload a texture role no
  shader reads under its flags. A test (new, `dead_roles_are_not_resolved`)
  covers each §3.1 row 6 role. When a shader starts reading a role, the
  same change turns its upload on, and the test fails if one side moves
  without the other.
- **SFM-TEX-003.** Starfield emissive maps MUST follow decision Q5: not
  uploaded while the emissive colour stays parked, or uploaded with the
  colour wired.
- **SFM-TEX-004.** Phase 0 MUST publish, for `CityAkilaCityCoeEstate` and
  `citycydoniamainlevel`, the loaded textures' bytes by DXGI format, by role
  and by streamability (floor-readable, single-mip **whole**, chain within
  the floor), so SFM-NG-002's owner can weigh re-encoding against measured
  bytes.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | **Phase 0: verify** — the original collides against the Havok bodies authored in each model's tagfile; settled by a Havok tagfile or Creation Kit collision-authoring reference | Architecture collides against its render triangles, now shared per shape key (SFM-COL-001) | The per-placement copies | A shared shape mutated per placement would move every sharer; shapes are immutable after build → SFM-COL-002, SFM-COL-005 |
| 2 | **Phase 0: verify** — whether the original keeps every cell's references of a plugin resident in an interior session; settled by Creation Kit or UESP documentation of cell loading | Other cells' references load on demand through a locator (SFM-IDX-002) | Holding the world for one interior | A quest, package, scene or door that reads an unloaded reference resolves nothing → SFM-IDX-004 |
| 3 | **Phase 0: verify** — whether the original streams texture levels; settled by a Creation Engine 2 rendering reference or its INI texture settings documented on UESP | Floors first, finer levels when wanted (Texture Streaming), on by default per Q4 | Full chains for surfaces seen small | Visible sharpening during about the first second after a load (FO4 settle measured 0.99–1.05 s, [Texture Streaming PRD](texture-streaming.md)) → TXS-WANT requirements, SFM-G-005 |
| 4 | — | Dead roles are not uploaded (SFM-TEX-002) | Uploads no shader reads | A later shader change that reads a role but forgets the upload samples the fallback → SFM-TEX-002's paired test |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | no | Collision is authored `bhk` and decoded; its index fits ([Memory Budget](../engine/memory-budget.md#esm-index-cpu-side)); BSA textures are Texture Streaming Phase 4 |
| Fallout 3 | no | As Oblivion |
| Fallout: New Vegas | no | As Oblivion |
| Skyrim (LE / SE / AE) | no | As Oblivion |
| Fallout 4 | yes, wave 2 | Shares the fallback collider path for packed-Havok architecture (PHYSAL), the BA2 texture switch (Q4) and the lighting / wrinkle / lookup dead roles; acceptance `MedTekResearch01`. The shared mechanisms carry no per-game branch |
| Fallout 76 | no | No cell loader ([Game Compatibility](../engine/game-compatibility.md)), so no load to budget |
| Starfield | yes, wave 1 | The owner's target; Tier 2 walkable interiors |

### 7.2 Acceptance

Every scenario runs the release build with `--bench-frames 300
--bench-hold`, the streaming switch isolated through
`BYROREDUX_SETTINGS_PATH`, `BYRO_DEBUG_SERVER=1` for `byro-dbg`, and
`/usr/bin/time -v` for peak RSS. It reads the `mem:`, `tex-stream:` and
`bench:` lines. Phase 0 checks the driver in as
`docs/smoke-tests/sf-memory.sh` and indexes it in the smoke-test README.
Starfield runs use `Starfield.esm` alone with the vanilla meshes, materials
and texture archives; no game data is checked in.

| # | Scenario | Data | Passes when |
|---|---|---|---|
| A1 | Cydonia memory | Starfield `citycydoniamainlevel` | the run reaches `bench-hold`; peak RSS and process VRAM at or under the Q6 budgets (SFM-G-001, SFM-G-002) |
| A2 | Shared fallback collision | Starfield `CityAkilaCityCoeEstate` | collider bytes ≤ 1.2 × distinct-key bytes; collider count and `spawn-probe:` equal the HEAD control (SFM-G-003, SFM-COL-005) |
| A3 | Interior session index | Starfield `aaaMarkers` | RSS after load drops by at least the Phase 0 share of other cells' references (SFM-G-004) |
| A4 | Unloaded references | `cargo test` `unloaded_cell_reference_resolves_through_the_locator` | passes (SFM-IDX-004) |
| A5 | Dead roles | `cargo test` `dead_roles_are_not_resolved` | passes (SFM-TEX-002) |
| A6 | Image A/B | Starfield `CityAkilaCityCoeEstate`, FO4 `MedTekResearch01`, pinned pose, `screenshot` on the change and on the HEAD control | pixel delta within the control's run-to-run noise (SFM-G-005) |
| A7 | FO4 parity | FO4 `MedTekResearch01` | collider count and `spawn-probe:` equal the HEAD control; the `mem:` line's collider bytes fall (SFM-COL-005) |
| A8 | Unload returns memory | `docs/smoke-tests/m-exteriors.sh fo4 soak`, with the collider-bytes owner Phase 2 adds | collider bytes return to baseline each cycle (SFM-COL-003) |

---

## 8. Metrics

The numbers go in a new Starfield memory row set in ROADMAP's
bench-of-record section, measured per
[ROADMAP § How to run a bench](../../ROADMAP.md#how-to-run-a-bench), with
their repro command in its repro table. The ceilings stay in
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
| Texture bytes by class | loaded texture bytes by DXGI format, role and streamability | per scene |

---

## 9. Delivery sequence

M39 Texture Streaming Phase 2 must exit before Q4 can flip the switch's
default; nothing else here waits on another milestone. Phases run in the
order that unblocks SFM-G-001 soonest: collision first (Cydonia's largest
measured cost), then textures, then the record index (the largest change).

**Phase 0 — Attribute and decide (no engine code).**
- Check in `docs/smoke-tests/sf-memory.sh`: the RSS and VRAM trace, peak RSS
  and the bench lines, for every §7.2 scene.
- Re-measure §3.2 with it, on a host without the tmpfs load.
- Publish SFM-IDX-001 (`esm_dim8_bench` extended) and SFM-TEX-004 (an
  example over `tex.loaded` and the BA2 DX10 records).
- Split row 4's 7.4× between the engine copy and the physics build.
- Measure the cell-finish tables (SFM-IDX-005) and the Starfield material
  index on its own.
- Close every **Phase 0: verify** item; the owner answers §10.

Exit: the scripts and both tables are checked in, §3.2 is replaced by the
re-measured table with its command, and every §10 item is **Decided**.

**Phase 1 — Memory report.** SFM-TEL-001…004. Exit: the `mem:` line on
every §7.2 scene, with the unattributed share at or under 15 % on
`CityAkilaCityCoeEstate` and `MedTekResearch01`.

**Phase 2 — Shared fallback collision.** SFM-COL-001…005. Exit: A2, A7 and
A8 pass, and A1 is run and its peak RSS published whether or not it passes.

**Phase 3 — Textures.** SFM-TEX-001…003. Exit: A5 and A6 pass, and the
process VRAM of `CityAkilaCityCoeEstate` is published against the HEAD
control.

**Phase 4 — Record index.** SFM-IDX-002…005. Exit: A1, A3 and A4 pass, and
the Starfield memory rows are published in ROADMAP.

---

## 10. Decisions required

1. **Q1 — How cheap may the fallback collider be?** Recommended: share one
   shape per shape key and keep today's build flags. Collision stays
   identical, and Coe Estate's copied triangles halve (§3.2).
   Alternative (a): also drop `FIX_INTERNAL_EDGES` for fallback colliders.
   This saves the topology and pseudo-normals, but bodies can catch on
   internal triangle edges, which the flag exists to smooth.
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
   for the loaded cells plus persistent references, with other cells loaded
   on demand from an offset locator into the plugin. This is the largest
   attributable RAM saving.
   Alternative (a): keep the index whole and stop holding the plugin bytes
   during the parse (SFM-IDX-003 only). This is simpler, but only the parse
   peak drops.
   Alternative (b): keep today's index and rely on Phases 2–3. This is the
   smallest change, but SFM-G-004 is withdrawn.
4. **Q4 — When the streaming switch defaults on for BA2 games.**
   Recommended: on for Fallout 4 and Starfield once Texture Streaming
   Phase 2 exits (A6 and A7 pass on both). FO4 already shows 136 vs 766 MiB
   and Starfield 99 vs 863 MiB resident, with zero deficits and churn. This
   amends the [Texture Streaming PRD](texture-streaming.md)'s Phase 4 exit,
   which flips the default for every game at once, so that PRD takes a
   revision with it.
   Alternative: keep that plan. BA2 games wait for the BSA wave, and
   Cydonia's VRAM goal waits with it.
5. **Q5 — Starfield emissive maps.** Recommended: stop uploading them while
   the emissive colour stays parked (#5283 / #5503), and upload them in the
   same change that wires it. Alternative: wire the emissive colour now.
   Glowing Starfield surfaces then light up, but that is a look change
   with its own calibration problem (the parked luminance is
   100×–10,000× the other games' range, `merge.rs:404-428`), outside a
   memory PRD.
   The FO4 lighting and wrinkle maps follow the same rule. Their upload
   cost is a decision recorded at `crates/renderer/src/vulkan/material.rs:316-329`,
   which this answer would revise.
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
| The fallback collision policy and the tagfile scope | [PHYSAL](../engine/physal.md) |
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
