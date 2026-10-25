# Fix-issue batch 6087-6091 (fetched 2026-10-25)

## #6087 — SAVE-SD-04: The post-load ambient re-seat runs before `ScriptPackage` is overlaid, so a script-package winner is not reproduced
**Severity**: LOW · **Dimension**: Live Load-Apply · **Data-Loss Class**: none (derived procedure state is cleared and re-installed; spurious package fragments)
**Location**: `byroredux/src/save_io/live_load.rs:620` (re-seat) before `:638` (`apply_deltas`); `byroredux/src/npc_spawn/ai_package.rs:892-894`, `:1036-1040` (script package precedence), `:1088-1099` (changed-handover clears behavior)

## Description
#4815's contract is "re-seat the winner against the restored clock and quest state *before* the overlay, so the overlay lands on the saved winner". `ScriptPackage` (M47.3, `register_replacing_component`, overlaid) changes the winner in `ambient_ai_package_system` (`script_package_of`), but it is restored by the overlay after the re-seat. The re-seat therefore stamps the scheduled winner W; the overlay then restores the script package's `TravelState` / `Traveled` markers and the `ScriptPackage` slot on an actor whose `AmbientPackageRuntime` names W. At the next in-game minute the tick selects the script package, sees `changed`, calls `clear_ambient_behavior` (deleting what the overlay restored) and installs the behavior fresh, reporting `W -> script` package transitions (PF_ fragments) for an actor that was already on the script package. The test comment at `round_trip_tests.rs:1715-1719` ("persisting its cached winner is unnecessary") assumes winners depend only on the clock and quest state; the alias-injected overlays are re-derived after the re-seat too.

## Impact
A saved script-package actor loses its Travel progress markers within about a game minute of loading and fires an extra package-change fragment. Cosmetic-to-minor, hence LOW.

## Related
#4815, #5864, #6074 (GAME-SD-04: the alias-overlay half is the load-side face of that finding), #6072 (GAME-SD-02).

## Suggested Fix
Overlay `ScriptPackage` (a plain-data replacing column) before the re-seat, or run the re-seat after `apply_deltas` and re-assert the markers; pin with a round-trip test that includes a running script package.

Source: docs/audits/AUDIT_SAVE_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-placement components absent from the eviction row)
- [ ] **TESTS**: A regression test pins this specific fix


## #6088 — ESM-SD-2026-10-19-03: `resolve_precombine_owner` treats the FormID slot byte as a load-order position
## Description
`resolve_precombine_owner` (`byroredux/src/cell_loader/precombined.rs`, callers in the same file) does `let idx = (form_id >> 24) as usize; load_order_paths.get(idx)`, indexing the load-order-aligned path list by the top byte of a remapped FormID. That byte is a **slot**; `allocate_global_slot` (`load_order.rs`) draws regular and light (ESL) plugins from independent counters, so position equals slot byte only while no ESL precedes a regular plugin. FO4 supports ESL (TES4 flag `0x200`), and ESM-flagged and ESL files load before ESPs. A regular plugin after k ESLs has slot `position - k`, so its cell resolves to a different plugin's path and `<name>.esm` subdirectory; a cell owned by an ESL has top byte `0xFE`, out of range, and falls back to the active plugin at the root directory.

Regression-shaped sibling of #3366 (closed): `plugin_for_form_id` (`load_order.rs`) decodes the slot with `global_slot_of` first; this site was not touched.

## Evidence
The only tests (`precombined.rs` `resolve_precombine_owner_follows_form_id_mod_index` and neighbours) use all-regular orders (Fallout4 + DLCs, slots 0..2). `allocate_global_slot_partitions_medium_light_and_regular` and `esl_plugin_own_forms_land_in_light_space` (`load_order.rs`) show the divergence is real. Vanilla FO4 and its DLCs are all regular masters, so the vanilla gate passes; the defect needs an ESL ahead of a precombine-owning plugin (a CC `.esl` before a settlement `.esp` that bakes new cells). The failure mode is a missed `_oc.nif` lookup, which `PrecombinedSpawnJob` already treats as a miss (refs spawn individually, because `absorbed_ref_bakes` skips only refs whose bake actually spawned, #5484). Not exercised at runtime (no FO4 ESL load order here).

## Impact
Precombined cells owned by mod plugins in an ESL-bearing FO4 load order silently fall back to per-REFR rendering (draw count, BLAS count); the CSG routing (`blobs_for_cell`) can also open the wrong plugin's blob. No vanilla impact.

## Related
#3366, #1590, #2369.

## Suggested Fix
Resolve the owner through `global_slot_of(form_id)` and the slot table (`LoadOrder::slots`) instead of the raw byte; add an ESL-before-regular case beside `resolve_precombine_owner_follows_form_id_mod_index`.

Source: docs/audits/AUDIT_ESM_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers/builders of the same field, other games sharing the code path)
- [ ] **TESTS**: A regression test pins this specific fix


