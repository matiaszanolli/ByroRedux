# Tech-Debt Audit — 2026-10-05

**HEAD**: `a2c24b16e` · **Baseline**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md` (HEAD `9fcfdc3fc`, 313 commits /
1 015 files / +72 004 −19 966 ago) · **Audited**: all 9 dimensions, delta-first (deep) · **Unchanged since baseline
(skimmed)**: none. Every dimension's `Paths:` had commits in the window (Dim 1/5: 248, Dim 2: 145, Dim 7: 112,
Dim 3: 86, Dim 4: 21, Dim 9: 5). Dims 5, 6 and 7 were reviewed in full and came out clean.

A single agent wrote this report during the `/audit-suite --preset comprehensive` run. It used no sub-agents and
fixed nothing. Findings were checked for duplicates against three sources:

- the 97 open issues in `/tmp/audit/issues.json`;
- every `tech-debt`-labelled issue in any state (500 issues);
- closed issues, via `gh --search`, and the 20 sibling `*_2026-10-05.md` reports.

Doc rot that a sibling report already files is cited as **Related**, not filed again.

## Executive Summary

| Severity | NEW | Regression | Total |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 2 | 0 | 2 |
| LOW | 10 | 1 | 11 |

- **CI's main gate has been red on every push since 2026-10-01 19:03Z (TD8-01).**
  - GitHub's `stable` moved to rustc 1.99.0. Its new `clippy::chunks_exact_to_as_chunks` lint fires at 24
    sites in plugin, renderer, hkx and menuxml. Two rustc future-incompat `f32: From<f64>` fallbacks fire in
    debug-ui.
  - `--keep-going` cannot lint anything that depends on a failed crate, so the `byroredux` binary, sdk,
    scripting and others have not been clippy-checked in CI since then.
  - Locally the only toolchain is 1.96, where the gate and `--all-targets` are both **green**. Developers
    therefore cannot see what CI sees.
  - This is the same class as last week's #5121 (rustc 1.98.1), so it needs a structural fix: a pinned
    toolchain or a beta canary.
  - Three of 10 CI jobs are red at HEAD, down from 5 on 09-29:
    - clippy: this report
    - ABBA: ECS-2026-10-05-D1-01
    - Vulkan validation: CONC-D3-2026-10-05-01
- **The interior and exterior CELL walkers are parallel copies with a history of divergent fixes (TD2-01).**
  - 20 sub-record arms and about 20 accumulator locals are spelled twice, in `walkers.rs` and `wrld.rs`.
  - #1220 already had to repair the exterior copy after the XCRI/XPRI fix landed only in the interior one.
- **Oversized files: still 5 in production, but the set changed.**
  - New crosser: `crates/physics/src/world.rs`, which went from 1659 to **2101** (+442 in five #5126/#5160/#5161/#5246
    commits). A split axis is proposed below.
  - New crosser: `shader_constants_data.rs` at 2004. It is a data table, so it folds into #5097.
  - The three open splits grew again: `volumetrics.rs` 2391 → 2510 (#5094), `actor/mod.rs` 2120 → 2171
    (#5093), `streaming.rs` 2069 → 2106 (#5092).
  - The #5087/#5089/#5090/#5091 splits retired four files and **held**.
  - `draw_frame` sits at 718 of its 720-line budget.
- **The baseline's big process items are mostly resolved.**
  - The open-issue pool dropped from 163 to 97 (the TD4-01 sweep).
  - The traceability gate is green (#5085).
  - The `--all-targets` clippy debt fell from 824 to 0 (#5115).
  - AGENTS.md is now a symlink (#5107).
- **Doc rot left by the window's file splits (TD3-01).**
  - Six code comments still name the deleted `npc_spawn/resumable.rs` as a live location.
  - Two comments name `draw.rs` for helpers that now live in `frame_params.rs` / `build_and_upload_instances.rs`.
  - `_audit-validate.sh` scans only skills and `docs/engine`. Comment-level `.rs` references that resolve
    nowhere went from 130 to 141 in the window.

## Baseline Snapshot (re-measured at `a2c24b16e`; `prod_loc` self-test ok)

```text
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 22   (09-29: 21)   15 are ESM XXXX false positives
allow(dead_code):                             34   (09-29: 27)   +3 tree_lod_* (#4913), +4 see TD8 notes
unimplemented!/todo!():                        0   (09-29: 0)
#[ignore] tests:                             265   (09-29: 250)  all new ones data-gated
files >2000 production LOC:                    5   (09-29: 7)    4 retired by splits, 2 new crossers
test files >2000 total LOC:                   66   (09-29: 61)
production fns >200 LOC:                     158   (same scanner on the 9fcfdc3fc tree: 151)
clippy --workspace -D warnings (rustc 1.96):   0   GREEN locally; RED on CI's rustc 1.99.0 (TD8-01)
clippy --workspace --all-targets (rustc 1.96): 0   (09-29: 824 sites) — #5115
_audit-validate.sh:                           OK — 0 STALE; 69 basename / 2 skill / 272 docs-engine / 1 tree advisories
comment `.rs` refs resolving nowhere:        141   (9fcfdc3fc tree: 130) — not policed by any gate
```

## Top 10 Quick Wins (trivial / small)

1. **TD8-01**:
   - Replace the 24 `chunks_exact(N)` sites with `as_chunks::<N>()`.
   - Annotate the two debug-ui float literals.
   - Then add `rust-toolchain.toml`, or a non-blocking `beta` clippy canary, so the next lint lands as a warning
     before it lands as red.
2. **TD3-01**: rewrite the 6 `npc_spawn/resumable.rs` references, the 2 `draw.rs` references and the 3
   `walkers.rs:158-…` line cites.
3. **TD3-02**: delete the orphaned two-line `/// Regression for D6-04 / #1811 …` fragment in `draw.rs`.
4. **TD8-02**: `#[cfg(feature = "debug-server")]` on `debug_server_allowed`, plus the `mut`, so the
   `--no-default-features` lane builds warning-free.
5. **TD4-01**: restate the `gpu_material_size_claims` scope in this skill (post-#5203).
6. **TD4-02**: change `/audit-scripting` Dim 8 to Dim 5 in audit-parsers, and give the BNAM-vs-MODB question an
   open tracker.
7. **TD4-03**: trim ROADMAP.md back under 800 lines, or move the cap check into a CI or text-integrity gate.
8. **TD8-03**: add `scratch:` to the disposable-example guard and re-word or delete the six "Scratch:" probes.
9. **TD3-04**: add the LSCR turntable row to game-loop.md and fix the footstep row.
10. **TD3-03**: bring feature-matrix.md's gameplay table to the 10-01 slice closure, adding P3–P5 rows and
    marking corpse loot ✓.

## Top 5 Medium Investments

1. **TD2-01**: introduce one `CellSubrecordFields` accumulator in `crates/plugin/src/esm/cell/helpers.rs` with
   `absorb(&mut self, reader, sub) -> bool`. Both CELL walkers call it and keep only their own arms
   (interior `DATA`/`XCLL`, exterior `XCLC`).
2. **TD1-01**: split `crates/physics/src/world.rs` into `world/recovery.rs` (explosion containment),
   `world/queries.rs` (ray, capsule, census and KCC) and a core file (struct, lifecycle and `step`).
3. **#5094 / #5093 / #5092** (open, still growing): see the "Existing issues that grew" table.
4. **#5100** (open, grew by 3): migrate the nine `production_text` copies onto `crates/core/src/source_scan.rs`.
   That module landed on 10-02 without absorbing the three copies added on 10-01.
5. **TD2-02**: hoist the shared Armor-phase unit (and the common Skeleton-phase core) of the two
   `npc_spawn/resumable` state machines into `resumable/mod.rs`.

---

## Findings

### MEDIUM

### TD8-2026-10-05-01: CI `Test + Check + Clippy` has been red on every push since 2026-10-01 — rustc 1.99's `chunks_exact_to_as_chunks` lint (24 sites) plus two debug-ui float-fallback errors; dependents are never linted
- **Severity**: MEDIUM.
  - It is promoted because a red board has hidden real signal for 4 days and 40+ pushes, and this is the
    second toolchain-drift red in a row (#5121).
  - The gate also stops linting every crate downstream of the failures.
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft (clippy gate)
- **Location**: `.github/workflows/ci.yml:138` (`dtolnay/rust-toolchain@stable`), the clippy step
  `cargo clippy --workspace --keep-going -- -D warnings`. The failing sites:
  - **plugin (17)**:
    - `esm/cell/helpers.rs:40,112`
    - `esm/cell/walkers.rs:387`
    - `esm/cell/wrld.rs:554`
    - `esm/records/actor/mod.rs:1476,1482,1535`
    - `esm/records/load_screen.rs:199`
    - `esm/records/misc/imagespace.rs:157,161`
    - `esm/records/misc/water.rs:1464`
    - `esm/records/misc/world.rs:636,1448`
    - `esm/records/outfit.rs:80`
    - `esm/records/pathgrid.rs:94,109`
    - `esm/records/tree.rs:190`
  - **renderer (4)**:
    - `vulkan/context/depth_capture.rs:100`
    - `vulkan/context/screenshot.rs:91`
    - `vulkan/groundcover/frame.rs:183`
    - `vulkan/pipeline.rs:25`
  - **hkx (2)**: `animation.rs:503,722`
  - **menuxml (1)**: `raster.rs:81` (`chunks_exact_mut`)
  - **debug-ui (2)**: rustc future-incompat "falling back to `f32` as the trait bound `f32: From<f64>` is not
    satisfied" at `panels.rs:170,471`, an error under `-D warnings`
- **Status**: NEW. It is the same class as CLOSED #5121, which covered rustc 1.98.1 in sdk and nif.
  `AUDIT_SAFETY_2026-10-05` §"Other existing items" mentions the red as "lint drift" but files nothing.
- **Age**: rustc 1.99.0 was released 2026-09-28. The first red run on it was `36908954333` (`4ad847a81`,
  2026-10-01 18:43Z). Every `ci.yml` push since then is red; I sampled 40 of them through HEAD run
  `37344961054`.
- **Effort**: small
- **Description**:
  - The clippy job installs whatever `stable` is on the day. The repository pins no toolchain, and the only one
    installed locally is 1.96 (`rustup toolchain list`: stable = 1.96.0).
  - On 1.96, `cargo clippy --workspace --keep-going -- -D warnings` and the `--all-targets` variant both exit 0
    here. Nobody can reproduce the CI failure locally.
  - Because the lint is an error in five library crates, `--keep-going` builds no metadata for them. Every
    crate that depends on them goes unlinted on CI: the `byroredux` binary, sdk, scripting, the debug server and
    the tools.
  - Behind the first layer there is already a second one. On 1.99 the `cargo check` steps of the same job warn
    `unused import: super::builders::*` at `byroredux/src/cornell/{glass_dragon,godray_lab,oracle}.rs:4`. Those
    warnings become clippy errors once the libraries are green.
- **Evidence**:
  - Run `37344961054`, job `111881105065`:
    ```
    rustc 1.99.0 (b940084d7 2026-09-28)
    error: using `chunks_exact` with a constant chunk size   --> crates/plugin/src/esm/records/pathgrid.rs:94:39
      = help: ... #chunks_exact_to_as_chunks
    error: could not compile `byroredux-menuxml` (lib) due to 1 previous error
    error: could not compile `byroredux-hkx` (lib) due to 2 previous errors
    error: could not compile `byroredux-debug-ui` (lib) due to 2 previous errors
    error: could not compile `byroredux-plugin` (lib) due to 17 previous errors
    error: could not compile `byroredux-renderer` (lib) due to 4 previous errors
    ```
  - Job history for `Test + Check + Clippy`:
    - failure on every run from 09-30 15:42Z through HEAD;
    - 09-30 was #5121's 1.98.1 layer: sdk and nif, plus an example compile;
    - 1.99 from 10-01 18:43Z.
- **Impact**:
  - Four days of pushes, including the whole 10-01 → 10-04 fix wave, got no CI lint signal for the binary
    crate.
  - A permanently red board teaches readers to ignore it. It is red alongside ABBA (ECS-2026-10-05-D1-01) and
    Vulkan validation (CONC-D3-2026-10-05-01).
  - The renderer-only `undocumented_unsafe_blocks` step still runs independently (#5121's fix), so the safety
    gate is unaffected.
- **Related**: #5121 (CLOSED, previous layer), #3894/#4765 (toolchain-drift class), SAFETY-2026-10-05 §"Other
  existing items", ECS-2026-10-05-D1-01, CONC-D3-2026-10-05-01.
- **Suggested Fix**:
  - Mechanically rewrite the 24 sites to `as_chunks::<N>().0` (the lint's own suggestion). Give the two debug-ui
    literals an explicit `f32` suffix. Delete the three unused `builders::*` imports.
  - Then stop the recurrence. Either commit a `rust-toolchain.toml` and bump it deliberately, or add a
    `continue-on-error` clippy job on `beta` so a new lint shows as a warning a release early.

### TD2-2026-10-05-01: The interior and exterior CELL sub-record walkers duplicate 20 arms and their accumulator locals, and the duplication already produced a one-sided fix (#1220)
- **Severity**: MEDIUM. Promotion rule: duplicated logic with a divergent bug-fix history.
- **Dimension**: 2 — Logic Duplication
- **Location**:
  - `crates/plugin/src/esm/cell/walkers.rs:215-560` (`parse_cell_group_inner`: locals at 219-260, sub-record
    arms at about 296-560)
  - `crates/plugin/src/esm/cell/wrld.rs:378-560` (`parse_wrld_children_inner`: locals at 378-420, arms at about
    456-560)
- **Status**: NEW. No open or closed issue proposes the consolidation. LEGACY_COMPAT 2026-05-19 and #1220 fixed
  only the symptom.
- **Effort**: medium
- **Description**:
  - Both walkers declare the same accumulator set (`display_name`, `water_height`, `water_height_is_explicit`,
    `image_space_form`, `water_type_form`, `acoustic_space_form`, `music_type_form`, `music_type_enum`,
    `climate_override`, `location_form`, `encounter_zone_form`, `regions`, `lighting_template_form`, the
    ownership triple, `regional_color_override`, the XCRI/XPRI precombine fields, …).
  - They then decode the same 20 sub-records: XCLW, XCIM, XCWT, XCAS, XCMO, XCMT/XCCM, XLCN, XEZN, XCLR, LTMP,
    XOWN/XRNK/XGLB, RCLR, XCRI, XPRI, FULL, …. A set difference over the two functions' `b"XXXX"` literals
    leaves only `DATA`/`XCLL` (interior) and `XCLC` (exterior).
  - The exterior copy's comments say "see the interior walker above". That walker is in a different file.
- **Evidence**:
  - **Divergent-fix history.** LEGACY_COMPAT 2026-05-19 found the exterior walker "hardcodes empty XCRI/XPRI on
    a wrong premise" after the interior one gained them. #1220 (CLOSED 2026-05-21) copied the arms across.
  - **The arms are still spelled two ways today.**
    - Interior `b"XRNK" => SubReader::new(&sub.data).i32().ok()` (`walkers.rs:418`).
    - Exterior `b"XRNK" if sub.data.len() >= 4 => Some(i32::from_le_bytes([...]))` (`wrld.rs:491-498`).
    - XOWN/XGLB carry a length guard only on the exterior side.
  - The cross-file cites have already rotted. `wrld.rs:429` and `:511` and `tests/wrld.rs:354` say the
    interior XCRI decode is "at `walkers.rs:158-190`/`158-204`". It is now at `walkers.rs:344`.
- **Impact**:
  - Every new CELL sub-record (Starfield, FO76 or modded) needs two edits.
  - #1220 shows that the second edit gets forgotten, and silent under-coverage on exteriors is the result. For
    FO4 precombines that is the headline exterior performance feature.
- **Related**: #1220, #1188 (CLOSED); LC-D3-02 (LEGACY_COMPAT today: Starfield XCLL/LGTM twin decoders, the same
  class one layer down); ESM-2026-09-21-D1-02 (skip arms duplicated across the same walkers); TD3-01 (stale
  line cites).
- **Suggested Fix**:
  - Add `struct CellSubrecordFields { … }` with `fn absorb(&mut self, reader: &EsmReader, sub: &SubRecord) -> bool`
    to `cell/helpers.rs`, where `read_form_id` and `gated_water_height` already live.
  - Each walker keeps only its own arms and falls through to `fields.absorb(...)`.
  - A single `tests/` case that feeds the same sub-record list through both walkers pins equivalence.

### LOW

### TD1-2026-10-05-01: `crates/physics/src/world.rs` crossed 2000 production LOC (1659 → 2101 in five commits)
- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/physics/src/world.rs` (4495 total lines, 2101 production)
- **Status**: NEW (first crossing)
- **Age**: `ea04f2689` (09-30, #5126/#5127) → `483776fe5` (#5160) → `44f7bab55` / `5ae7f8ad4` (#5161) →
  `e8de9f8c8` (10-04, #5246). +760 −50 lines.
- **Effort**: medium
- **Description**: the file already has two internal seams, and the window's growth sits in a third:
  - **Lifecycle and step** (`impl PhysicsWorld` at 454-1394): `new`, body add/remove, forces, motion type,
    `step` (1144-1388, **244 lines**).
  - **Queries and character motion** (`impl PhysicsWorld` at 1469-2076): `cast_ray*`, `cast_ray_corridor`,
    `line_of_sight_blocked`, the capsule probes, `colliders_near_xz`, `static_colliders_aabb`,
    `move_character`, plus the `CharacterMove*` types and the group/filter helpers at 129-205.
  - **Explosion recovery and containment** (the growth): `DynamicBodySnapshot` / `body_needs_recovery` /
    `restore_invalid_dynamic_bodies` (365-454), `refresh_query_geometry_after_restore`,
    `recover_pre_broken_bodies`, `accept_keyframe_target`, the label/refusal counters and
    `clamp_explosive_velocities` (822-1114, 145 lines), plus the sanity-cap constants at 59-110.
- **Suggested Fix**:
  - Turn `world.rs` into `world/mod.rs` holding the struct, lifecycle and `step`. Move the recovery cluster to
    `world/recovery.rs` and the query/KCC impl block to `world/queries.rs`.
  - Pull `step`'s per-substep body (snapshot → pipeline.step → clamp → restore) into a `run_substep` helper.
  - Mind *feedback_file_split_include_str*: grep for `include_str!("world.rs")` before moving anything. The
    `wake_contract_tests` module (4382+) reads the file's own docs.
- **Related**: SAFE-D3-2026-10-05-01 (the recovery maps that `remove_body` does not prune live in this
  cluster); PHYSICS-2026-10-05 findings at `world.rs:91-99`, `:949-1048`, `:1295-1314`.

### TD2-2026-10-05-02: The two `npc_spawn/resumable` state machines carry a 94%-identical Armor phase, and the copies already diverge in diagnostics
- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**: `byroredux/src/npc_spawn/resumable/prebaked.rs:252-301` and `byroredux/src/npc_spawn/resumable/runtime.rs:794-845`.
  The Skeleton arms are at `prebaked.rs:151-193` and `runtime.rs:508-574` (56% similar).
- **Status**: NEW. The #5091 split (`37db35cca`) made the two copies siblings. They previously lived in one file.
- **Effort**: small
- **Description**:
  - Both Armor arms do the same work: extract the model, build the `hide_skin_partitions` pre-spawn closure,
    call `load_nif_bytes_with_skeleton` with the same nine arguments, `parent_equipment_part`, track
    `original_roots`, and count the armor.
  - `difflib` finds one difference. The runtime miss log prints `armor.resolved_fid` plus
    `(from CNTO {source_fid})`, while the prebaked one prints `armor.form_id` only, so the prebaked path loses
    the CNTO provenance.
  - The Skeleton arms share the extract → load → install-fallback-ragdoll core. Only the runtime one logs "no
    root entity", and the prebaked one carries a comment saying it mirrors the runtime arm ("Same no-bone-collider
    contract as the runtime Skeleton phase").
- **Suggested Fix**:
  - Move the Armor unit into `resumable/mod.rs` as `spawn_armor_unit(state_parts…, armor) -> UnitOutcome`, next to
    the helpers both arms already share (`hide_skin_partitions`, `parent_equipment_part`).
  - Extract the Skeleton phase's common core the same way.
- **Related**: LC-D3-02 / #5079 (the Child-flag translation is duplicated between `player_body.rs` and
  `resumable/runtime.rs`).

### TD3-2026-10-05-01: The window's file splits left code comments pointing at deleted or moved files (`npc_spawn/resumable.rs` ×6, `draw.rs` ×2, `walkers.rs:158-…` ×3)
- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**:
  - **`npc_spawn/resumable.rs`** (deleted by #5091 / `37db35cca`, now `resumable/{mod,prebaked,runtime}.rs`):
    - `byroredux/src/player_body.rs:14,202`
    - `byroredux/src/scene_import_cache.rs:25`
    - `byroredux/src/material_translate.rs:2486`
    - `byroredux/src/npc_spawn.rs:1604`
    - `byroredux/src/save_io/round_trip_tests.rs:1431`
  - **`draw.rs`**:
    - `byroredux/src/render/camera.rs:115` says `origin_corrected_prev_view_proj` is "in
      `vulkan/context/draw.rs`". It is at `context/frame_params.rs:2102` since #5087.
    - `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:280` says it "mirrors the `GpuInstance` morph
      lookup in `draw.rs`". That lookup is at `context/build_and_upload_instances.rs:449`.
  - **`walkers.rs:158-190` / `158-204`**: `crates/plugin/src/esm/cell/wrld.rs:429,511` and
    `crates/plugin/src/esm/cell/tests/wrld.rs:354`. The interior XCRI arm is at `walkers.rs:344`.
- **Status**: NEW
- **Effort**: trivial
- **Description**:
  - `_audit-validate.sh` resolves backticked paths only in the audit skills and `docs/engine/*.md`. Code comments
    have no gate.
  - On the baseline tree, 130 backticked `.rs` references in comments resolved nowhere. At HEAD there are 141.
  - Most of the 141 are legitimate provenance ("Split from `boot.rs`"). The ones listed above are not: they
    claim the code lives there now.
- **Related**:
  - CONC-D1-2026-10-05-02: a FIF rider still points at `groundcover.rs` after #5089.
  - AUD-2026-10-05-D5-03: footstep doc sites.
  - TD2-2026-10-05-01.
- **Suggested Fix**:
  - Re-point the 11 sites.
  - Optionally extend `_audit-validate.sh`, or a hygiene test, to flag a comment that pairs a missing `.rs` path
    with "in"/"see"/"mirrors". Bare "split from" provenance would stay exempt.

### TD3-2026-10-05-02: `draw.rs` carries a truncated, orphaned doc fragment above `draw_frame_size_budget_tests`
- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1304-1305`
- **Status**: NEW
- **Age**: lines from `a60a01534` (2026-07-04). They were orphaned when `e17b4b653` (2026-09-23) moved
  `next_clean_skin_frames` / `should_skip_skin_gpu_refresh` (now `frame_params.rs:1697-1720`, which carries its
  own complete `D6-04 / #1811` docs).
- **Effort**: trivial
- **Evidence**:
  ```rust
  /// Regression for D6-04 / #1811. `next_clean_skin_frames` /
  /// `should_skip_skin_gpu_refresh` gate the bone_world upload + device
  #[cfg(test)]
  mod draw_frame_size_budget_tests {
  ```
  A tree-wide scan for a `///` line that ends mid-sentence immediately before `#[cfg(test)]` finds only this one.
- **Impact**: rustdoc attaches a half-sentence about skin refresh to the size-budget test module. The next
  reader looks for a #1811 regression test that is not there.
- **Suggested Fix**: delete the two lines.
- **Related**: GAME-D1-2026-10-05-03 and UI-D5-2026-09-29-01 / #5029 (the same "doc comment spliced onto the
  wrong item" class).

### TD3-2026-10-05-03: feature-matrix.md's gameplay table predates the 2026-10-01 slice closure (P1/P2 "not closed", corpse loot ✗, no P3–P5 rows), and the slice doc contradicts itself on P1
- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `docs/feature-matrix.md:223-231`; `docs/engine/playable-vertical-slice.md:3` vs `:98`
- **Status**: NEW
- **Effort**: small
- **Description**:
  - `ROADMAP.md:260-262` says "**Closed 2026-10-01.** All six phases (P0 input → P5 persistence and soak) were
    closed by live gates", and `playable-vertical-slice.md:3` says "complete — P0–P5 closed".
  - `feature-matrix.md`, the status floor, still says:
    - P1 "~ Core traversal gate passes; not closed";
    - P2 "Core checkpoint, not P2 closure";
    - "Corpse interaction / loot transfer | ✗ | P2 remainder".
  - Corpse looting has shipped:
    - `interaction.rs:1904` `lethal_combat_then_physical_activation_loots_corpse_through_its_collider`;
    - `npc_spawn/loot_appearance.rs`;
    - `inventory.rs:770` `is_loot_source`;
    - the 2026-09-17 Skyrim corpse-loot live validation in the slice doc.
  - The table has no P3, P4 or P5 rows at all.
  - Separately, the slice doc's own P1 section still ends "P1 as a whole is not closed yet" (`:98`), against its
    header.
- **Related**: #4747 (OPEN; the combat-sound row at `feature-matrix.md:228`, which AUDIO-2026-10-05 re-confirms);
  #5108 (CLOSED 10-01, synced only the player-body / dialogue / container rows).
- **Suggested Fix**:
  - Add P3–P5 rows with their gate scripts.
  - Mark P1/P2 closed with ROADMAP's named follow-ons (gamepad, #5161).
  - Flip corpse loot to ✓.
  - Re-word `playable-vertical-slice.md:98` to "gamepad sources are a follow-on, not a P1 blocker".

### TD3-2026-10-05-04: game-loop.md's live-schedule table, refreshed by #5110 on 10-01, already misses the LSCR turntable system, and its footstep row describes the pre-#5146 camera emitter
- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**:
  - `docs/engine/game-loop.md:138-165` (the schedule table)
  - `:160` (the footstep row)
  - The registration is at `byroredux/src/boot/schedule/update.rs:599`
    (`scheduler.add_exclusive(Stage::Update, loading_model_turntable_system)`, `e60911864`, 10-02).
- **Status**: NEW
- **Effort**: trivial
- **Description**:
  - The table says it lists "the live schedule built in `App::new`".
  - `loading_model_turntable_system` is the one system added since the refresh (diffed against the baseline
    `boot/schedule/*.rs`) that appears in no row.
  - The Late `footstep_system` row still says it "reads the propagated camera `GlobalTransform` (#848)". Since
    #5146 the character-mode emitter is on the body.
- **Related**:
  - AUD-2026-10-05-D5-03: the same footstep staleness in `late.rs`, `scene.rs` and `components.rs`.
  - ECS-2026-10-05-D5-01: that turntable never turns.
  - #5110 (CLOSED).
- **Suggested Fix**: add an Update row for the turntable, and re-word the footstep row to name the body emitter
  in character mode and the camera emitter in FlyCam boots.

### TD4-2026-10-05-01: This skill still says `gpu_material_size_claims` "does NOT cover `GpuCamera` / `GpuInstance`" — #5203 widened it to every size-pinned `Gpu*` struct
- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:147-151`
- **Status**: NEW
- **Effort**: trivial
- **Description**:
  - #5203 (`d61d06da0`, 10-04) made `pinned_sizes()` cover `GpuMaterial`, `GpuLight`, `GpuTerrainTile`,
    `GpuInstance` and `GpuCamera` (`crates/renderer/src/vulkan/material_tests.rs:1409-1431`).
  - `audit-renderer/SKILL.md:85` already states the widened scope.
  - This skill was re-synced today (`a2c24b16e`) and still sends Dim 3 to hand-check GpuCamera/GpuInstance
    prose that a test now polices.
  - What the guard genuinely cannot see remains `Vertex::SIZE`, a size written without the type name, and
    `GpuWaterParams`-class structs outside `pinned_sizes()`.
- **Suggested Fix**: restate the bullet to cover all five pinned structs, and keep the hand-check instruction for
  the uncovered cases.

### TD4-2026-10-05-02: Two skill cross-references rotted: audit-parsers routes HKX playback to a nonexistent `/audit-scripting` Dim 8, and audit-speedtree tracks an open question on closed #3740
- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/audit-parsers/SKILL.md:10`; `.claude/commands/audit-speedtree/SKILL.md:137`
- **Status**: NEW
- **Effort**: trivial
- **Description**:
  - **audit-parsers.**
    - `/audit-scripting` has Dimensions 1–7.
    - Cinematic, root-motion and completion-event playback is Dim 5, "Scene / Package / Dialogue / Cinematic
      Playback".
    - A scan of every `/audit-x Dim N` cross-reference in `.claude/commands` finds only this one mismatch.
  - **audit-speedtree.**
    - "whether Oblivion should size from BNAM or MODB is an open format question (#3740)". #3740 was CLOSED on
      2026-08-31 for its comment half.
    - No open issue tracks the behaviour question, and `AUDIT_SPEEDTREE_2026-10-05` again declines to answer it.
- **Suggested Fix**:
  - Change "Dim 8" to "Dim 5".
  - Either open a research issue for BNAM-vs-MODB and cite it, or reword the line as a dated known-open fact
    with no issue number.

### TD4-2026-10-05-03: ROADMAP.md is 818 lines against the 800-line cap that #5111 restored; fix commits between closes crossed it
- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `ROADMAP.md` (818 lines); the cap is at `.claude/commands/session-close/SKILL.md:264,328`
- **Status**: Regression of #5111 (CLOSED 2026-10-01, "budgets restated as measured ceilings (README ≤ 600,
  ROADMAP ≤ 800) with a wc -l check")
- **Effort**: small
- **Evidence**:

  | Commit | Date | ROADMAP lines |
  |---|---|---|
  | `86a883251` (session 93 close) | 10-01 | 797 |
  | `273692be5` (slice P2 gates) | 10-02 | 801 |
  | `876ad2ae9` (session 94 close, re-trimmed) | 10-03 | 800 |
  | `1dc538108` (Fix #5067) | 10-04 | 818 |
  | `7fab80a26` | 10-04 | 818 |

  README is at 580, within its cap.
- **Description**: the `wc -l` check lives only in session-close Step 6. Any `Fix #N` commit that edits ROADMAP
  between closes can breach the cap silently, and the next close inherits the trim.
- **Suggested Fix**: trim to ≤ 800, and add the two `wc -l` ceilings to an always-run gate
  (`scripts/check-text-source-integrity.sh` or a `workspace_hygiene_tests` case) so the cap holds per commit.

### TD8-2026-10-05-02: The CI `cargo check -p byroredux --no-default-features` lane builds with two warnings: `debug_server_allowed` is dead without the feature, and `scheduler` needs no `mut`
- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/main.rs:78-80` (`fn debug_server_allowed`), `:1051` (`let mut scheduler`); the
  lane is `.github/workflows/ci.yml:172`
- **Status**: NEW
- **Age**: `debug_server_allowed` came with `63c0aee3b` (2026-09-27). Its only production caller is under
  `#[cfg(feature = "debug-server")]` (`main.rs:1069-1070`).
- **Effort**: trivial
- **Evidence**: reproduced locally on rustc 1.96:
  ```
  warning: variable does not need to be mutable  --> byroredux/src/main.rs:1051:13
  warning: function `debug_server_allowed` is never used  --> byroredux/src/main.rs:78:4
  ```
- **Impact**:
  - The lane is not `-D warnings`, so it stays green while the non-default build accumulates warnings.
  - That is the slow-rot shape the feature-lane rule exists to catch (#3894/#4387).
  - A future `-D warnings` on this lane, or a clippy run with `--no-default-features`, fails straight away.
- **Suggested Fix**:
  - Gate `debug_server_allowed` with `#[cfg(any(test, feature = "debug-server"))]`.
  - Make the `mut` conditional by rebinding inside the cfg block.
  - Consider `RUSTFLAGS=-D warnings` on the feature-lane `cargo check` steps.

### TD8-2026-10-05-03: Six committed examples open their module doc with "Scratch:", which the #5114 disposable-example guard does not match; two arrived this window
- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**:
  - `crates/nif/examples/ragdoll_dump.rs:1` (new, `5ae7f8ad4` 10-04)
  - `crates/plugin/examples/xclw_census.rs:8` ("Scratch probe for the W2 LOD-coverage investigation — not a
    gate", new, `98061ec58` 10-03)
  - `crates/nif/examples/texset_dump.rs:1` (09-28)
  - `crates/nif/examples/dump_nolighting.rs:1`, `dump_alpha.rs:1`, `import_probe.rs:1` (May)
  - The guard is `byroredux/src/workspace_hygiene_tests.rs:56-59`
- **Status**: NEW (a gap in CLOSED #5114's guard)
- **Effort**: trivial
- **Description**:
  - `no_committed_example_self_describes_as_disposable` matches a doc line that starts with "throwaway",
    "one-off" or "temp scratch", or that contains "not for commit".
  - "Scratch:" is the codebase's most common self-description of a disposable probe, and it passes.
  - `AUDIT_EXTERIOR_2026-10-05` already found a bug in one of these probes (`xclw_census.rs:89-91`, a missing
    `.abs()`) whose output #5244 cites as reconciliation evidence. So a "scratch" probe is load-bearing evidence
    the moment an issue quotes it.
- **Suggested Fix**:
  - Add `"scratch"` to `LINE_START_MARKERS`.
  - Then either delete each flagged probe or rewrite its doc to state what it measures and why it stays, as in
    `watr_wind_census.rs`.
- **Related**: #5114 (CLOSED), TD8-2026-09-29-01; EXTERIOR-2026-10-05 (the `xclw_census` ring bug).

---

## Existing issues that grew in the window (not re-filed)

| Issue | State | Growth since `9fcfdc3fc` | Driver commits |
|---|---|---|---|
| #5094 `volumetrics.rs` | OPEN | 2391 → **2510** prod LOC (+119); `new_inner` (`volumetrics/init.rs`) 782 → 830 lines | `c705c310d` (#5055/#4784 occupancy mask), `17e17de5e`, `ad872da14`, `3b7e25b91` |
| #5093 `esm/records/actor/mod.rs` | OPEN | 2120 → **2171** (+51) | `de6bdb381` (#5005), `dca2bb401` (#5013) |
| #5092 `streaming.rs` | OPEN | 2069 → **2106** (+37) | `98061ec58` (#5243), `c83e4837a` |
| #5096 per-frame drivers | OPEN | `about_to_wait` 969 → 1009, `render_one_frame` 656 → 716 lines; `draw_frame` 718 / budget 720 | window-wide |
| #5097 shader constants ×3 | OPEN | `shader_constants_data.rs` now **2004** prod LOC (crossed 2000; a data table, so the fix is #5097's single table, not a split) | `4bf2ec3a4`, `4dfe97f3e`, `67969adc2` |
| #5100 `production_text` copies | OPEN | +3 copies (now 9 plus the core helper): `crates/nif/src/version_literal_tests.rs:37` (with the `map_or(src, …)` whole-file fallback #5100 names), `byroredux/src/render/groundcover_hasher_tests.rs:19`, and `draw.rs`'s `production_lines` (#5087). All landed 10-01. `crates/core/src/source_scan.rs` arrived 10-02 (`69fd54fb2`) without migrating them | `72769b95a`, `c57e5cc4a`. Related NIF-D2-2026-10-05-01 (that guard's cut skips about 1,000 production lines) |

## Watch list (within 5% of the 2000 production-LOC line)

| File | prod LOC | Δ since `9fcfdc3fc` |
|---|---|---|
| `byroredux/src/components.rs` | 1997 | +77 |
| `crates/nif/src/import/types.rs` | 1993 | +6 |
| `crates/scripting/src/fragment/effects.rs` | 1940 | +24 |
| `crates/scripting/examples/mq101_conformance.rs` | 1923 | — (example) |
| `byroredux/src/save_io.rs` | 1900 | +62 |
| `crates/renderer/src/vulkan/context/init.rs` | 1877 | −7 |
| `crates/plugin/src/esm/cell/mod.rs` | 1866 | +59 |

Production functions over 200 lines number 158; the same scanner on the baseline tree gives 151. New ones over the
line:

- `register_all` (`debug-server/registration.rs`, 273 lines)
- `activate_ragdoll` (230)
- `apply_environment` (210)
- `build_ragdoll` (209)
- `faction_hostility_system_inner` (205)
- `combat_input_system` (203)

The two `resumable/*` and the `cornell/oracle.rs` entries moved in the splits without growing. The largest growth
was in `build_blas_batched` (890 → 975) and `record_skinned_blas_refit` (920 → 976).

---

## Per-Dimension Notes (clean areas / what was checked)

- **Dim 1**:
  - `prod_loc` self-test ok.
  - The #5087/#5089/#5090/#5091 splits held: `draw.rs`, `groundcover.rs`, `cornell.rs` and `resumable.rs` are
    all under the line, and `draw.rs` / `frame_params.rs` carry file budgets.
  - `pub use` hubs are unchanged: `core/ecs/components/mod.rs` 43, scripting `lib.rs` 26 (+1),
    `records/mod.rs` 23.
- **Dim 2**:
  - Z-up→Y-up has no reimplementation. The `particle.rs` local `zup_to_yup` delegates to
    `core::math::coord::zup_to_yup_pos`.
  - The compute descriptor-set-layout binding arrays (svgf/taa/caustic: 162 `DescriptorSetLayoutBinding::default()`)
    stay unfiled, per TD-2026-09-05's "not worth a helper" verdict.
  - `atomic_write` has one home (`core/atomic_file.rs`).
- **Dim 3**:
  - `_audit-validate.sh` OK, with 0 STALE.
  - The two skill symbol advisories (`FIXTURE_DATA_ENV`, `FIXTURE_GATES`) are shell variables in
    `docs/smoke-tests/fixtures/*.env`. The symbol check scans `.rs` only, so this is gate noise and not rot.
  - Six basename advisories are the documented `triangle_early.frag.spv` truncation.
  - `Vertex` is 104 B (`vertex.rs:329` pin), matching CLAUDE.md.
- **Dim 4**:
  - The CRITICAL/HIGH findings of reports dated 2026-07-03 → 07-07, which newly crossed 90 days, all trace to
    CLOSED issues: SAVE-07 → #1862 (pinned at `save_io/live_reload_tests.rs:227`), SCR-D5-NEW-03 → #1905,
    RT-2 → #1698, CONC #1782.
  - `_audit-common.md` placeholder figures re-measure exactly: cxx-bridge `lib.rs` 26 + 9-line C++ stub;
    platform 60 LOC.
  - The baseline's TD4-01 issue-closure sweep landed (163 → 97 open). #5083, its automation half, remains open.
- **Dim 5**: clean.
  - 22 hits break down as 15 `XXXX`, 3 upstream FIXME quotes (`bgem.rs`, `bs_geometry.rs`, OpenMW in
    `misc/world.rs:278`), the documented `items.rs` TBD, 1 historical (`scene.rs:1318`), and the two
    `loading_screen.rs` TODOs.
  - The `loading_screen.rs` TODOs are MOD2 camera paths. `e60911864` rewrote them 3 days ago, which is under the
    30-day floor.
  - Shader grep: 0 hits. The `triangle.frag` MIT/Burley notice is intact.
- **Dim 6**: clean.
  - 0 `unimplemented!` / `todo!()`. 50 stub/placeholder comments (base 50), all descriptive.
  - Of 264 new `pub` / `pub(crate)` / `pub(super)` fns, the ones with no non-test caller are explained:
    - `tree_lod_*` (#4913, deferred);
    - `tonemap::adaptation_chroma_compress` (a host mirror of `presentation.frag` by design);
    - `reflect::uniform_block_member_offsets_by_name` / `volumetrics_host_buffer_bytes` (test and ledger pins,
      the same pattern as `uniform_block_size_by_name`);
    - sfmaterial `test_support`.
  - No production-unreachable feature found.
- **Dim 7**: clean.
  - The count of hand-written numeric `#define`s outside the generated `include/shader_constants.glsl` is
    unchanged (2). Every new define lands in the generated header.
  - No bare 8-lane splat literal remains in production (#5113/#5173).
  - The physics explosion-containment caps are named constants.
  - The new ESM gates follow house style.
  - REN-D11-2026-10-05-01 (`exposure_meter.comp` hand-types S/K = 8.0) is owned by the renderer audit.
- **Dim 8**:
  - The `cargo machete` CI job is green.
  - No `#[deprecated]`, `// removed:` or `_unused`.
  - The `allow(dead_code)` increase from 27 to 34 breaks down as:
    - 3 `tree_lod_*` (deferred, #4913);
    - `loading_screen.rs:84` `ModelStage.key` (= GAME-D7-2026-10-05-02);
    - `save_io.rs:237` `PendingPlayerSaveActions::queued` (a `cfg_attr(not(test))` test-only accessor — prefer
      `#[cfg(test)]`, fold into the next touch);
    - the `dhat-heap` gate on an integration test;
    - an example `Leaf` enum.
  - Reasonless `#[allow(clippy::…)]` went from 82 to 86. Most were carried by file splits. The genuinely new ones
    are `cell_loader/water.rs:1201` and five sfmaterial example sites; style only.
  - `cxx-bridge` and `platform`: no commits and no second consumer.
- **Dim 9**:
  - The 15 net new `#[ignore]`s are all data- or env-gated.
  - The hygiene guards are live and not ignored.
  - `[features]` are unchanged and the lanes match them. Only the `not(parallel-scheduler)` branch still has no
    lane (pre-existing, noted only).
  - `golden_frames.rs` and its images are present.
  - The issue-traceability job is green again (#5085).

## Deferred

- `tree_lod_supported` / `tree_lod_archive_path` / `tree_lod_atlas_path`
  (`byroredux/src/cell_loader/object_lod.rs:795-812`, `cfg_attr(not(test), allow(dead_code))`): their consumer
  is open work in #4913 (EXAL tree LOD). They are not dead code to delete.
- `byroredux/src/loading_screen.rs:10,47` TODOs (MOD2 camera-path NIFs, ONAM/ZNAM zoom): M48 / LSCR follow-on.
  Re-triage them once they are older than 30 days.

## Next Step

`/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-10-05.md`

Label mapping:
- TD3-01..04 and TD4-01/02 → `doc-rot`.
- TD8-03 → `test-gap` (a hygiene-guard gap).
- TD4-03 and TD8-01 → `tech-debt`. They are audit infrastructure and CI; flag the CI one in the publish summary,
  since CI has no label of its own.
- The rest → `tech-debt` + `bug`. TD2-01 also takes `esm-plugin`, and TD1-01 takes `physics`.
