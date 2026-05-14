## Description

Dead code, stale `#[allow(dead_code)]` annotations, and backwards-compat cruft. The repo has good hygiene culture overall (CLAUDE.md global instruction: "If you are certain that something is unused, you can delete it completely"), but ~13 sites have drifted past their original justifications. Total estimated reduction: ~150 LOC, ~13 mutes removed, ~7 dead `pub fn`s, ~30 unused re-exports.

Net effect of fixing this batch: `#[allow(dead_code)]` count drops from 42 → ~29; dead-code lints regain teeth in debug builds where they're currently silenced.

## Findings consolidated

### Stale markers (Dim 1)
- **TD1-001 / TD1-002** — `// TODO: thread StagingPool through scene load (#242)` in `byroredux/src/scene.rs:477` and `byroredux/src/main.rs:1144`. Issue #242 closed on commit date (2026-04-13). Either delete the comment or open a fresh issue for current code path. (Same scenario as TD5-005 — StagingPool plumbing.)

### Stale `#[allow(dead_code)]` where consumer landed (Dim 2 + Dim 8 overlap)
- **TD2-001 / TD8-004** — `byroredux/src/cell_loader/unload.rs:33`: `unload_cell` is called from `main.rs:750, 1046`. Mute lies.
- **TD2-002** — `byroredux/src/cell_loader/exterior.rs:34`: `OneCellLoadInfo.cell_root` consumed at `scene/world_setup.rs:659` and `main.rs:905`.
- **TD2-003** — `byroredux/src/cell_loader/load.rs:24`: `CellLoadResult` struct-level mute; specific fields ARE read.
- **TD8-004** (additional) — `cell_loader/nif_import_registry.rs:57`: `CachedNifImport.embedded_clip` is read at `references.rs:497`.

### Test-only items that should be `#[cfg(test)]`, not muted
- **TD2-004** — `byroredux/src/scene_import_cache.rs:101,109,114,119`: `parses()`, `hits()`, `misses()`, `len()`.
- **TD2-005** — `byroredux/src/parsed_nif_cache.rs:167,195`: `is_empty()`, `clear_entries()`; chain back to `NifImportRegistry::clear` which is itself only called from tests.
- **TD2-006** — `crates/renderer/src/deferred_destroy.rs:106`: `DeferredDestroyQueue::is_empty()`.

### Truly unused renderer pub fns
- **TD2-007** — `crates/renderer/src/vulkan/volumetrics.rs:900`: `integrated_view(frame)` singular variant; `integrated_views()` plural is the actual consumer-facing API.
- **TD2-009** — `crates/renderer/src/vulkan/scene_buffer.rs:1255`: `instance_buffer_mapped_mut`; UI overlay path uses `upload_instances` instead.
- **TD2-010** — `crates/renderer/src/vulkan/acceleration.rs:704, 3398, 3434, 3446, 3464`: five orphan `*_bytes`/`*_telemetry` getters. Pattern: commits `cb230ad8` + `3314ee08` shipped getters without wiring consumers — pattern flag for review hygiene.

### CLAUDE.md policy violation
- **TD2-008** — `crates/nif/src/import/material/texture_slot_3_4_5_tests.rs:521`: `fn _uses_ni_texturing_property() -> NiTexturingProperty { panic!() }` — directly violates the "delete completely, don't rename to `_var`" rule.

### Debug-only hash helpers (gate, don't mute)
- **TD2-011** — `crates/bsa/src/archive.rs:29,64`: `genhash_folder`/`genhash_file` — currently `#[allow(dead_code)]`, but the call sites are inside `if cfg!(debug_assertions) { ... }` blocks. Should be `#[cfg(any(debug_assertions, test))]`, dropping the mute and re-engaging warnings in debug.

### Dead helpers (delete outright)
- **TD8-005** — `NifImportRegistry::clear` (`cell_loader/nif_import_registry.rs:146`), `ParsedNifCache::is_empty` (`parsed_nif_cache.rs:167`), `clear_entries` (`:195`) all genuinely dead.

