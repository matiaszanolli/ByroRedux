# Issues 2193 + 2206

## #2193: OBL-2026-07-25-01 -- is_grounded stays false at Oblivion interior spawn (ICMarketDistrictTheGildedCarafe) -- jump unconditionally broken
- Severity: HIGH
- Labels: bug, nif-parser, high, legacy-compat
- State: OPEN
- Location: crates/nif/src/import/collision/shape.rs:354 (resolve_tri_strips_data_refs, suspected root); consumed at byroredux/src/systems/character.rs:195,219,335 (is_grounded gates jump_fired and desired_vertical); crates/physics/src/components.rs:107
- Follow-up to #2013 (CLOSED, e2f75456 fixed spawn-positioning "infinite freefall" via cast_capsule_down). Its closing comment found: Oblivion spawn now stable, but resting contact's surface normal points DOWN (dot-up ~= -0.99) instead of up -- collision triangle at that spot appears inverted, independent of spawn accuracy.
- Impact: is_grounded stuck false -> jump_fired can never be true, vertical-velocity resolution always takes non-grounded full-gravity-integration branch despite resting on solid ground.
- Suspected root: NiTriStrips-based Oblivion collision import path (resolve_tri_strips_data_refs / merge_tri_strips_shape, shape.rs:340-). Shared FO3/FNV path (also via extract_from_classic) grounds correctly on equivalent cells -- narrows defect to Oblivion-specific geometry/import handling.
- Suggested fix: isolate with live Vulkan + real Oblivion data (ICMarketDistrictTheGildedCarafe), confirm which mesh/triangle contributes resting contact, check winding order against shared FO3/FNV path to find Oblivion-specific divergence, fix winding/normal derivation without regressing correctly-oriented floors elsewhere.
- SIBLING check: compare winding-order handling in shared FO3/FNV NiTriStrips collision path vs Oblivion-specific path.
- TESTS: regression test pinning correct collision-triangle winding/normal for the NiTriStrips-derived shape (unit test on parsed mesh data).

## #2206: NIFAL-D4-02 -- billboard_mode silently dropped on the cell-loader path (all cell-loaded games)
- Severity: MEDIUM
- Labels: bug, nif-parser, import-pipeline, medium
- State: OPEN
- Dimension: 4 (Nodes), tier violated: parked-not-leak
- Games affected: ALL cell-loaded games (Oblivion, FO3, FNV, Skyrim SE, FO4, Starfield)
- Location: byroredux/src/cell_loader/references/import.rs:196-200 -- placement_root_billboard: None hardcoded. walk_node_flat never produces an ImportedNode so billboard mode never leaves the parser on this path.
- Loose-NIF path works (byroredux/src/scene/nif_loader.rs:454-456); spawn.rs:416-418 only reached for .spt placeholders. billboard_system IS live in the scheduler (boot.rs:877) running each frame over an empty NIF-sourced set.
- Measured prevalence (live archives, clean parse): 550 FNV / 1527 Skyrim Meshes0 / 517 FO4 / 213 Oblivion / 5 Starfield NiBillboardNode instances.
- docs/engine/nifal.md:102-104 asserts the opposite ("consumed at the spawn sites") -- doc rot that let this hide through 4 prior audit sweeps. Related NIFAL-D4-03 (the doc-rot finding itself, not separately filed apparently).
- Related: #994 (CLOSED, .spt half only).
- Suggested fix: propagate nearest-ancestor billboard mode onto ImportedMesh in walk_node_flat, attach per-mesh at byroredux/src/cell_loader/spawn.rs:1134, reusing the #1235 flags-parity mechanism. Correct docs/engine/nifal.md:102-104 in the same change.
