# #5529: PERF-D7-2026-10-09-01: A failed cell-climate override logs a warning and re-resolves on every exterior frame

**Labels**: bug, low, performance, terrain-exterior

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-09.md` — finding `PERF-D7-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**:
  - `byroredux/src/app_step.rs:92-106`: the per-frame call, deliberately outside the `grid_changed` guard. The comment there says it "Costs one map lookup and an `Option<u32>` compare on every other frame".
  - `byroredux/src/scene/world_setup.rs:472-514` (`apply_cell_climate_override`).
  - `byroredux/src/env_translate.rs:642-658` (`resolve_cell_climate`'s warn) and `:683-714` (`resolve_default_weather`).
- **Status**: NEW. The code has existed since `a919fcd70` (2026-08-13, #2451) and is reported here for the first time.
  - Related but distinct: #5463 (open) covers what TES5/FO4 `XCCM` means; it mentions the warning but not that it fires every frame. #5424 (closed) covers the WTHS stand-in. #3679 (closed) fixed the region-ambient sibling of this call with a per-grid cache.
- **Description**: `step_streaming` calls `apply_cell_climate_override` on every frame in exteriors. It does no caching of its own. Two failure arms re-run each frame:
  1. **The cell's `XCCM` is not a parsed `CLMT`.** `resolve_cell_climate` emits `log::warn!` and returns the worldspace climate. That equals `applied_climate`, so the caller returns `false`, and the warning fires again on the next frame.
  2. **The override climate resolves but has no default weather.** The caller warns and returns `false` without recording anything. The comment says this is "so a later fix (or a different cell) is still re-evaluated". So every frame re-runs `resolve_default_weather` and warns again.
     - Since #5424, that re-run includes `default_weather_by_edid`: an O(all WTHR records) EDID string scan whenever the climate carries WTHS rows.
- **Trigger**:
  - A modded Skyrim or FO4 exterior that authors `XCCM`. On those games it is a `REGN` reference (#5463), so it never matches a `CLMT`.
  - A missing master.
  - A `CLMT` whose weather list points at WTHR records the load order never supplied.
  - Vanilla: none. #5463's census found all 214 + 35 + 19 Skyrim/DLC `XCCM` cells are interiors.
- **Evidence**:
  - `world_setup.rs:486-493`: resolve first, then compare with `applied_climate`.
  - `:499-514`: warn plus `return false` with no state recorded.
  - `env_translate.rs:653`: an unconditional `log::warn!`.
- **Impact**: one formatted warning per frame (60–144 lines per second) written through the logger, for as long as the player stands in that cell. Arm 2 also repeats the resolve work. Failure path only; the sky itself stays correct. No quantitative guard exists for this site.
- **Related**: #5463, #5424, #3679 (the cache shape to copy), #5339 (the same "failure retried every frame" class in `water.rs`).
- **Suggested Fix**:
  - Cache the decision per `(worldspace_key, player_grid)`, as `applied_region_ambient` does, so the resolve and any warning run once per grid change.
  - Optionally record a failed override per override FormID so it warns once per session.
  - Correct the `app_step.rs:96-98` cost comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
