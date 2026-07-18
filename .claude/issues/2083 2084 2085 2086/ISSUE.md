# Issue batch: 2083, 2084, 2085, 2086

## #2083 — FNV-D7-01: activate_ragdoll has no re-activation guard — second trigger leaks the first ragdoll's Rapier bodies
- Severity: MEDIUM (bug, memory)
- Location: `byroredux/src/ragdoll.rs:204-318`; trigger `byroredux/src/commands/scene.rs:690-703`; cleanup `byroredux/src/cell_loader/unload.rs:418`
- `activate_ragdoll` never checks for existing `RagdollActive`/`Ragdoll` component before building a fresh Rapier body/joint set and overwriting the component, orphaning the first set (leak, fights the live solver).
- Suggested fix: precheck existing ragdoll state, call `remove_ragdoll` on the existing set before rebuilding (or early-return). Add regression test: double-activate, assert body/joint count doesn't grow.

## #2084 — FNV-D8-01: Bench-of-record (613 commits stale) significantly undersells current interior performance
- Severity: MEDIUM (documentation, performance)
- Location: `ROADMAP.md:16-31` (bench table), `ROADMAP.md:694` (`R6a-stale-15`)
- Live re-run of Prospector Saloon repro vs stale baseline (`1c26bc25`) shows big uncontrolled-sample improvement (wall FPS 76.2→149.1, fence 11.12ms→4.87ms). ROADMAP frames `R6a-stale-15` as an open, uninvestigated gap; single sample suggests most of the recovery already happened.
- Suggested fix: run formal 3-scene multi-sample re-bench, update ROADMAP.md, re-scope/close the "fence recovery uninvestigated" framing if confirmed.

## #2085 — FNV-D8-03: CLAUDE.md references asset_provider.rs as a single file; it's now a directory
- Severity: LOW (documentation)
- Location: `CLAUDE.md` workspace-structure tree ("Asset Provider" row), `CLAUDE.md:282`
- `byroredux/src/asset_provider.rs` was split into `byroredux/src/asset_provider/{archive,material,mod,script,texture,tests}.rs`; CLAUDE.md still documents it as a single file.
- Suggested fix: update workspace-structure row + inline reference to point at the directory/submodules.

## #2086 — FO3-D4-01: PlacementLodProvider distant-object LOD never fires on FO3 or FNV — vanilla archives ship zero distantlod .lod files
- Severity: MEDIUM (bug, legacy-compat)
- Location: `byroredux/src/cell_loader/placement_lod.rs:283-353` (`stream_placement_lod_blocks`, gated `GameKind::Oblivion | GameKind::Fallout3NV`); `docs/engine/exal.md:240,576-613`
- The `DistantLOD\<World>_<x>_<y>.lod → _far.nif` scheme was validated only against Oblivion. Direct probes of every FO3/FNV archive show zero `distantlod\` matches — the gate incorrectly includes `Fallout3NV`, so `spawn_placement_lod_cell` silently no-ops forever on those games.
- Related: #1726 (closed, correct for Oblivion, incomplete for FO3/FNV), #1731.
- Suggested fix: narrow the gate to Oblivion-only, update `exal.md` §5 table, note the FO3/FNV gap explicitly on closed #1726.
