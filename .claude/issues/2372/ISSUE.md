# #2372 — EX-16: Integrate REGN, NAVM, ambient audio, and AI with exterior streaming

**This is a multi-month milestone/plan issue, not a scoped bug.** Filed as
"Plan: EX-16" with 6 broad acceptance criteria, `enhancement`-labeled, no
`bug` label.

## Acceptance criteria — verified status against current code (2026-08-31)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | REGN drives ambient sound, fog/weather, ground cover, encounter metadata (deterministic priority) | **PARTIAL** | Only the `Sound`-kind REGN entry (music/incidental) is consumed (`components.rs::RegionAmbientRes::resolve`, `misc/world.rs::select_active_region_sound`) with deterministic highest-priority selection. `RegionDataKind::{Weather,Grass,Landscape,Objects}` are parsed but consumed nowhere. Fog/weather comes from CLMT/WTHR (`systems/weather.rs`), not REGN. Ground cover is LTEX-based, unrelated to REGN. No encounter metadata handling anywhere. |
| 2 | NAVM tiles load/unload with cells, preserve cross-cell path connectivity | **PARTIAL** | Load/unload works (`cell_loader/load.rs`, `exterior.rs`, `unload.rs::bump_navmesh_residency`). Cross-cell connectivity is explicitly **not implemented** — `systems/navmesh_path.rs` docs it as "genuinely blocked": `NavmExternalConnection` has no confirmed source-triangle field, so pathing is single-tile only. |
| 3 | Actors/packages suspend, migrate, resume across stream boundaries, no duplication/dangling refs | **NOT DONE** | `cell_loader/unload.rs::unload_cell_inner` despawns every cell-owned entity wholesale (narrow cinematic-retention exception only). `persistent_ref_index.rs` states plainly it is "the foundational identity mechanism EX-16's actor/package migration needs before it can be built" and nothing in production calls it yet. No suspend/migrate/reparent code exists anywhere. |
| 4 | Ambient audio emitters/regions crossfade and reclaim ownership on unload | **PARTIAL/NOT DONE** | Only one global music channel crossfades (`asset_provider/audio.rs::dispatch_region_ambient_music`). No per-emitter/per-region ownership tracking or reclaim system. `components.rs` flags "EX-16 item 4's snapshot/restore" as a pending, unbuilt consumer. |
| 5 | Debug telemetry reports active REGN/NAVM/audio/AI owners per cell | **NOT DONE** | No console command reports this (`world.owners` is unrelated GPU/asset ownership tooling for a different issue). |
| 6 | Boundary/soak tests: unload/reload while an actor path crosses a cell edge | **NOT DONE** | No such test exists. Has no substrate yet — depends on criteria 2 and 3, both unimplemented. |

**0 of 6 fully done.** Substantial adjacent work has landed over many
sessions under separate issue numbers (NAVM streaming residency, PACK
procedure runtimes M42.1–M42.9, REGN ambient music dispatch, terrain-seam
validation, persistent-ref identity index) — but none of it closes any of
this issue's 6 criteria outright, and two of the three undone criteria
depend on subsystems (cross-tile NAVM pathing, actor/package
suspend-migrate-resume) that don't exist and aren't small additions.

## Why this doesn't fit the fix-issue pipeline
- Not a bug — no reproducible defect, no `bug` label.
- Not scoped — 6 independent subsystems, several genuinely blocked or
  requiring new architecture (a full actor/package migration system, a
  cross-tile NAVM search once the blocking data question is resolved).
- Far past the >5-file scope-check threshold for a single fix commit.

## Decision needed
Asked the user how to proceed (skip / narrow slice / attempt in full).
