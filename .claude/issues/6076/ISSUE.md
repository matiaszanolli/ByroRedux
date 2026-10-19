# FNV-SD-02: The worldspace persistent CELL is spawned once for the entry window; persistent refs beyond it never materialise as the player moves

Labels: high,terrain-exterior,gameplay,bug,game:fnv,legacy-compat

**Severity**: HIGH (fails under realistic conditions - continuous travel; the door-round-trip workaround is fragile) · **Dimension**: 1 (cell load and streaming) · **Game**: FNV (FO3 twin FO3-SD-02)

## Description
A worldspace's persistent CELL holds every reference flagged Persistent, wherever it stands. `begin_worldspace_persistent_cell(wctx, center_grid, full_3d_radius, ...)` partitions them **once, at worldspace entry**: refs within `radius_load` of the arrival cell are spawned by the persistent job; actors beyond it become `SceneAliasCandidate` stubs; **non-actor refs beyond it are dropped**. Nothing re-runs the partition when streaming moves the centre, and nothing promotes a stub to a full actor when its position enters the loaded radius. The persistent root is rebuilt only by a worldspace-level drain (door transition, `drain_streaming_state`), centred on the new arrival. The design doc describes this population as "resident ... never evicted by radius streaming" (`stream-boundary-state-continuity.md` section 1), which holds for the 3D that was spawned, not for the population.

## Evidence
Location: `byroredux/src/cell_loader/exterior.rs:1143-1151` (partition loop), `:1180-1189` (`persistent_position_is_local`, Chebyshev against `center_grid`), `byroredux/src/scene/world_setup.rs:1083-1092` (only caller, guarded by `state.persistent_root.is_none() && state.persistent_apply.is_none()`), `exterior.rs:370-406` (remote actors become identity-only stubs). No other site consumes `worldspace_persistent_cells` for spawning. Re-confirmed present at HEAD.

`refs_near` with an unbounded radius, centre = the P0 arrival cell (-17, 0), radius 12:
- WastelandNV persistent cell: **6,069 refs**; 884 within radius 12 (157 within radius 3); **5,185 beyond radius 12**.
- Of the 6,069: **373 teleport doors** (335 beyond radius 12), 913 refs whose base carries a script (808 beyond), 312 item/container/activator/terminal refs, 530 creature refs (274 modelled, 256 model-less/marker). Beyond the window (known static base): 1,284 `MarkerXHeading`, 527 `marker_creature`, 233 `architecture\strip` pieces, 90 `HELIOS_One` pieces.
- Examples beyond the window: `HeliosOneSolarReflectorRow`, `BMRadioControlPanel` x4, `NVBlackMtnSatelliteDishNOPHYSICS`, `IndDoorSmAnim01`, `BlackCaveDoor`, `UtlDoor01`, `HVPaladinDead06`, `MoeTheCentaur`.
- Other FNV worldspaces are small enough that radius 12 covers them (`thestripworldnew` 477 refs, `freesideworld` 172).
- Disproof attempted: the exterior tile loader reads only the tile's own `cell.references` (persistent CELL is kept separate); no later system promotes stubs (`materialize_scene_actor_alias_stubs` only makes identity stubs). The claim that the original engine loads a persistent ref's 3D when its cell is in the loaded grid is flagged by the audit as needing a ruling (no local source states the 3D-loading rule). Not verified live.

## Impact
After a continuous walk from Goodsprings, far persistent doors cannot be activated, quest props and scripted activators are absent (their object scripts are also not attached, since attach happens at spawn), persistent creatures and the Helios/Black Mountain set pieces are missing. With FNV-SD-01 and SCR-SD-03 (persistent ObScript refs on stubs run `GameMode` regardless) the persistent population is over-scripted where stubbed and absent where not.

## Related
SCR-SD-03, #5997, #4112, #5483, #3376, `stream-boundary-state-continuity.md`. FO3 twin FO3-SD-02: Wasteland persistent CELL 4,853 refs / 240 teleport doors, 3,813 (79%) beyond radius 12 from the Megaton arrival, 4,110 (85%) from Vault 101.

## Suggested Fix
Keep the partition list on the job/state and run it as part of the per-step streaming reconcile: spawn the persistent refs (and promote stubbed actors) whose position enters `radius_load`, release them past `radius_unload`, owned by the persistent root or by the tile `CellRoot`. Pin it with a test that moves the centre.

Source: docs/audits/AUDIT_FNV_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)
