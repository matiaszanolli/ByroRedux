**HEAD**: `6f44874ac` · **Baseline**: `docs/audits/AUDIT_PHYSICS_2026-10-16.md` (HEAD `d099eb68f`) · **Audited**: Dim 5 (WATAL sink, in full), the exterior slices of Dim 1 (terrain collider translation), Dim 2 (terrain collider stream-in/out, fixed-step/wake/query-leaf contract), Dim 4 (player capsule waterline / swim / dive; character teleport guard) and Dim 6 (footstep surface probe on terrain/water), scope=exterior-area · **Unchanged since baseline (skimmed)**: Dim 5 sampler/latch code (one commit, `b6577da90`), Dim 2 `step`/fast path, Dim 6 queries

# PHYSAL / Physics Audit — 2026-10-18 — AREA-SCOPED (exterior water + terrain colliders)

**This is an AREA-SCOPED audit run for the `--area exterior` suite, not a full `/audit-physics`.** In scope: the WATAL buoyancy sink, exterior water planes/volumes (`cell_loader/water.rs`, `systems/water.rs`, `crates/core/src/ecs/components/water*`), the player capsule's waterline/swim/dive transitions, the footstep surface probe over terrain and water, and LAND terrain colliders (stream-in/out, fixed-step determinism around them). Everything else is "out of area" below. Read-only except this report; no engine launch; no sub-agents.

**Dedup**: `docs/audits/AUDIT_EXTERIOR_2026-10-18.md` read in full (none of its five findings is physics). Coordinator note: renderer REN-EXT-L-02 (grass blades drawn after water) is a render-order observation, not re-reported. Open-issue snapshot `/tmp/audit/issues.json` plus `gh issue list --state all` searches for LTEX/HNAM/terrain friction/terrain surface material: no hits.

## Executive Summary

**2 findings, both NEW and LOW. 0 CRITICAL / 0 HIGH / 0 MEDIUM / 2 LOW.**

