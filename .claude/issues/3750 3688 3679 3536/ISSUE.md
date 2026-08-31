# Batch fix: #3750, #3688, #3679, #3536

All four are LOW-severity audit findings (2026-08-27/30 sweeps), independent
of one another, each scoped to a single call site.

## #3750 — SPT-2026-08-30-D3-02: .spt import cache keyed by model path only
**Domain**: binary (`byroredux`) — cell_loader / SpeedTree import
**Location**: `byroredux/src/cell_loader/references/synth_child.rs` (cache_key
= `canonical_model_path_key(&stat.model_path)`),
`byroredux/src/cell_loader/nif_import_registry.rs` (`canonical_model_path_key`)
**Bug**: `parse_and_import_spt` bakes per-TREE-record metadata (ICON, OBND,
MODB, BNAM) into `CachedNifImport`, but the cache is keyed by model path
alone. TREE records sharing one `.spt` file all inherit the first-imported
record's texture/size. 3 collisions in vanilla Oblivion (all into
`TestToddTree*` dev stubs, so vanilla yield ~nil), but breaks retexture mods
that rely on ICON-overrides-tag-4003 (multiple TREE records → one `.spt`).
**Fix**: extend the cache key for the `.spt` branch only (leave NIF key
alone) — include the TREE form id or a hash of the four consumed fields.

## #3688 — PERF-D7-2026-08-30-03: LOD-coverage/terrain-seam diagnostics recompute every reconcile frame
**Domain**: binary (`byroredux`) — streaming
**Location**: `byroredux/src/streaming_helpers.rs:128-130` (unconditional
tail of `reconcile_lod_rings`), `update_lod_coverage` (~152-203),
`update_terrain_seam_stats` (~228-303), `byroredux/src/cell_loader/lod_coverage.rs`
(`find_overlaps` etc.)
**Bug**: Both diagnostics recompute from scratch every frame while
`lod_reconcile_pending` is set, including two O(n²) scans, even on frames
where nothing in `state.lod_blocks`/`state.object_lod_blocks`/`state.loaded`
changed. Only consumers are two byro-dbg console commands.
**Fix**: gate both on a residency-change epoch bumped at the
stream_*_lod_blocks / state.loaded insert-remove sites; skip recompute when
epoch unchanged since last sample. Seam stats can be gated purely off
`state.loaded`'s key set (landscape data is immutable per-worldspace).

## #3679 — PERF-D1-2026-08-30-03: apply_cell_region_ambient re-resolves REGN ambient every exterior frame
**Domain**: binary (`byroredux`) + plugin (`byroredux-plugin`)
**Location**: `byroredux/src/app_step.rs:87` (unguarded call, outside
`grid_changed`) → `byroredux/src/scene/world_setup.rs:509-523` →
`byroredux/src/components.rs:552-575` (`RegionAmbientRes::resolve`) →
`crates/plugin/src/esm/records/misc/world.rs:792-804`
(`select_active_region_sound`)
**Bug**: `select_active_region_sound` allocates a fresh `Vec` and sorts it
every frame, contradicting both the cost comment at the call site (which
only actually describes the climate sibling) and `RegionAmbientRes`'s own doc
claiming "not recomputed per-frame."
**Fix**: cache resolved `RegionAmbientRes` against `(worldspace_key,
player_grid)`, same shape as `applied_climate`; recompute only when that pair
changes. Correct the misleading doc/comment either way.

## #3536 — LC-2026-08-27-D5-02: assemble_exterior_streaming undocumented game==Skyrim branch
**Domain**: binary (`byroredux`) — EXAL boundary / tech-debt
**Location**: `byroredux/src/scene/world_setup.rs:933-941`
**Bug**: `assemble_exterior_streaming` (shared entry point, 4 callers) ends
with an uncommented `if game == GameKind::Skyrim` branch hardcoding two MQ101
form IDs to call `materialize_scene_actor_alias_stubs`. No in-code pointer to
`docs/engine/m47-2-design.md`, no explanation of intended scope.
**Fix** (doc-only per issue's own suggestion, minimal-risk option): add a
three-line comment naming MQ101, pointing at `m47-2-design.md`, stating the
intended scope, so the branch reads as deliberate not a leak.

## Plan
Independent single-site fixes touching ≤5 files total. No specialist agents
needed — tracing directly.
