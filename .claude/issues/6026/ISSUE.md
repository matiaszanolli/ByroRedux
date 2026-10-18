# EXT-D1-2026-10-18-01: The Show-Sky interior climate (and the forced-weather re-apply) enqueue cloud / sun-sprite DDS uploads that nothing flushes. Debug builds trip `finish`'s own `debug_assert`; release builds sample the fallback…

Labels: medium,terrain-exterior,renderer,bug,game:oblivion,game:fo3,game:fnv
Source: docs/audits/AUDIT_EXTERIOR_2026-10-18.md

**Full title**: EXT-D1-2026-10-18-01: The Show-Sky interior climate (and the forced-weather re-apply) enqueue cloud / sun-sprite DDS uploads that nothing flushes. Debug builds trip `finish`'s own `debug_assert`; release builds sample the fallback checkerboard for the life of the cell

- **Severity**: MEDIUM. It defeats #5785's purpose (authored clouds in a sky-exposed interior) on every first entry, and it is a debug-build panic on the new path. Contained to TES4 / FO3 / FNV cells that author `XCCM` + Show Sky (Skyrim / FO4 route `XCCM` to `sky_region` since #5463).
- **Dimension**: EXAL boundary discipline
- **Location**:
  - `byroredux/src/cell_loader/load.rs:729` (call) and `:789` (`debug_assert_eq!(pending_dds_upload_count(), 0)`), both in `InteriorCellApplyJob::finish`.
  - `byroredux/src/scene/world_setup.rs:529` (`apply_show_sky_interior_climate`) -> `:299` (`apply_environment_with_sources`) -> `:132` `resolve_cloud_layer` (`:168` `resolve_texture`) and `:909` `resolve_sun_sprite`.
  - Second caller: `world_setup.rs:272` `apply_forced_weather` (`app_step.rs`, new in `d1b298921`).
- **Status**: NEW (introduced by `34a820ceb` and `d1b298921`).
- **Tier Violated**: n/a (a missing batch-flush owner)
- **Game Affected**: Oblivion, FO3, FNV (Show-Sky interiors with `XCCM`); any exterior session using ObScript `ForceWeather`.
- **Description**:
  - `resolve_texture` does not upload. It reserves a bindless slot, points the descriptor at the fallback checkerboard, and queues the DDS (`texture.rs:743-768`, `upload.rs:231-253`). "The engine has no per-frame flush"; the owning loader must drain the queue (`load.rs:1137` doc, `loot_appearance.rs:729` "the frame renderer does not drain it").
  - Interior loads flush once, forced, at `load_references`' tail (`references/mod.rs:902`). `finish` runs after that, and `34a820ceb` put `apply_show_sky_interior_climate` at its top. The clouds' four layers and the CLMT sun sprite are therefore queued with nobody to flush them.
  - The exterior callers of `apply_environment` are followed by a cell load, which flushes (`begin_exterior_streaming`, then the first cell's forced flush). The new interior caller is not followed by one. `apply_forced_weather` is called mid-session from `app_step.rs` and is flushed only if a cell or LOD attempt happens to follow (`flush_pending_lod_textures` runs `if attempted > 0`).
  - `finish`'s own assertion exists for exactly this class (#3320): "a `resolve_texture` call has moved below `load_references`' flush".
- **Evidence**:
  - `finish` order: `apply_show_sky_interior_climate(...)` (`:729`) ... `debug_assert_eq!(ctx.texture_registry.pending_dds_upload_count(), 0, ...)` (`:789`). Nothing between them flushes (`populate_*` load clips, not textures).
  - The only `flush_pending_uploads` callers are `streaming_helpers.rs:485`, `scene.rs:182`, `loot_appearance.rs:740/1056` and `references/mod.rs:919`. None runs after `finish` for an interior.
  - The new test `show_sky_interior_climate_resolves_its_xccm_default_weather` covers only the climate / weather resolution (a `HashMap` lookup); no test reaches a `VulkanContext`.
  - Cache-hit case: if the realm's cloud textures are already resident (second entry), `acquire_by_path` hits and nothing is queued, so the defect shows on first entry per session.
- **Disproof attempted**: could `resolve_cloud_layer` bypass the queue? No: it calls `resolve_texture` (`world_setup.rs:168`). Could the cover logic or a frame hook drain it? `grep flush_pending_uploads` finds no per-frame caller. Not confirmed at runtime (no engine launch allowed).
- **Impact**: Debug build: panic when entering an Oblivion-realm-style Show-Sky interior for the first time. Release build: the cloud layers render the fallback checker (magenta / black) through windows and in the Show-Sky sky until the next cell load flushes. For `ForceWeather` while stationary, the new weather's clouds show the checker until the next cell / LOD apply.
- **Related**: #5785 (closed, introduced the path), #3320 (the invariant), `apply_cell_climate_override` has the same shape but is always followed by a streaming apply.
- **Suggested Fix**: One flush owner. Either call `references::flush_pending_cell_textures(ctx)` after `apply_show_sky_interior_climate` in `finish` and after `apply_forced_weather`, or make `apply_environment_with_sources` end with the same forced flush the loose-load path uses (`scene.rs:172`). Add a source-shape pin that every `apply_environment*` caller is followed by a flush before the `finish` assertion.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, other callers)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