## #6089 — FO3-SD-03: FO3 child worldspaces with "Use Land Data" draw no ground; #5505's landscape inheritance is Oblivion-only
**Severity**: MEDIUM · **Dimension**: 3 (worldspace context, terrain) · **Game**: FO3

## Description
WRLD `PNAM` bit `0x01` ("Use Land Data") makes a child worldspace use its `WNAM` parent's landscape. Four FO3 worldspaces carry it: `Oasis` (`0x0005`), `ParadiseFalls` (`0x003d`), `GNRroofWorld` (`0x0007`), `MegatonWorld` (`0xffc5`), all with parent `Wasteland`. Their cells are authored without LAND. #5505 added the landscape inheritance but gated it to Oblivion, so on FO3 those cells spawn their references with no terrain mesh and no ground collision.

## Evidence
Location: `byroredux/src/cell_loader/exterior.rs:1795-1805` (`if record_index.game == GameKind::Oblivion { inherit_parent_landscapes() }`), `crates/plugin/src/esm/cell/index.rs:358-425` (the function; its doc says "FO3+ child grids are not assumed to share the parent's coordinates"). Re-confirmed present at HEAD.

Raw scan of `Fallout3.esm` (`LAND` attributed to the preceding `CELL`):

| Worldspace | cells | LAND-less | of those with refs | parent LAND at same (x,y) |
|---|---:|---:|---:|---:|
| Oasis | 34 | **34** | 7 | 34 |
| ParadiseFalls | 13 | 11 | 10 | 11 |
| GNRroofWorld | 143 | 58 | 4 | 58 |
| MegatonWorld | 16 | 5 | 1 | 5 |
| total | 206 | 108 | 22 | 108 |

The co-location test #5505's author asked for: static objects of the child cells against the parent's `VHGT` terrain height at the same position. `Oasis`: median |dz| **166** units vs **2,480** for a control shifted by (+7,+5) cells. `ParadiseFalls`: **46** vs **4,126**. So both towns stand on the Wasteland terrain at those coordinates. `GNRroofWorld` (1,547 vs 936) and `MegatonWorld` (431 vs 717) are inconclusive (rooftop and offset geometry), not disproved. Disproof attempted: LOD ring is parent-keyed only for coordinates the child has no CELL for; the cells are not hidden-quad cells (#6043 is about Hide Quad flags); the PNAM bit values have bit 0 set and the parent chain exists. Not verified live.

## Impact
Entering Paradise Falls or the Oasis puts the player in a world whose ground is missing (22 cells with placed content have none).

## Related
#5505 (closed; Oblivion leg, which "measured on Oblivion only"), `EsmCellIndex::inherit_parent_landscapes`, EXT-D6 sibling (terrain LOD hole), #6043.

## Suggested Fix
Run the inheritance for `Fallout3NV` too (the CS rule is the same), keep the cell's own LAND winning, and pin it with an `--ignored` test over `Oasis` / `ParadiseFalls`; confirm the roof and Megaton worlds visually before relying on them.

Source: docs/audits/AUDIT_FO3_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)


## #6090 — FO3-SD-04: The RACE body-section ICON skin is never applied on FO3
**Severity**: MEDIUM · **Dimension**: 5 (NPC body assembly) · **Game**: FO3 (FNV hit the same way, not separately censused)

## Description
The Oblivion arm of #5487 paints the RACE body-section `ICON` over the body NIF's embedded skin. FO3 authors the same data and the parser already collects it (`body_part_textures`), but the index mapping exists only for Oblivion, so every FO3 body piece keeps the skin texture baked into the NIF. The doc comment on `body_part_textures` states the opposite of the data.

