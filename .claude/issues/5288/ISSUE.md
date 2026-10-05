# #5288: PERF-D7-2026-10-05-01: Every Skyrim/FO4 door transition opens the full archive set twice on the main thread (LSCR model cover + `step_cell_transition`), and the cover re-imports its model each time

**Labels**: medium,performance,game:skyrim,game:fo4,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5288

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-05.md` — `PERF-D7-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**:
  - `byroredux/src/loading_screen.rs:312-322` (`spawn_model_stage`) and `:191-231` (`begin_artwork`, called from `begin` and `begin_save`)
  - `byroredux/src/app_step.rs:1041-1042` (Ext→Int) and `:1141-1142` (→Ext)
  - the stale doc at `byroredux/src/app_step.rs:887-933`
  - `byroredux/src/asset_provider/texture.rs:479-535` (`build_texture_provider`)
- **Status**:
  - The cover half is NEW (`e60911864`, 2026-10-02).
  - The transition half is the cost **#2039** (PERF-D7-02, 2026-07-16) recorded. That issue was closed on 2026-07-17 as a design note: "not urgent before Stage 4 interactive door activation". Interactive doors have since shipped (`InteractionKind::Door`, `interaction.rs:1281-1286`, and the `p5-door-transition.sh` smoke), so the deferral's trigger has been met.
  - No open issue covers either half.
- **Description**: `spawn_model_stage` calls `build_texture_provider(&args)` and `build_material_provider(&args)`. That re-opens every `--bsa`/`--textures-bsa` archive (headers and file tables) and builds a cold BGSM/BGEM cache. It then extracts the NNAM model, runs `peek_or_parse_scene` and `load_nif_bytes`, which registers and uploads meshes and textures and builds the BLAS. All of this happens on the main thread, before the cover's first presented frame (`Phase::AwaitingPresentation`).
  - The model path has no key check. The image path at `:249-254` does have one, and reuses its texture when the key matches.
  - The stage is released at cover end (`retire_stage` → `release_entities`), so the next door repeats the whole spawn.
  - `step_cell_transition` then builds a second fresh provider pair for the destination apply (#2039).
  - The doc comment at `app_step.rs:890-893` still says this path is "reachable today only via the `door.teleport` console command".
- **Evidence**: `loading_screen.rs:321-322`; `app_step.rs:1002` (`self.loading_screen.begin(...)` runs ahead of the `:1041` rebuild on the same transition); `asset_provider/texture.rs:479` (no caching: every call opens archives through `open_with_numeric_siblings`).
- **Impact**: each door use pays two full archive-index opens, a model import and GPU upload, and the BLAS build, all on the main thread. The cover was added to hide the transition stall, but it delays its own first frame. #2039 estimated "a few-hundred-ms BSA re-open" per rebuild; FO4's dozens of BA2s are the heavy case. Unmeasured this run. No quantitative guard exists for this site.
- **Related**: #2039 (design note with the cache shape), #5061 (the same "loader opens its own archive set" pattern, fixed for the gear and corpse loaders), #5193 (cover teardown), `save_io.rs:1388/1543` (save-load also rebuilds, but that path is unbudgeted by design).
- **Suggested Fix**: implement #2039's App-owned provider slot, keyed on the plugin-set identity, and lend it to `LoadingScreen::begin*` and `step_cell_transition`. Key the model stage the way the image artwork is keyed, so a repeat cover reuses the registered stage instead of re-importing it.

## Publisher note

The transition half is the cost recorded in the closed design note #2039, whose deferral trigger ("interactive doors ship") has since been met; this issue re-activates it together with the new cover half.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