- The physics-scope delta since the baseline is small (`d099eb68f..HEAD`: `crates/physics` +~330 lines, `systems/character.rs` +27, `systems/audio.rs` footstep/impact gating). In-area, only one commit touches the water sink: `b6577da90` (#5934), which fixes baseline PHYS-D5-02 and is non-vacuously pinned.
- **Baseline PHYS-D5-01 (#5933, marker-exit stale force) is still present at HEAD and still open.** The reset gate at `crates/physics/src/water.rs:1070` is unchanged (`current_flow.is_some() || surface.is_some() || t.prior_wet`), and nothing in the delta touches it. Not re-reported; it is the one in-area defect that matters for exterior rivers (the #5129 marker-wider-than-plane configuration).
- The two new findings are both terrain-collider fidelity items. Neither affects solver correctness: (1) terrain colliders ignore the authored LTEX Havok data, (2) the terrain tile collider's winding is load-bearing under `ORIENTED` but unpinned, with an inverted explanatory comment.
- **PHYSAL verdict (area slice)**: no `GameKind`/`bsver` branch exists on the solver side of the water or terrain paths. The terrain path is deliberately game-agnostic (one `spawn_trimesh_collider_ghost` for LAND and architecture).
- **Games traced**: none with real data (no game data loaded; code and unit tests only). The FNV Lake Mead W1 smoke (`docs/smoke-tests/w1-water-traversal.sh`) needs a Vulkan device and was not run.

**Lane counts at HEAD** (default toolchain, `TMPDIR=/mnt/data/tmp`, repo `target/`):

| Lane | Result |
|---|---|
| `cargo test -p byroredux-physics` | 258 passed (baseline 253) |
| `byroredux --bin byroredux -- water` | 166 passed / 4 ignored |
| `-- character` | 64 passed (baseline 63; +`place_character_body_rejects_non_finite_positions_before_writing`) |
| `-- footstep` | 20 passed (baseline 19) |
| `-- rapier_release` | 9 passed |
| `-- synthesize` (collider ghost) | 17 passed |
| `-- scheduler_access` | 29 passed |
| `-- submersion` | 5 passed |
| `-- terrain` | 82 passed / 3 ignored (game data) |

Not run: `--ignored` real-data tests (FO3 ragdoll gate is Dim 3, out of area), the skeever probe, any windowed or Vulkan test.

## Dimension status

| Dim | Status |
|---|---|
| 1 Shape Translation | Area slice only (terrain TriMesh collider): see PHYS-EXT-D1-01 / -02. Rest out of area. |
| 2 Step & Sync | Area slice only (terrain stream-in/out, query leaves, fast path): VERIFIED, below. Rest out of area. |
| 3 Ragdoll & Constraint Seam | out of area |
| 4 Character & NPC Controller | Area slice only (player waterline/swim/dive, teleport guard): VERIFIED. NPC KCC out of area. |
| 5 WATAL Sink | In area, audited in full against the delta. |
| 6 Queries, Diagnostics & Cost | Area slice only (footstep probe on terrain/water): see PHYS-EXT-D1-01 consequence. Rest out of area. |

## Solver Invariant Matrix (area slice)

| Invariant | Verdict | Notes |
|---|---|---|
| Fixed step / accumulator | VERIFIED | `step` (`world/mod.rs:1298-1305`): NaN-safe `frame_dt.max(0.0)`, clamp before loop. Unchanged. |
| Wake discipline vs terrain stream-in | VERIFIED | `register_newcomers` does not `wake()`. A streaming frame with no motion takes the fast path, which calls `insert_pending_query_leaves()` before zeroing the accumulator (`mod.rs:1369-1372`), so a new LAND collider is visible to the KCC and rays the same frame the sync registered it. Static terrain bodies never enter the dynamic index, so a tile load never raises `has_awake_dynamics`. |
| Terrain collider lifetime | VERIFIED | The ghost is spawned inside the cell's entity range (`exterior.rs` `begin`: `first_entity` ... `stamp_cell_root_range`), so `CellRoot` covers it; `release_entities` removes `RapierHandles` bodies (`unload.rs:949-976`). `rapier_release_tests` green (9). |
| Terrain/render geometry parity | VERIFIED | `spawn_trimesh_collider_ghost` is fed the exact `vertices`/`indices` uploaded to the GPU (`terrain.rs:1304-1317`), after `sanitize_land_height` (non-finite -> 0), identity placement, scale 1, so the collider cannot drift from the drawn surface (including the 128 BU quad diagonal). |
| Lock order / phase order | VERIFIED | No change; the streaming apply runs outside the physics stage, the sync then registers. |
| Player teleport | VERIFIED | `place_character_body` now refuses non-finite/insane positions before any write (`character.rs:927-930`, fix for baseline PHYS-D4-02 half); pinned by the new test. |
| Water two-samplers rule | VERIFIED | No water input added to either side. `player_water_state` re-read: `surface_y_at` (XZ AABB reject, then flat `max.y` or mesh triangles), `weather_wave_adjustment` sampled once before storage guards, kind-damped scroll, plane flow only (marker composed in `player_current_drift`). |
| Water latches / `water_force_targets` | VERIFIED | Unchanged since baseline except `b6577da90` below. |
| Exterior water volume footprint | VERIFIED | `terrain_water_components` marks a water cell wet if ANY of its four LAND corners is at or below XCLW (conservative, covers the shoreline crossing), connected components get separate `WaterVolume` entities in the same cell range; the floor is `xclw - 4096` for exterior half-extents. Row order pinned by `land_row_zero_wets_the_same_south_edge_the_terrain_mesh_occupies`. LOD water carries no volume (pinned source-scan test at `water.rs:1667`). |

## Findings

### PHYS-EXT-D1-2026-10-18-01: Terrain colliders ignore the authored LTEX Havok data (material, friction, restitution); every exterior ground tile is `surface_material` 0 / friction 0.5 / restitution 0.3
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

### PHYS-EXT-D1-2026-10-18-02: The terrain tile collider's upward winding is load-bearing under `FIX_INTERNAL_EDGES | ORIENTED`, is pinned by no test, and its explanatory comment states the opposite
- **Severity**: LOW (test gap and doc rot; behaviour is correct by arithmetic)
- **Dimension**: Shape Translation
- **Location**: `byroredux/src/cell_loader/terrain.rs:966-985` (comment `:969-971` and the index loop), `byroredux/src/cell_loader/spawn/synthesize_trimesh_tests.rs` (no orientation test), `crates/physics/src/config.rs:34-49` (`TriMeshFlagBits::DEFAULT = FIX_INTERNAL_EDGES | ORIENTED`)
- **Status**: NEW
- **Description**: The tile is emitted as `(tl, tr, bl), (tr, br, bl)` and the same index buffer is handed to the collider. In the Y-up frame: `tl=(x,h,-y)`, `tr=(x+128,h,-y)`, `bl=(x,h,-y-128)`, so `(tr-tl) x (bl-tl) = (128,0,0) x (0,0,-128) = (0, +16384, 0)`: the triangle normal points up. That is the orientation `ORIENTED` needs for a floor (a mesh with its normals pointing down would treat the walkable side as the interior). The comment says the Z-up to Y-up transform "negates Z, flipping winding" and so the indices are emitted "CW so it becomes CCW". `zup_to_yup_pos` is `(x,y,z) -> (x,z,-y)`, a proper rotation (determinant +1) that does not flip winding, and the emitted order is already CCW seen from above in both frames. The code is right; the reasoning written beside it is wrong, which invites a "fix" that would invert the floor. No test builds a LAND-shaped tile, registers it, and asserts that a downward ray or the KCC grounds on it; the existing ghost tests use one-triangle XZ fixtures and check only AABB/scale/ownership.
- **Evidence**: arithmetic above; `zup_to_yup_pos` at `crates/core/src/math/coord.rs:105`; tests listed in `synthesize_trimesh_tests.rs` (single triangle, floor ghost static/renderer-free, ownership backlink, scale once). The `ORIENTED` semantics were not probed in this pass (read-only, no scratch crate); live walking on exterior terrain (W1 smoke, normal play) is the empirical evidence that the current orientation works.
- **Impact**: None today. A future winding edit (for example to "fix" front-face culling on the render side, since the index buffer is shared) would silently drop the player through every exterior tile with no red test.
- **Trigger Conditions**: a change to the terrain index loop or the shared Y-up conversion.
- **Related**: the `terrain.rs` comment at `:969`, `config.rs` ORIENTED note.
- **Suggested Fix**: correct the comment (rotation, not reflection; the order is CCW from above), and add a test that builds a small LAND-shaped grid through the same index pattern, registers it via `spawn_trimesh_collider_ghost` + `physics_sync_system`, and asserts `cast_ray_down` hits at the surface height and a capsule `move_character` is grounded on it.

## Carry-overs verified at HEAD (not re-reported)

- **#5933 (PHYS-D5-2026-10-16-01) OPEN, present**: `water.rs:1070` gate unchanged; no `in_current_prev` term in the reset condition. Exterior relevance: a dynamic body leaving a marker-only `WaterCurrentVolume` (river marker wider than its plane band) keeps the last marker force. Still the one in-area defect worth fixing first.
- **#5932 (root-node collider drop on behavior-graph props)**: open; not an exterior-water or terrain path. Out of area.
- **#5697, #4772**: ragdoll items, out of area.

## Baseline findings closed by the delta (commit seen; area-relevant ones read)

| Baseline ID | Fix | Verified |
|---|---|---|
| PHYS-D5-2026-10-16-02 (link in marker-only current undamped) | `b6577da90` (#5934): marker arm adds `-c*m*v` force and `-c_ang*m*w` torque for multibody links when no surface arm ran | Diff read. Guard `marker_only_current_keeps_authored_link_damping` asserts `-600 < vy < -100` after 60 steps (non-vacuous against the measured -684 undamped). The sibling test `current_drag = 10.0` adjustment is legitimate (damping now acts against the current). |
| PHYS-D5-2026-10-16-03 (stale comments) | `bfa0007d1` (#5935) | Commit seen only. |
| PHYS-D2-2026-10-16-01 (re-sleep swallows wake) | `00ec063ec` (#5936) | Commit seen only; Dim 2 general, out of area. |
| PHYS-D2-2026-10-16-02 (contact position fallback) | `b3b473487` | Test `contact_event_position_uses_dynamic_body_when_pair_is_gone` read. |
| PHYS-D4-2026-10-16-02 (non-finite teleport) | `267ccee0e` | `place_character_body` guard and test read (above). `ActorWalkCommand` half not read (console, out of area). |
| PHYS-D4-2026-10-16-03 (const indentation), D6-01/-03 | `49a24773b`, `44f3a4f35`, `b4bff7784` | Not read; out of area. |

## Dimension notes

### Dim 5 (in area): checklist
- **Sink code**: the only change is the marker-arm link damping (above). Lift/heave/latch/target-set logic, wake discipline, both target sources, `WaterContact` transition frame, and the two-sampler rule are byte-identical to the baseline except that hunk. `scheduler_access_tests` water/submersion guards green.
- **Swim/dive transitions (player)**: `terrestrial_carry_velocity` zeroes carried vertical velocity on the swim to wade exit; `depth_reaches_swimlevel` is the single predicate shared by the controller, `sync_player_water_contact` (depth measured at the sampled pose, so next frame's `was_swimming` round-trips) and the new ObScript `actor_is_swimming` (`character.rs:1422-1430`), which reuses it rather than adding a second definition. Good: one predicate, three callers.
- **Exterior seam behaviour**: adjacent cells' water volumes are separate entities with AABBs ending on the cell edge (`x > max` is a reject, inclusive on the edge); `player_water_state` takes the nearest column by `|surface_y - pos.y|`, so crossing a tile seam at a common XCLW picks whichever tile contains XZ with no gap. At a tile with a different XCLW the nearer surface wins. No defect found.
- **Stream-out while swimming**: the player row is owned solely by `sync_player_water_contact`, which writes the dry sentinel once when `player_water_state` returns `None`; a plane despawned by cell unload therefore ends the swim state in the next controller tick (no stale `WaterContact`).
- **Depth floor**: exterior volumes extend `EXTERIOR_CELL_UNITS` (4096 BU) below the surface. A body or the player below that depth reads dry. No such depth exists in the vanilla ocean data checked by the existing `water.rs` doc; unverified for mods, not reported.

### Dim 2 (area slice): terrain stream-in/out and determinism
- The terrain ghost is a `Static` body with a TriMesh converted by `collision_shape_to_parts` on the rayon pool and inserted serially in newcomer order, like any other static (`register_newcomers_parallel_section_holds_no_world_guard`). No terrain-specific path in `crates/physics`.
- One frame of lag between the streaming apply (collider entity exists) and the sync registering it is by design; the spawn ladder and `place_character_body` ground the player, and a player standing on a not-yet-registered tile is the pre-existing streaming-radius question owned by `/audit-gameplay` and the baseline. No new gap.
- `contact_event_position` terrain-origin issue (#5675) is fixed and re-pinned by `contact_event_position_is_the_contact_not_the_body_origins` (terrain tile parked at the origin).

### Dim 6 (area slice): footstep probe
- `footstep_system` passes the walker's own body to `surface_material_under` with `FOOTSTEP_SURFACE_PROBE_REACH` (128 BU, pinned > 1.5x capsule centre height). Over water there is no water collider, so the ray reads the bed (or `None` beyond reach); swimming suppresses the emitter via the `WaterContact` edge (`audio.rs:157-176`). Consistent with the 10-16 verdict. The terrain material content gap is PHYS-EXT-D1-01.

## Out of area (one line each)

- Dim 1 other shape variants, convert/config: out of area.
- Dim 2 explosion recovery, ragdoll wake, contact-force events, kill plane: out of area (unchanged; `step` fast path spot-checked above).
- Dim 3 Ragdoll & constraint seam: out of area.
- Dim 4 NPC KCC / `locomotion.rs`: out of area.
- Dim 6 casts, corridor queries, trap/projectile costs, `phys.stats`: out of area.

## Known-Open Register (what this pass changed)

- **Newly opened here (not yet issues)**: PHYS-EXT-D1-01 (LOW), PHYS-EXT-D1-02 (LOW). Suggest `/audit-publish` with labels `physics` (+ `terrain-exterior` for D1-01); D1-01 also hands the LTEX decode to `/audit-esm`.
- **Closed by the delta**: baseline D5-02 (#5934), D5-03 (#5935), D2-01 (#5936), D2-02, D4-02, D4-03, D6-01, D6-03 (see table).
- **Still open, unchanged**: #5933 (marker-exit stale force, verified present), #5932, #5697, #4772, #5015, mass-inverted-rig root cause, water-walking/freezing unbuilt.
- **Skill drift for the next `/audit-physics` edit**: add the terrain ghost path (`terrain.rs` -> `spawn_trimesh_collider_ghost`, default static body, no LTEX data) to Dim 1; note that `actor_is_swimming` (ObScript) now reuses `depth_reaches_swimlevel` in Dim 5; the lane counts above (physics 258, character 64, footstep 20).