## Evidence
Location: `byroredux/src/npc_spawn.rs:689-702` (`humanoid_body_path_icon_index` returns `None` for every game but Oblivion), `byroredux/src/npc_spawn/resumable/runtime.rs:249-254` and `:131-147` (`race_body_texture` only reached through it), `crates/plugin/src/esm/records/actor/race.rs:85-94` (doc: "FO3 / FNV author body textures in the head-section table instead; this list is empty there"). Re-confirmed present at HEAD.

- `Fallout3.esm` RACE records carry `ICON` under `NAM1` (the body section): `Ghoul` -> `Characters\Ghoul\UpperBodyMale.dds`, `Characters\Ghoul\HandMale.dds`; the four `*Raider` races -> `Characters\Raider\UpperBodyMale.dds`, `HandMale.dds` (female: `RaiderFemale`); the four `*Child` races -> `Characters\Child\UpperBodyMale.dds`, `Characters\ChildFemale\...`. All 12 textures exist in `Fallout - Textures.bsa`. FO3 body indices: 0 upper body, 1 left hand, 2 right hand (not Oblivion's 0 / 1 leg / 2 hand / 3 foot).
- The body NIFs embed the default skin: `upperbody.nif` -> `characters\male\UpperBodyMale.dds`; `lefthand.nif` -> `characters\male\HandMale.dds`; **`childupperbody.nif` -> `characters\male\UpperBodyMale.dds` (adult male)**; `femaleupperbody.nif` -> `characters\female\UpperBodyFemale.dds`.
- Placed humanoids whose resolved race has a different body ICON: raiders 417 of 3,252 ACHR, ghouls 122, children 62: **~600 (18%)**. FNV's RACE records author identical ICONs for Raider, Ghoul and Child.
- The head already gets its race ICON (`head_part_texture`, `runtime.rs:294-295`); the ghoul teeth / tongue ICONs are unused too.
- Disproof attempted: armour covers most raiders but exposed hands/neck/head remain and the child, ghoul and underwear cases are naked-body; no second mechanism recolours skin (`body_textures` is only the Oblivion fill and the Body phase consumer). Not verified live.

## Impact
Ghoul NPCs have human bodies under ghoul heads, children wear adult male skin, raiders have default instead of tanned skin.

## Related
#5487 (closed; Oblivion only), #5783, FO3-SD-10 (the `race.rs` doc).

## Suggested Fix
Add a FO3/FNV mapping (0 upper body, 1 left hand, 2 right hand) in `humanoid_body_path_icon_index`; `upperbody.nif` also carries underwear and gore-cap meshes, so apply the texture to the skin mesh only (the Body phase currently sets `base_color` on every mesh of the NIF).

Source: docs/audits/AUDIT_FO3_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)


## #6091 — FO3-SD-10: Stale statements in the FO3 skill and in race.rs
**Severity**: LOW · **Dimension**: skill / doc rot · **Game**: FO3

## Description
Stale statements in the FO3 audit skill and in `race.rs`.

## Evidence
Location: `.claude/commands/audit-fo3/SKILL.md` (Game Context bullet "Known-open, do not re-file ... #4772" at line 29; Dim 5 scripting bullet "there is no general executing runtime for FO3 script logic" at line 104), `crates/plugin/src/esm/records/actor/race.rs:91-93`. Re-confirmed at HEAD.
1. #4772 is closed (`fc9622f6f`, 2026-10-19): `p2-melee-core.sh` now asserts `joints=17 live_joints=17`, `recoveries_total=0` and no explosive detach for FO3 (`P2_RAGDOLL_JOINTS=17` in `fo3.env`); the greps match `ragdoll_status.rs:97` and `commands/physics.rs:235-241`. `gh issue view 4772` reports CLOSED.
2. M47.3 object scripts attach and run on FO3 with no dialect gate (`references/attach.rs:352`, `obscript_objects.rs`); only the compiled-bytecode *compatibility analysis* has no FO3 dialect.
3. `race.rs:91-93` says the FO3 / FNV body textures live in the head-section table; the data has them under `NAM1` (see FO3-SD-04).

## Impact
Future FO3 audits skip a closed issue as "known-open" and under-report the FO3 scripting runtime.

## Related
FO3-SD-04, #4772.

## Suggested Fix
Update the skill's known-open list and Dim 5 scripting bullet, and the `race.rs` doc together with FO3-SD-04.

Source: docs/audits/AUDIT_FO3_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)