### Unused fn parameters / shims
- **TD8-001** — `TextureRegistry::new` carries `_swapchain_image_count` not used post-bindless. Single caller.
- **TD8-002** — `debug_server::start` takes `_world: &mut World` it never reads. Single caller; removing might free `main.rs` to interleave borrows.
- **TD8-003** — `SkinnedMesh::new` is a legacy shim duplicating `new_with_global`; zero production callers.
- **TD8-008** — `humanoid_skeleton_path` + 2 siblings take `_gender: Gender` "for a future mod-aware lookup." Textbook CLAUDE.md anti-pattern.
- **TD8-006** — `RefrTextureOverlay::inner` (`cell_loader/refr.rs:64-65`) preserved "for parity" with no consumer; round-trip claim is hypothetical (no save/load yet).

### Crate-root unused re-exports
- **TD8-007** — ~30 unused `pub use` re-exports across 8 crate roots:
  - `renderer/src/lib.rs:7-12`: `MeshRegistry`, `TextureRegistry`, `DrawCommand`, `ScreenshotHandle`, `GpuMaterial`
  - `bsa/src/lib.rs:26`: `MAX_CHUNK_BYTES`, `MAX_ENTRY_COUNT`
  - `bgsm/src/lib.rs:48`: `AlphaBlendMode`, `ColorRgb`, `MaskWriteFlags`
  - `audio/src/lib.rs:140`: `kira::Frame`
  - `facegen/src/lib.rs:30-33`: `EgmMorph`, `EgtFile`, `EgtMorph`, `TriHeader`
  - `spt/src/lib.rs:54-58`: `TAG_MAX`, `TAG_MIN`, `SptScene`, `SptValue`, `TagEntry`, `SptStream`, `dispatch_tag`, `SptTagKind`, `detect_variant`, `SpeedTreeVariant`
  - `scripting/src/lib.rs:17-20`: `ActivateEvent`, `AnimationTextKeyEvent`, `AnimationTextKeyEvents`, `HitEvent`, `TimerExpired`, `ScriptTimer`
  - `physics/src/lib.rs:23`: `PlayerBody`

### Future-tracked (leave alone)
- **TD2-013** — Staged-rollout XCLL fields + RawDependency — documented in file header; keep.
- **TD2-014** — `MSWP peek_path_filter` — reserved for FO4-DIM6-02; just add issue link.
- **TD2-015** — NIF schema-completeness mutes (`VF_UVS_2`, `VF_LAND_DATA`, `VF_INSTANCE`, `VF_FULL_PRECISION`) — defense-in-depth, keep.
- **TD2-016** — BA2 `Dx10Chunk.start_mip`/`end_mip` — needed for M40 partial-mip-range streaming; add `// TODO(M40 streaming)` link.

## Suggested approach

Single PR (or 2-3 small ones) running mechanical deletes + grep verification. `cargo check` at each step. The whole batch is ~1-2 hours of grep-and-delete work, guarded by the type system.

## Source

- Audit: `docs/audits/AUDIT_TECH_DEBT_2026-05-13.md`
- Findings: Dim 1 (TD1-001/002) + Dim 2 (TD2-001 through TD2-016) + Dim 8 (TD8-001 through TD8-008)
- Dimensions: Stale Markers, Dead Code, Backwards-Compat Cruft

## Completeness checks

- [ ] `grep -RcE 'allow\(dead_code\)' crates byroredux` ≤ 30 (currently 42)
- [ ] No `_var`-prefixed function survivors in non-test code
- [ ] `grep -RnE '// (removed|kept|deprecated|legacy|compat|backward)' crates byroredux | grep -v 'crates/plugin/src/legacy/'` returns nothing actionable
- [ ] Zero unused crate-root `pub use` re-exports — verify with downstream import grep
- [ ] `genhash_folder/file` gated by `cfg(any(debug_assertions, test))`, mute removed
- [ ] `cargo check` and `cargo test` both pass at every step

