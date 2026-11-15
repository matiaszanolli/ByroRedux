# Tech-Debt Audit — 2026-11-15

**HEAD**: 995ed089b (Phase-1 counts taken at 49c92afbc, one commit earlier) · **Baseline**: `docs/audits/AUDIT_TECH_DEBT_2026-10-31.md` (HEAD 670f81917; 758 commits since) · **Audited**: all 9 dimensions, `--depth deep`

Run as an orchestrator with six dimension agents (Dims 1, 2, 3+4, 8+6, 7+9; Dims 5 and 6's grep half were done inline). Builds ran only in clean detached HEAD worktrees under `/mnt/data/tmp` with their own target dirs, all since removed. Another session was committing M65 stealth/detection work throughout; code that existed only in its uncommitted diff was excluded, and findings that touch the M65 files are listed under **Deferred** as well. Nothing was committed, filed or edited outside this report.

In one sentence: the 758-commit window (M39 texture residency, M43 close-out, M44 music, M47, the M-LIGHT local-fire work, the Bink/video decoders, M65 Phase 1–2) left the familiar shapes: a file across the 2000 line, a long tail of functions crossing 200 lines, single homes that new code re-spells, and docs that shipped features overtook. It also left one red CI gate from today's M65 Phase 1 commit.

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 45 |
| **Total (new)** | **47** |

By dimension: Dim 1 ×7 · Dim 2 ×12 · Dim 3 ×10 · Dim 4 ×3 · Dim 5 ×0 · Dim 6 ×1 · Dim 7 ×5 · Dim 8 ×5 · Dim 9 ×4.

- **The two MEDIUMs**
  - **TD2-01:** the Havok classic-packfile container is decoded twice, in `crates/nif` and `crates/hkx`, and each copy has been hardened against faults the other has not. Only hkx reads the v11 predicate area at `0x3E` and validates section ordering; only nif carries #4624 / #4155.
  - **TD9-01:** `parse_rate_fo4_esm`, the only guard for closed-HIGH #819, is `#[ignore]`d and missing from `.github/real-data-pins.txt`, so nothing runs it.
- **Gates**
  - **The blocking `cargo clippy --workspace --keep-going -- -D warnings` gate is red at HEAD** (TD8-01). The cause is one `chunks_exact_to_as_chunks` at `crates/plugin/src/esm/records/movt.rs:37`, from 49c92afbc (M65 Phase 1, today). This is the sixth recurrence of the class (#5308, #6222, #6582). A no-`-D` re-run of the dependents shows nothing else hidden.
  - **The non-blocking `--all-targets` lane is red too** (TD8-02, same commit). Four deny-level `approx_constant` errors at `crates/core/src/character/stance.rs:157-158` stop core's lib-test target from linting.
  - `cargo machete --with-metadata` is clean.
  - `_audit-validate.sh` is OK. Its one STALE ref was the then-uncommitted `detection.rs`, which resolves at 995ed089b.
  - README.md is at 600 lines (exactly at the cap) and ROADMAP.md at 798 (cap 800).
- **The 10-31 backlog is gone.** All nine 10-31 findings and the 10-27 set (#6520–#6527) are closed. The only open `tech-debt` issues are the #6686 bin source-scan tracker, the #6680 refactor tracker and #6371 (compute-pass teardown).
- **Partial fixes that left survivors**
  - TD8-04: ten of #5745's fifteen unreferenced `pub` items are still present.
  - TD2-05: six `PlayerEntity` reads slip past the #6362 guard's needle.
  - TD2-03: `normalize_mesh_path`'s own doc asks for a follow-up sweep that never happened, and the copies grew to nine.

### Delta vs the 2026-10-31 baseline

| Metric | 10-31 (670f81917) | 11-15 (49c92afbc) | Note |
|---|---:|---:|---|
| markers (TODO/FIXME/…) | 38 | 40 | All false positives: `XXXX`, the FO4 `HACK` FourCC, quoted upstream FIXMEs, one documented `TBD`. **Real markers: 0** (Dim 5). 0 shader hits. |
| `allow(dead_code)` | 39 | 46 | +3 since the skill's 11-14 snapshot are `cfg(test)` type-ascription guards (not debt). Four `cfg_attr(not(test), …)` items are production-dead (TD8-03). |
| `unimplemented!` / `todo!()` | 0 | 0 | The two `panic!("not …")` hits are test-only. |
| `#[ignore]` tests | 382 | 436 | Only 2 have a non-data reason, both justified. One closed-HIGH guard is unpinned (TD9-01). |
| production files >2000 LOC | 2 | 2 | `shader_constants_data.rs` 3502 (data table; 1527 code lines, not filed) and **`misc/world.rs` 2052** (TD1-01). |
| production fns >200 / >300 / >500 / >1000 (code lines) | 97 / 29 / 2 / 0 | **108 / 26 / 1 / 0** | The big drivers were split (#6708–#6710); the >200 tail grew by 11. Span counting, comparable to older reports: 207/78/24/0 → 221/79/18/0. |
| test-heavy files >2000 total LOC | 91 | 95 | |
| blocking clippy gate | red (REG-2026-10-31-02) | **red** (TD8-01, new site) | It was green between 3c9949ec9 (11-14) and 49c92afbc (11-15). |
| `--all-targets` clippy lane | n/a | **red** (TD8-02) | |
| `cargo machete` | clean | clean | |

## Baseline Snapshot (Phase 1, measured at 49c92afbc)

```
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 40
allow(dead_code):      46
unimplemented!/todo!(): 0
#[ignore] tests:        436
files >2000 production LOC: 2
test files >2000 total LOC (lower priority, separate bucket): 95
```

`prod_loc_self_test`: ok. The function-size scanner (`fnscan.py`, brace-matching, comments and strings stripped, `cfg(test)` skipped) reproduces the 10-31 report's 207/78/24/0 exactly on the 670f81917 tree, and the `render_one_frame` pin (494 code lines) exactly at HEAD.

Watch list (production LOC; skill snapshot 11-14 → HEAD):
- `byroredux/src/app_events.rs`: 1994 → 1994. Six lines from the band. Axis: bench reporting and telemetry go to `app_telemetry.rs`.
- `byroredux/src/app_step.rs`: 1899 → **1935**. Axis: the bench-camera driver goes to `app_step/bench.rs`.
- `asset_provider/material/merge.rs` (1908), `texture_registry/mod.rs` (1894), `vulkan/volumetrics.rs` (1893), `build_and_upload_instances.rs` (1859), `nif/import/material/mod.rs` (1859), `app_frame.rs` (1835) and `hkx/src/behavior.rs` (1826) are all flat.
- **New to the list:** `byroredux/src/systems/character.rs` (1842; axis: player water, swim and breath go to `character/water.rs`) and `crates/physics/src/world/mod.rs` (1807; axis: forces and impulses go to `world/forces.rs`).

## Top 10 Quick Wins

| # | ID | Effort | What |
|---|---|---|---|
| 1 | TD8-01 | trivial | Turn the blocking clippy gate green: `movt.rs:37` → `as_chunks::<4>()`. |
| 2 | TD8-02 | trivial | `#[allow(clippy::approx_constant)]` with a reason on the Skyrim SPED table in `core/character/stance.rs`; struct-update syntax in `scripting/src/stance.rs:122`. |
| 3 | TD9-01 | trivial | Append `plugin-esm parse_rate_fo4_esm` to `.github/real-data-pins.txt` (**MEDIUM**). |
| 4 | TD3-01 | trivial | Fix `CLAUDE.md:45`, `game-loop.md:38` and the `scene.rs:505` comment: `--radius` is clamped 1..=12 with default 12. |
| 5 | TD3-02 | trivial | `shader-pipeline.md:490`, `coordinate-system.md:321` and the DLOD PRD still describe conventional depth; reversed-Z shipped 11-07. |
| 6 | TD3-03 | trivial | Add the Set 0 binding 2 `TextureFirstLevels` row to the `shader-pipeline.md` binding table. |
| 7 | TD9-03 | trivial | The `parallel-scheduler`-off CI step: `cargo check` → `cargo test` (916 pass today). |
| 8 | TD2-05 | trivial | Six hand-spelled `PlayerEntity` reads → `PlayerEntity::get` / `::is`, and widen the #6362 guard's needle. |
| 9 | TD8-03 | trivial | Gate four production-dead LOD items under `#[cfg(test)]` and drop their `cfg_attr` allows. |
| 10 | TD2-10 | trivial | Two groundcover files re-spell `descriptors::memory_barrier`. |

## Top 5 Medium Investments

| # | ID | Effort | What |
|---|---|---|---|
| 1 | TD2-01 | medium | One Havok packfile container decoder (hkx's), with nif's `HavokPackfile` as a thin adapter (**MEDIUM**). |
| 2 | TD1-01 | medium | Split `misc/world.rs` into a `world/` directory by record family (navigation / region / lighting / activator_terminal / keyword). Watch the `include_str!` guard at `records/tests.rs:3303`. |
| 3 | TD2-02 | medium | `lod_support::load_candidates`: one load loop for the five LOD ring reconcilers, the load-side twin of `retain_resident`. |
| 4 | TD1-03 | medium | Stage functions for `volumetrics/init.rs::new_inner` (614 code lines, the largest function in the repo; a regression of #2256) and `composite.rs::new_inner`, following the #6143 `init_stages.rs` precedent. |
| 5 | TD1-02 | small | Split `flush_upload_batch` (157 → 369 code lines in M39) into its four phases and pin it, before the next residency change lands. |

## Findings

### MEDIUM

#### TD2-2026-11-15-01: The Havok classic-packfile container is decoded twice, in `nif` and in `hkx`, with different hardening in each
- **Dimension**: 2 · **Severity**: MEDIUM (duplicated logic, divergent bug-fix history) · **Effort**: medium · **Labels**: `tech-debt` · **Existing**: none
- **Age**: `crates/hkx/src/packfile.rs` 02c24e4f1 (2026-08-01); `crates/nif/src/blocks/collision/havok_packfile.rs` 83565a9b0 (2026-08-31, #3809)
- **Location**: `crates/nif/src/blocks/collision/havok_packfile.rs:60-62` (constants), `:320-` (`read_local_fixups`, `read_global_fixups`, `read_virtual_fixups`), `parse_havok_packfile` (`:382-`); `crates/hkx/src/packfile.rs:1-250` (`Packfile::parse`)
- **Description**: Both parse the same container: magic pair `0x57e0e057 / 0x10c0c010`, header fields at `0x0C` (version), `0x10..0x13` (layout rules), `0x14` (section count), `0x28` (contents-version string), the section table (19-byte name, then seven `u32` offsets), the three fixup tables (8-byte local, 12-byte global, 12-byte virtual, each ending at a `0xFFFFFFFF` sentinel), and the `__classnames__` names. The two have drifted apart, one fault class at a time:
  - **hkx handles v11's predicate area**: `table_start = 0x40 + u16 @ 0x3E` (`packfile.rs:100-114`; FO4 animation clips carry a 0x10-byte one, found under #5901). nif hard-codes `SECTION_TABLE_START = 0x40` and never reads `0x3E` (`grep 0x3E` in `havok_packfile.rs` is empty). A v11 blob with a predicate area would read its section table 0x10 bytes early. The 25 sampled FO4 `bhkSystemBinary` blobs have none, so it is latent.
  - **hkx validates section ordering** (`start <= local <= global <= virtual <= exports <= end <= len`) and range-checks each global fixup's target section and each virtual fixup's class section (`:134-197`). nif does none of these; it clamps reads with `end.min(data.len())` and keeps the raw offsets.
  - **nif fixed `Vec::with_capacity(num_sections)` from an on-disk `u32` (#4624) and `u32 + u32` overflow in `absolute_end` (#4155)**. hkx has its own cap (`section_count > 64`) and checked adds, found independently.
  - **hkx fixed the quadratic class-name string materialisation (#4648/#4649)**: it stores `(source, name_start, name_len)` and compares in place. nif's `class_names: Vec<String>` is per-record, so it is not quadratic, but it is a different representation of the same table.
  - Each decoder carries its own `read_u32` / `read_cstr` pair (`nif havok_packfile.rs:263,268`, `hkx packfile.rs:373,388`).
- **Suggested Fix**: one container decoder. hkx already has the more strictly validated one, and it has no dependency on nif, so the lower-risk direction is `crates/nif` depending on `byroredux-hkx`. Expose a public `hkx::container::{PackfileHeader, Section, ClassNames}` view (header, section table with the validated offsets, the three fixup tables, the `__classnames__` list with name offsets) from the existing `Packfile::parse`, and rebuild `HavokPackfile` / `PackfileSection` in `nif` as a thin adapter over it. Keep nif's `is_havok_tagfile_error` (the `TAG0` check, #6302) in nif, since it is the caller-side gate. The existing synthetic-blob tests in `havok_packfile.rs` and the corpus example `crates/nif/examples/havok_blob_recon.rs` become the equivalence check. `np_ragdoll.rs:309` is the only production caller.

#### TD9-2026-11-15-01: `parse_rate_fo4_esm`, the only guard for closed-HIGH #819 (and #817's five categories), is `#[ignore]`d, in no pin lane and in no workflow
- **Severity**: MEDIUM (skill trigger: an `#[ignore]`d test that guards a fix from a closed HIGH issue)
- **Dimension**: 9 (test hygiene)
- **Location**: `crates/plugin/tests/parse_real_esm.rs:3361-3363` (test); absent from `.github/real-data-pins.txt` (70 rows, none names it); the lane that would run it is `plugin-esm` (`scripts/run-real-data-pins.sh`, `-p byroredux-plugin --test parse_real_esm`)
- **Age**: closed 2026-05-04 (#819); the pin list's last additions were #6354/#6460/#6554
- **Effort**: trivial
- **Labels**: `tech-debt`, `test-gap`
- **Status**: NEW (same class as closed #6224 and #6354; those added other rows)
- **Description**: `.github/real-data-pins.txt` is the only list the strict real-data runner executes for `parse_real_esm.rs` (`real-data-gates.yml` states the plugin crate's whole-suite run stays out of the lane deliberately, because one process loading every master spikes past 20 GB). Its siblings are pinned (`parse_rate_oblivion_esm`, `parse_rate_fnv_pack_pldt_location`), but the FO4 parse-rate floor is not. The corpus job's filters (`fallout_4 fo4_all_meshes`) run only inside `-p byroredux-nif`, so they do not reach this plugin test. The test's doc comment says what it guards: "Closes #819 ... Floors specifically lock in the 5 FO4-architecture categories that #817 added to `EsmIndex::categories()` (texture_sets / scols / packins / movables / material_swaps)". #819 is labelled `high` and closed; the test is the fix. `parse_rate_fnv_esm` / `parse_rate_fo3_esm` are also unpinned, but cite no closed issue and are not claimed here.
- **Evidence**:
  ```
  parse_real_esm.rs:3361  #[test]
  parse_real_esm.rs:3362  #[ignore = "needs FO4 game data on disk"]
  parse_real_esm.rs:3363  fn parse_rate_fo4_esm() {
  grep -c parse_rate_fo4_esm .github/real-data-pins.txt   -> 0
  ```
  Weaker siblings with the same absence (each also has synthetic or other coverage, so not claimed at MEDIUM): `dlccoast_header_classifies_as_fallout4` (`parse_real_esm.rs:111`, #2904 HIGH; a synthetic `GameKind::from_header(Tes5Plus, 0.95, 131)` check exists at `reader.rs:1937`); `real_skyrim_load_order_preserves_categories_and_resolves_archive_strings` (`byroredux/src/cell_loader/load_order.rs:1388`, #2907 + #2912 HIGH; synthetic localization tests sit beside it); `lod_descriptor_tiles_the_shipped_quad_names` (`byroredux/src/cell_loader/lod_support.rs:1104`, #2586 HIGH, "reopened" on the real corpus; synthetic origin tests `:981-1085` exist).
- **Impact**: a regression that wipes an FO4 category (the #817 shape: SCOL/PKIN/MSWP/TXST maps dropping) trips nothing in CI and in no scheduled lane; it surfaces only if someone runs the ignored test by hand.
- **Related**: #819, #817, #5799, #6224, #6354, #6183 (the `every_real_data_pin_names_an_ignored_test_in_its_lane` guard would accept the new row).
- **Suggested Fix**: append `plugin-esm parse_rate_fo4_esm` to `.github/real-data-pins.txt` and run `scripts/run-real-data-pins.sh parse_rate_fo4_esm`; add the three weaker rows under `plugin-esm` / `bin` at the same time if the maintainer wants the real-corpus legs.

### LOW

#### Dimension 1: File / Function / Module Complexity

##### TD1-2026-11-15-01: `crates/plugin/src/esm/records/misc/world.rs` is 2052 production LOC (1910 on 10-31); eight record families, split by family
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Age**: 1910 at 670f81917 (2026-10-31), 2019 on 11-09, 2052 now. The growth is 12 commits. The main ones are 3312c84a4 and 3bfcbd58b (FO4 / FO76 fog tail, god-ray records, 11-08), cf3ccc958 (`covers_cell`, 11-07), 8774d1adb, cf901d557 and 6439145f9 (REGN RDMP / RDMD / lstring, 11-08 and 11-15), and 7af55a0df (XCT).
- **Effort**: medium (mechanical; the cost is the source-scan and citation hazards)
- **Labels**: `tech-debt`
- **Location**: `crates/plugin/src/esm/records/misc/world.rs:1-2050` (production); inline tests `:2051-3825`
- **Status**: NEW (no issue in the 1000-issue tech-debt set names `world.rs`)
- **Description**: The module doc says "navigation, regions, encounter zones, lighting templates, image-space adapters, activators, terminals". The file really holds eight independent record families that share only two row helpers (`rows`, `rows_exact`, `:981`/`:992`) and `common` imports. There is no shared state between them. The tests (1774 lines) are already grouped by family in three modules. The sibling files in `misc/` are each one family: `water.rs`, `quest.rs`, `scene.rs`, `music.rs`, `imagespace.rs` (that one is `IMAD`, not `IMGS`).
- **Evidence** (production line ranges at HEAD):

  | Family | Items | Lines | ~LOC |
  |---|---|---|---:|
  | `NAVI` / `NAVM` | `NaviRecord`, `NavmRecord`, `decode_nvnm`, `decode_nv*_row` | 18-660 | 640 |
  | `REGN` | `RegionData*`, `RegionArea`, `RegnRecord`, `select_active_region_sound`, `parse_regn`, `apply_region_payload` | 661-1288 | 630 |
  | `ECZN` / `CNDF` | | 1289-1391 | 100 |
  | `KYWD` / `AORU` | | 1392-1469 | 80 |
  | `LGTM` / `IMGS` | | 1470-1725 | 255 |
  | `ACTI` / `TERM` | | 1726-2050 | 325 |

  Inline tests: `tests` `:2052` (26 tests: LGTM / IMGS 11, ACTI / TERM 10, rest NAVI / ECZN / REGN), `regn_tests` `:2840` (439 lines), `navm_tests` `:3279` (548 lines).
- **Impact**: Every `REGN` or fog / god-ray change (the three busiest areas of the last three weeks) recompiles and re-reviews a file that also carries the navmesh and terminal decoders. 12 commits in 15 days touched it.
- **Suggested Fix**: Turn `world.rs` into a `world/` directory. `world/mod.rs` is a ~60-LOC hub holding the module doc, `pub use` of the same names `misc.rs` already re-exports (so `misc.rs:118` and every caller are unchanged), and `rows` / `rows_exact` as `pub(super)`.
  - Children, each taking its tests with it:
    - `navigation.rs`: NAVI / NAVM + `navm_tests`, ~640.
    - `region.rs`: REGN + ECZN + `regn_tests`, ~730. This is the largest child, with the `regn_tests` module beside it.
    - `lighting.rs`: LGTM + IMGS, ~255.
    - `activator_terminal.rs`: ACTI + TERM, ~325.
    - `keyword.rs`: KYWD + AORU + CNDF, ~180.
  - Hazards:
    - (a) `crates/plugin/src/esm/records/tests.rs:3303` runs `include_str!("misc/world.rs")` in `remap_fid_has_exactly_one_definition`. It must list the child files or the guard goes vacuous (*feedback_file_split_include_str*).
    - (b) Cite-by-line references need a sweep: `.claude/commands/audit-esm/SKILL.md:147,343,359`, `docs/prd/game-music.md:184`, `volumetric-lighting.md:155`, `procedural-exteriors.md:162,562`, `cross-cell-actor-travel.md:130`.
    - (c) The `//!` header already omits KYWD / AORU / CNDF and should be fixed in the same commit.
  - Do not split by name or by game. `world.rs` carries no per-game branches.
- **Related**: #4219 and #6145 (earlier barrel splits, precedent), `misc/pack.rs` (1578), `misc/water.rs` (1499) and `misc/magic.rs` (1390), which are under the line.

##### TD1-2026-11-15-02: `TextureRegistry::flush_upload_batch` grew 157 to 369 code lines (+212) in the M39 residency work
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Age**: all M39 commits, 494d077b2 (2026-11-03) through f754eee28 (2026-11-14). Other contributors: 6a4c661a8 (#6738), b8d89bdcb (#6737), b34b897df (#6624), dd2cd84e9 and 9179059a4 (11-04).
- **Effort**: small (verbatim extraction, no barrier or order change)
- **Labels**: `tech-debt`
- **Location**: `crates/renderer/src/texture_registry/upload.rs:1253-1710` (369 code lines, 457 span, nesting 5)
- **Status**: NEW. It is the largest organic growth of any function since 670f81917 and has no size pin.
- **Description**: One function does four phases in sequence:
  1. Declare `StagedUpload` and run the one-time-command closure that records every copy (~265 lines of the span; parse, staging, residency-change staging, floor-first slices, cube faces).
  2. Handle a failed submission. This is the `may_be_in_flight` leak-or-destroy branch of #4891, and it is the part you least want to re-read in a 457-line body.
  3. Install staged textures (the `for s in staged` loop, ~75 lines, #6738 refusal handling).
  4. Record the floor sources and re-parse their DDS headers (`floor_sources` loop).
- **Evidence**: No size pin covers it (the renderer pin list in `crates/renderer/src/vulkan/context/function_size_pin_tests.rs` is context-only). My scan gives 157 code lines at 670f81917 against 369 at HEAD, same file and name.
- **Impact**: The newest and still-moving residency code sits in the function with the most failure-path subtlety (leak versus destroy decided by `OneTimeCommandError::may_be_in_flight`). The next M39 change adds to it.
- **Suggested Fix**: Extract the three post-submit phases and the recording closure body as `&mut self` methods in the same file: `record_batch_uploads(..) -> (staged, failed_changes)`, `discard_failed_batch(staged, e)`, `install_staged(staged)` and `record_floor_sources(..)`. Move the code verbatim in the same order. Add a `flush_upload_batch` line to `crates/renderer/src/vulkan/context/function_size_pin_tests.rs` (or a texture-registry sibling) so it cannot regrow. Read *feedback_speculative_vulkan_fixes* first: this is an extraction only, no behaviour change.
- **Related**: #6143 (closed; same extraction shape: #6708, #6709, #6710).

##### TD1-2026-11-15-03: Vulkan constructors `volumetrics/init.rs::new_inner` (614 code lines, largest function in the repo) and `composite.rs::new_inner` (434, +46) have no budget
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Age**: volumetrics 594 at 670f81917 to 614 now (d3e3acc7b, 626197ae5, 2f3e16891, 684bc9161 and 0e55ec32b added the combustion and local-fire stages). composite 388 to 434 (0e55ec32b, local-fire bindings).
- **Effort**: medium (construction only, no recording)
- **Labels**: `tech-debt`
- **Location**: `crates/renderer/src/vulkan/volumetrics/init.rs:167-880` and `crates/renderer/src/vulkan/composite.rs:582-1190`
- **Status**: NEW for composite. For volumetrics, **regression of #2256** (TD1-078, "volumetrics.rs crossed 2000 LOC, plus a 556-LOC constructor"; the file was split but the constructor was left at 556 span lines and is now 714 span / 614 code).
- **Description**: `volumetrics::new_inner` builds, in one body: the six per-frame froxel volumes (`for i in 0..MAX_FRAMES_IN_FLIGHT`, `:238`), three samplers (`:373-401`), the params buffers (`:418`), the inject layout / pool / sets / pipeline (`:528-690`), the integrate params / layout / pool / pipeline (`:695-800`), and the combustion stage after it. All of it is guarded by the `partial` + `try_or_cleanup!` rollback pattern. The stages are already commented and separate; they only lack function boundaries. `composite::new_inner` has the same shape.
- **Impact**: This is the only >500 code-line function left (the other 494-line driver is pinned). The three stage-1 volumetrics commits of 10-31 to 11-07 each grew it.
- **Suggested Fix**: Follow the precedent of `context/init_stages.rs` (#6143): one `build_<stage>(&mut partial, ..) -> Result<()>` per stage (`create_volumes`, `create_samplers`, `build_inject_stage`, `build_integrate_stage`, `build_combustion_stage`), with `new_inner` as the sequencing driver, each stage keeping its `try_or_cleanup!` rollback. Pin the driver in `function_size_pin_tests.rs`. Construction only; no barrier or order change. Verify the composite constructor's rollback shape before applying the same split to it.
- **Related**: #2256, #6143, #6371 (compute-pass teardown tracker).

##### TD1-2026-11-15-04: `advance_runtime_unit` is a 383-line, 8-deep nine-phase state machine in one `match`
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (fn >200, nesting >5)
- **Age**: unchanged since #5091 (2026-10-01) and last edited 2026-10-28 (477124477). It was never named by a tech-debt report.
- **Effort**: small
- **Labels**: `tech-debt`
- **Location**: `byroredux/src/npc_spawn/resumable/runtime.rs:642-1030`
- **Status**: NEW
- **Description**: `match state.phase` has nine arms (`Skeleton` `:656`, `Body(i)` `:707`, `Head` `:792`, `Hair` `:807`, `Brow` `:841`, `Eye(i)` `:860`, `HeadSubPart(i)` `:909`, `Armor(i)` `:935`, `Finalize` `:959`). Two of them (`Skeleton` ~50 lines, `Body` ~85) are large; the deepest nesting I saw is inside the `Body` arm (`:818`, 36-column indent).
- **Impact**: Every new NPC-assembly stage or per-game race rule lands in this one function.
- **Suggested Fix**: One `fn advance_<phase>(state, world, ctx, ..) -> UnitOutcome` per arm in the same file (or `resumable/phases.rs`); `advance_runtime_unit` becomes the 20-line dispatcher. The step-per-frame resumable contract and the per-phase `UnitOutcome` return are unchanged. Pin the dispatcher (`scheduler_access_tests.rs` already pins `advance_actor_synth` at 151).

##### TD1-2026-11-15-05: `crates/sfmaterial` ships about 520 lines of test fixtures in the production library (`pub mod test_support`, incl. a 291-line fixture function)
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (cross-dim 9: test hygiene)
- **Age**: introduced by #3398; `synthetic_cdb_chunks_with` has been extended for #5277, #5283 and #5673.
- **Effort**: small
- **Labels**: `tech-debt`, `test-gap`
- **Location**: `crates/sfmaterial/src/index.rs:935-1458` (`pub mod test_support`, `#[doc(hidden)]`), `synthetic_cdb_chunks_with` at `:1066`
- **Status**: NEW (a crate no tech-debt report has named)
- **Description**: The module is intentionally public because `byroredux/src/asset_provider/tests/starfield_mat.rs:42,51` uses it cross-crate. It is not gated by a feature, so it compiles into every build of the crate. It is 35% of `index.rs`'s 1458 production LOC, and it contains the 291-line fixture function.
- **Evidence**: `crates/sfmaterial/Cargo.toml` has no `[features]`; the module has only `#[doc(hidden)]`. `prod_loc` reports 1458 for a file of 1934 total lines, and the test module starts at `:1459`.
- **Impact**: It inflates the production LOC figure for the crate and makes `index.rs` the second-largest file in a crate that has no other debt. A fixture-only change rebuilds the shipping library, and the byte-assembly helpers can be mistaken for production decoders.
- **Suggested Fix**: Add `[features] test-support = []`. Gate the module with `#[cfg(any(test, feature = "test-support"))]`. Enable the feature from `byroredux`'s `[dev-dependencies]` entry for `byroredux-sfmaterial`. Optionally move the module into `src/test_support.rs`. The `lib.rs:77` re-export gets the same gate.

##### TD1-2026-11-15-06: `crates/ui/src/avm2_host.rs::build_adapter_abc` is a single 375-line builder (pool construction and three method bodies in one function)
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Age**: not measured; a crate no tech-debt report has named
- **Effort**: small
- **Labels**: `tech-debt`, `ui`
- **Location**: `crates/ui/src/avm2_host.rs:788-1163`
- **Status**: NEW
- **Description**: The function builds the three ABC constant pools (`strings`, `namespaces`, `multinames`, ~110 lines; the #2724 / #2725 derive-never-transcribe rule makes every index a returned value), then three method bodies (ready, installer, init, each a `methods` / `method_bodies` / `traits` triple), plus the optional destroy registration. The pools feed the bodies by many `Index` locals, which is why it was never split.
- **Impact**: This is the one place that assembles the bytecode adapter injected into Fallout 4 Scaleform menu movies (the module header says so). A new lifecycle hook means finding where in 375 lines to add a pool entry and a body.
- **Suggested Fix**: Two steps. First, a `struct AdapterPools { strings, namespaces, multinames, .. }` built by `fn build_adapter_pools(object, has_destroy_trait) -> (AdapterPools, AdapterNames)`. Second, `fn ready_method(..)`, `fn installer_method(..)` and `fn init_method(..)` that each append to `methods` / `method_bodies` / `traits` and return their indexes. `build_adapter_abc` becomes the ~40-line sequencer. Run the existing `avm2_host` tests to confirm the emitted ABC is unchanged (not verified here that they pin the bytes).
- **Related**: #2724, #2725, #2963.

##### TD1-2026-11-15-07: Two never-swept tools carry a deep-nesting function and a 296-line print match
- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (nesting >5, fn >200)
- **Effort**: trivial (`draw_gpu`) + small (`print_response`)
- **Labels**: `tech-debt`
- **Status**: NEW
- **Sites**:
  - `tools/byro-launcher/src/settings_screen.rs:125-170` `draw_gpu` (47 code lines, brace-depth 9 as measured; about 8 constructs deep: frame closure, `match &state.gpu`, `Ok`, `match caps.verdict()`, `Ok(())`, `if !already`, `if let`, `horizontal` closure, `if clicked`).
  - `tools/byro-dbg/src/display.rs:5-` `print_response` (296 code lines, depth 5): one `match` over every `DebugResponse` variant with the printing inline.
- **Description**:
  - `draw_gpu` is the launcher's only function with this depth. The GPU-verdict logic (floor warning, recommendation, blocker colouring) is mixed with egui layout.
  - `print_response` is a transport enum printer that grows by one arm of ~20 lines per new debug response.
- **Suggested Fix**:
  - `draw_gpu`: extract `fn recommended_preset_row(ui, state, caps)` and `fn blocker_label(ui, blocker)`. The two `Err(blocker)` arms are already the same call.
  - `print_response`: one `fn print_<variant>(..)` per variant over 10 lines (`print_stats`, `print_entity_list`, ...), the `match` becoming a one-line dispatch table. `byro-dbg` is a pure CLI formatter, so the response-formatting tests are the check.

#### Dimension 2: Logic Duplication

##### TD2-2026-11-15-02: Five LOD ring reconcilers each carry the same "admission gate, budget, attempt, classify, complete" loop; #6092 had to edit four of them identically and the fifth was copied from them
- **Dimension**: 2 · **Severity**: LOW · **Effort**: medium · **Labels**: `tech-debt` · **Existing**: none (#5820 consolidated only the unload half, `retain_resident`)
- **Age**: #6092 635330d08 (2026-10-23) touched four copies; `tree_lod_legacy.rs` 510e7dc4d / 8e7624256 (2026-11-08) is the fifth
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:731-844` (`stream_lod_blocks`), `object_lod.rs:327-377` (`stream_object_lod_blocks`), `placement_lod.rs:406-447` (`stream_placement_lod_blocks`), `tree_lod.rs:594-631` (`stream_tree_lod_blocks`), `tree_lod_legacy.rs:463-495` (`stream_legacy_tree_lod_blocks`)
- **Description**: After `retain_resident` unloads, every ring runs the same shell: filter `candidates` not yet resident; `if !ctx.mesh_registry.<tier>_geometry_admission_open() { budget.note_held_back(remaining); starved = true; break }`; `if !budget.try_take() { break }`; call a spawn function; on `None`, `match classify_lod_attempt(false, <admission again>) { Miss => insert sentinel, HeldBack => { budget.note_held_back(remaining + 1); starved = true; break }, Keep => unreachable!() }`; then `complete = starved || <all attempted>`. The doc on `retain_resident` (`lod_support.rs:485`) says it exists so "one body serves all four rings" and the leak-fix class lands in one place; the load half has the opposite property. Two spellings of the bookkeeping already exist: terrain/object/placement count `attempted` and test `attempted == candidate_count` (remaining = `candidate_count - attempted (+1)`), while tree and legacy-tree test `candidates.iter().all(|k| ring.blocks.contains_key(k))` and use the loop index. They are equivalent today. The `+1` in the held-back arm is exactly the kind of off-by-one that gets fixed in one copy.
- **Suggested Fix**: add `lod_support::load_candidates<K: Copy, B>(candidates: &[K], budget: &mut LodWorkBudget, admission_open: impl Fn() -> bool, mut spawn: impl FnMut(K) -> Option<B>, mut sentinel: impl FnMut(K) -> B, mut insert: impl FnMut(K, B)) -> LoadOutcome { attempted, starved }` beside `retain_resident` and `classify_lod_attempt`, and migrate the five callers (each keeps its own spawn call and per-ring log line). `streaming_tests.rs:977-980` text-pins the admission call in four of the five files (terrain, object, placement, tree); re-point it at the helper's call sites in the same commit.

##### TD2-2026-11-15-03: "Prepend `meshes\` unless present" has a single home (`normalize_mesh_path`) and nine hand-spelled copies, five behaviours
- **Dimension**: 2 · **Severity**: LOW · **Effort**: small · **Labels**: `tech-debt` · **Existing**: none
- **Age**: `mesh_path` ×3 df6ad0410 / 3bb6806b5 (2026-10-14); `kf_mesh_path` d1b298921 (2026-10-18); others older
- **Location**: the single home is `byroredux/src/asset_provider/archive.rs:695` (`pub fn normalize_mesh_path`, case-insensitive on either separator, and exempts `geometries\` per #1292). The copies:
  - `byroredux/src/asset_provider/object_graph.rs:67`, `archetype_idle.rs:62`, `actor_graph.rs:69` (`fn mesh_path`; the second and third are character-for-character equal, the first differs only by not lower-casing)
  - `byroredux/src/asset_provider/animation.rs:207` (`kf_mesh_path`)
  - `byroredux/src/cell_loader/placement_lod.rs:266` (`full_model_path`), `nif_import_registry.rs:49` (`canonical_model_path_key`), `tree_lod_legacy.rs:264` (`lod_flat_nif_path`), `references/import.rs:358` (`meshes_rooted` closure), `references/synth_child.rs:640` (inline `starts_with("meshes\\") || ...("meshes/")` chain)
  - `project_directory` is also byte-identical in `archetype_idle.rs:74` and `actor_graph.rs:78`.
- **Description**: the helper's own doc says "the cell-loader form is now a redundant idempotent double-normalisation and can be removed in a follow-up sweep"; that sweep never happened, and the copies kept multiplying. They disagree on lower-casing, on `/` versus `\` handling, on whether a leading separator is trimmed, and none carries the #1292 `geometries\` exemption that the home needed (Starfield content-addressed `.mesh` companions, where a `meshes\` prefix made 99.7% of Cydonia statics miss). I found no `geometries\` input that reaches any copy today, so I rate it LOW; it meets the letter of the promotion trigger (one copy fixed, the others not) and should be promoted if such an input is found.
- **Suggested Fix**: replace the nine bodies with `normalize_mesh_path(..)` (plus `.to_ascii_lowercase()` / `replace('/', "\\")` at the three sites that need a lower-cased key, via one `canonical_mesh_key(path) -> String` next to it), move `project_directory` into `asset_provider/` once (it needs `mesh_path`'s prefix rule too), and delete the five private copies. Add a `workspace_hygiene_tests` scan for `format!("meshes\\{` outside `archive.rs`.

##### TD2-2026-11-15-04: `consume_list` / `consume_map` re-inline the count check that `checked_container_count` already holds; `consume_object` / `skip_object` / `stream_db_file_index` triplicate the chunk-kind match
- **Dimension**: 2 · **Severity**: LOW · **Effort**: small · **Labels**: `tech-debt` · **Existing**: none
- **Age**: `checked_container_count` 9d94875e5 (2026-09-17); read/skip split drift fixed by #5323 (258ba2b90, 2026-10-05)
- **Location**: `crates/sfmaterial/src/reader.rs:586` (`consume_object`), `:628` (`consume_list`), `:671` (`consume_map`), `:697` (`skip_object`), `:728` (`skip_list`), `:743` (`skip_map`), `:760` (`checked_container_count`); `crates/sfmaterial/src/index.rs:741` (`stream_db_file_index`)
- **Description**: three things.
  - `skip_list` and `skip_map` call `checked_container_count(raw, payload.len(), what)?`. `consume_list` and `consume_map` spell the same `usize::try_from(raw).map_err(NegativeCount).min(payload.len())` inline, with a 25-line #2623 comment on the first and a "same rationale" pointer on the second.
  - The `(is_cast, is_diff)` match over `ChunkType::{Objt, User, Diff, Usrd}`, the `WrongChunkType` fallback, the `consume_chunk` / `Cursor` open, the cast `target_ref` prefix, the trailing cast `u32` and the trailing-bytes check are copied between `consume_object` and `skip_object`. `stream_db_file_index` carries the third copy, deliberately narrowed (it rejects DIFF/USRD since #5320).
  - History: #5323 was exactly a read/skip drift (the read path moved to `Class::read_order`, the skip path did not, so a reordered class desynced); the fix added a byte-exact skip-vs-read equivalence test rather than removing the second copy.
- **Suggested Fix**: `State::open_object(&mut self) -> Result<ObjectChunk { payload, cur, is_cast, is_diff }>` and `ObjectChunk::finish(self) -> Result<()>` (cast trailer plus the `ObjectTrailingBytes` check), used by all three; `consume_list` / `consume_map` call `checked_container_count`. `stream_db_file_index` keeps its DIFF/USRD refusal as a pre-check on `peek_kind()` before calling `open_object`.

##### TD2-2026-11-15-05: Six production reads of the player entity bypass `PlayerEntity::get`, and the #6362 guard cannot see them
- **Dimension**: 2 · **Severity**: LOW · **Effort**: trivial · **Labels**: `tech-debt` · **Existing**: #6362 (CLOSED; this is a regression of the "one read" rule plus a guard blind spot)
- **Age**: 65adad604 (2026-05-21, character.rs), fcd1cce4a (2026-10-24, find_package), dbe562c1f (2026-10-12, combat.rs); the guard landed 67722b6e4 (2026-10-27)
- **Location**: `byroredux/src/combat.rs:708` (`note_player_damage`), `byroredux/src/systems/character.rs:228` and `:773` (two five-line `let Some(player_res) = try_resource::<PlayerEntity>() else {return}; let Some(e) = player_res.0 else {return}; drop(player_res)` ladders), `byroredux/src/systems/find_package.rs:111` (`is_actor`) and `:483` (`.is_some_and(|p| p.0 == Some(target))`), `byroredux/src/cell_loader/unload.rs:57` (`player_owned_entities`)
- **Description**: #6362 replaced 66 hand-spelled reads with `PlayerEntity::get` (`components/player.rs:28`) and added `no_source_hand_spells_the_player_entity_read` (`byroredux/src/workspace_hygiene_tests.rs:1191`). The guard's needle is the whitespace-stripped text `PlayerEntity>().and_then(`, so it matches only the `.and_then(|p| p.0)` spelling. The six sites above use the `let Some(res) = ...; res.0` ladder or `.is_some_and(|p| p.0 == Some(x))`, so they slipped through. They are the "two resources that differ in FlyCam are only safe to reason about if each has exactly one read" case the guard's doc names.
- **Suggested Fix**: `PlayerEntity::get(world)` at the four ladder sites, and a `PlayerEntity::is(world, entity) -> bool` (`Self::get(world) == Some(entity)`) for the two compare sites. Widen the guard needle to `try_resource::<` followed by a path ending in `PlayerEntity>()` outside `components/player.rs` (excluding the `Papyrus` prefix), and keep `PapyrusPlayerEntity` out of it.

##### TD2-2026-11-15-06: `Movie::open` / `open_bink2` and `extract_audio` / `bink2_audio` are two copies of one pipeline; this week's three video fixes each had to touch both
- **Dimension**: 2 · **Severity**: LOW · **Effort**: small · **Labels**: `tech-debt` · **Existing**: none
- **Age**: c678f1065 (2026-11-13, the Bink 2 arm copied the Bink 1 arm); fixes 118afd645 (11-14), aa193bd9e (11-14), 706f95884 and 538992417 (11-15)
- **Location**: `crates/video/src/lib.rs:393-442` (`open`) vs `:444-483` (`open_bink2`); `:199-229` (`extract_audio`) vs `:259-299` (`bink2_audio`)
- **Description**: both open paths do: derive `name` from the file name (the same `path.file_name().map_or_else(..)` line), clamp `(width, height) = size.max(2)`, build the `sync_channel(FRAME_RING)`, spawn the `"movie-decode"` thread, run the body under `guard_decoder`, and on a frame error `log::warn!("video: {reason}")` + `tx.send(Err(reason))` + `return`, then `tx.send(Ok(fit(..)))` or return when the receiver is gone, and finally build the identical eight-field `Self { .. }` literal (also built a third time in `from_decoded`). Both audio paths: open a decoder (warning on failure), compute `pcm_cap`, loop frames, stop on decode error with the same "its audio stops at frame N" warning, call `over_pcm_cap`, then `write_wav`. They already differ without a stated reason: the Bink 2 loop refuses a packet before `decode` grows the buffer (`:271-274`, added by 538992417), the Bink 1 loop extends first and truncates after; the Bink 1 warnings omit the movie `name` that the Bink 2 ones carry. The test `every_reader_thread_runs_under_the_decoder_guard` (`:654`) exists because each new reader thread had to be remembered.
- **Suggested Fix**: `fn spawn_reader(name: String, size: (u32, u32), decode: impl FnOnce(&mut dyn FnMut(Result<Vec<u8>, String>) -> bool) + Send + 'static) -> Result<Movie, VideoError>` taking the per-codec frame producer (the `emit` closure returns `false` when the receiver dropped), and one `Movie::assemble(info, size, rx, reader, audio)`. For audio, one `fn soundtrack(name, cap, next_chunk: impl FnMut(&mut Vec<i16>) -> Step)`; the Bink 1 and Bink 2 arms supply only "decode this frame's packet". Do it before the next codec arm (BIK-DEC-004 and later work add more).

##### TD2-2026-11-15-07: Three raw-`Allocation` readback paths re-spell `GpuBuffer::invalidate_if_needed` and lack its #2683 range guard
- **Dimension**: 2 · **Severity**: LOW · **Effort**: small · **Labels**: `tech-debt` · **Existing**: none
- **Age**: screenshot ddd05a9df (2026-04-13) + #2751/#2752 (be39814d9); depth_capture 283b71824 (2026-08-29); `linear_capture.rs` 75b4024df (2026-11-02) is the third copy
- **Location**: `crates/renderer/src/vulkan/context/screenshot.rs:47-72`, `depth_capture.rs:55-80`, `linear_capture.rs:150-172`; the home is `GpuBuffer::invalidate_if_needed` (`crates/renderer/src/vulkan/buffer.rs:1270`)
- **Description**: each copy is the same 25 lines: `if !allocation.memory_properties().contains(HOST_COHERENT) { aligned_flush_range(..); unsafe { invalidate_mapped_memory_ranges(&[MappedMemoryRange..]) }; warn on Err }`. Each cites the earlier one in its comment ("Same reasoning and same shared range helper as the screenshot path"). `GpuBuffer::invalidate_if_needed` additionally calls `debug_assert_flush_range_bounded(alloc, ..)` (#2683, `buffer.rs:747-`), which flags a widened range that escapes a dedicated allocation (VUID-VkMappedMemoryRange-size-01389); none of the three copies has it. #2740 had to retrofit the depth path to match the screenshot path, and the third path was added by copying the second. The only divergence is a debug assertion, which is why this is LOW; it fits the promotion trigger in letter only.
- **Suggested Fix**: `buffer::invalidate_allocation(device: &ash::Device, allocation: &vulkan::Allocation) -> Result<()>` holding the coherent check, `aligned_flush_range`, `debug_assert_flush_range_bounded` and the `invalidate_mapped_memory_ranges` call; `GpuBuffer::invalidate_if_needed` delegates to it when `!self.is_coherent`; the three readbacks call it and keep their own `warn!`. `screenshot_readback_invalidates_before_mapping` (`context/resources.rs:1319`) scans `screenshot.rs` for the literal `invalidate_mapped_memory_ranges`, so re-point it at `invalidate_allocation` in the same commit.

##### TD2-2026-11-15-08: Seven near-identical `ScriptFunctionDeclaration` builders in `sdk::compatibility`
- **Dimension**: 2 · **Severity**: LOW · **Effort**: small · **Labels**: `tech-debt` · **Existing**: none
- **Age**: 6c94ca3f5 / 234665577 (2026-09-05, 2026-09-23 splits; the copies are older)
- **Location**: `crates/sdk/src/compatibility/declarations.rs:12` (`papyrus_game_content_declaration`) and `:49` (`papyrus_game_entity_declaration`), `input_ui.rs:13` (`papyrus_input_declaration`) and `:48` (`papyrus_ui_declaration`), `mod_events.rs:7` (`papyrus_mod_event_declaration`), `legacy_containers.rs:21` (`papyrus_legacy_container_declaration`), `storage_util/declarations.rs:282,292,301` (`papyrus_storage_util_declaration`, `_void_declaration`, `_declaration_with_result`)
- **Description**: each builds `EnginePapyrusFunctionDeclaration { route, declaration: ScriptFunctionDeclaration { id: ScriptFunctionId::new(id).expect("built-in <X> function ID is valid"), component: ComponentId::new("<c>").expect(..), parameters: parameters.iter().cloned().map(|(id, value_type[, optional])| ScriptParameterDeclaration { id: ScriptParameterId::new(id).expect(..), value_type, optional }).collect(), result: result.map(|value_type| ScriptResultDeclaration { value_type, optional: false }), papyrus: Some(PapyrusFunctionAlias { provider, function }), description } }`. What varies is five strings (component, provider, description, and the `expect` labels) and whether the parameter tuple carries `optional` and the result is `Option`. Two of them (`declarations.rs`, `papyrus_ui_declaration`) take `(id, type)` pairs and hard-code `optional: false`, the other four take triples; the `expect` messages differ only in the product name. A fix to how an invalid ID is reported, or a new field on `ScriptFunctionDeclaration`, must be applied seven times.
- **Suggested Fix**: one `fn engine_papyrus_declaration(spec: &DeclarationSpec, route, id, function, parameters: &[(&str, ScriptValueType, bool)], result: Option<ScriptValueType>) -> EnginePapyrusFunctionDeclaration` in `compatibility/mod.rs`, with `DeclarationSpec { component, provider, description }` constants per surface. The seven become one-line wrappers or disappear into their call tables.

##### TD2-2026-11-15-09: The perk entry-point composition's scheduler `Access` is hand-copied at five registrations; the crime, ownership and gold subsystems already export a `fn(Access) -> Access`
- **Dimension**: 2 · **Severity**: LOW · **Effort**: small · **Labels**: `tech-debt` · **Existing**: none (#6701 added `Access::with` for exactly this; the perk block was not migrated)
- **Age**: 49aa54e36 (2026-11-05, `npc_combat_ai.rs`); the block predates the split
- **Location**: `byroredux/src/boot/schedule/update/combat/combat_input.rs:17-29`, `update/combat/npc_combat_ai.rs:26-38`, `update/player_animation/combat_damage.rs:38-49`, `update/interaction_and_crime/interaction.rs:119-126`, `byroredux/src/boot/schedule/early.rs:142-148`
- **Description**: the first two are byte-identical for 13 lines (`PerkEntryTable`, `Perks`, `StoryManagerRng`, `WeaponSubject`, `PerkFragments`, `PendingPerkEntries`, `PapyrusPlayerEntity`, `FormIdComponent`, `FormIdPool`, `SceneAliasCandidate`), including the same two comment blocks ("PERK-SUB-002 - the outgoing-damage composition ...", "PERK-HOOK-008 ..."); `combat_damage.rs` carries the same set interleaved with its own entries, and `interaction.rs` / `early.rs` a subset. `scripting` already exports `crime_gold_access`, `crime_faction_access` and `ownership_access` (`crates/scripting/src/crime_gold.rs:36`, `crime_faction.rs:184`, `ownership.rs:85`) and the scan follows them (`acquisition_scan.rs`, #6701). A composer that gains a resource (the next `PERK-HOOK-*`) must be declared at all five or the lock-order and access guards go stale at the ones it forgets.
- **Suggested Fix**: `pub fn perk_entry_composition_access(access: Access) -> Access` in `crates/scripting/src/perk_entry_points/mod.rs`, next to the composer, covering the shared set; the five registrations call `.with(perk_entry_composition_access)` and keep only their own extras. The `acquisition_scan` already resolves `fn(Access) -> Access` helpers, so no guard change is needed beyond adding the file to the `#6701` list at `boot/schedule/mod.rs:756`.

##### TD2-2026-11-15-10: Two groundcover files hand-write the global memory barrier that `descriptors::memory_barrier` exists for
- **Dimension**: 2 · **Severity**: LOW · **Effort**: trivial · **Labels**: `tech-debt` · **Existing**: none (regression of #1061 / TD3-103, the `descriptors.rs` helper)
- **Age**: `groundcover_models.rs:973` aabd99a05 (2026-09-24), `groundcover/frame.rs:791` 637b65264 (2026-09-06)
- **Location**: `crates/renderer/src/vulkan/groundcover/frame.rs:791` (`fn buffer_barrier`, five call sites at `:363,388,468,537,562`), `crates/renderer/src/vulkan/groundcover_models.rs:973` (`fn barrier`, call sites `:811,848`); the home is `crates/renderer/src/vulkan/descriptors.rs:305` (`pub(crate) unsafe fn memory_barrier`, about twenty other call sites across `context/` and the pass files)
- **Description**: both local functions are the same body as the home: build one `vk::MemoryBarrier` from the two access masks and `cmd_pipeline_barrier(cmd, src_stage, dst_stage, empty, &[barrier], &[], &[])`, only wrapped in a safe `fn` with its own SAFETY comment. Every other pass imports `memory_barrier`.
- **Suggested Fix**: delete both, call `unsafe { memory_barrier(device, cmd, ..) }` at the seven sites (each already sits in a recording context). The `frame.rs` scan at `:963` that looks for the literal `buffer_barrier(` between two markers needs the new spelling.

##### TD2-2026-11-15-11: The FSR2/3 "reactive-mask MAX" blend attachment state is spelled three times
- **Dimension**: 2 · **Severity**: LOW · **Effort**: trivial · **Labels**: `tech-debt` · **Existing**: none (the sibling `auxiliary_blend_attachment` was shared for the same reason in #3821)
- **Age**: composite.rs 5d3625416 (2026-07-28); pipeline.rs and water.rs similar
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:693`, `water.rs:1010` (both named `fsr_mask_max`), `composite.rs:1120` (`mask_blend`); the shared sibling is `pipeline::auxiliary_blend_attachment` (`pipeline.rs:666`)
- **Description**: all three are `color_write_mask(R)`, `blend_enable(true)`, ONE/ONE factors, `BlendOp::MAX` on both colour and alpha. `water.rs` reuses `auxiliary_blend_attachment()` two lines above its copy with the comment "Shared ... rather than duplicated so the two pipelines' coverage semantics cannot drift apart", then spells the mask state inline. `pipeline.rs:690` carries the AMD integration-contract rationale for MAX accumulation, which the other two copies do not.
- **Suggested Fix**: `pub(crate) fn fsr_mask_max_attachment() -> vk::PipelineColorBlendAttachmentState` beside `auxiliary_blend_attachment`, with the rationale comment and a pin test like `auxiliary_blend_attachment_is_a_src_alpha_coverage_blend` (`pipeline.rs:1189`); the three call it.

##### TD2-2026-11-15-12: "First inventory row holding base form X" is hand-spelled five times next to `Inventory::count_of`
- **Dimension**: 2 · **Severity**: LOW · **Effort**: trivial · **Labels**: `tech-debt` · **Existing**: none (same family as #6363 / `count_of`)
- **Age**: d467b074c (2026-10-30) for `queue_drop_by_form_id`; the others older
- **Location**: `byroredux/src/inventory/mod.rs:149` (`queue_consume_by_form_id`), `:163` (`queue_drop_by_form_id`), `:199` (`drain_script_weapon_equips`); `crates/scripting/src/obscript_effects/inventory.rs:157`; `byroredux/src/systems/projectiles.rs:200` (`inventory.items.iter().any(|s| s.base_form_id == *ammo && s.count > 0)`, which is `count_of(*ammo) > 0`)
- **Description**: four sites spell `items.iter().position(|s| s.base_form_id == X && s.count > 0)` and one spells the held test. `queue_consume_by_form_id` and `queue_drop_by_form_id` are also identical for 12 lines except the `InventoryAction` variant and the verb in two strings.
- **Suggested Fix**: `Inventory::first_row_of(&self, base_form_id: u32) -> Option<usize>` in `crates/core/src/ecs/components/inventory.rs` beside `count_of` (`:144`), `count_of(..) > 0` at `projectiles.rs`, and one private `queue_by_form_id(world, form_id, make: impl FnOnce(u32, u32) -> InventoryAction, verb)` for the consume/drop pair.

#### Dimension 3: Stale Documentation & Comments

##### TD3-2026-11-15-01: CLAUDE.md and game-loop.md state `--radius 1..=7` / "default 3"; the code clamps 1..=12 with default 12
- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none (`docs/engine/exterior-grid-streaming.md:28-30` already calls both notes stale, nobody fixed them).
- **Age**: the clamp became `1..=12` in 5916a9d99 (2026-05-30). `docs/engine/game-loop.md:38` carries the old text since 6f73eadb4f (2026-05-28). CLAUDE.md:45 was rewritten on 2026-11-13 (0d17432bd8) and kept the stale range.
- **Evidence**: `byroredux/src/scene.rs:55-58` (`parse_exterior_radius` -> `r.clamp(1, 12)`), `byroredux/src/scene.rs:39` (`DEFAULT_EXTERIOR_RADIUS: i32 = 12`). Stale: `CLAUDE.md:45` ("radius 1..=7"), `docs/engine/game-loop.md:38` ("radius 1..=7, default 3").
- **Also**: `docs/engine/exterior-grid-streaming.md:25-28` cites `scene.rs:75`, `scene.rs:228-263`, `scene.rs:50-55`; those are in range (so `stale_line_references` stays green) but now mean something else (`setup_scene` is `scene.rs:224`, `parse_exterior_radius` `scene.rs:55`, `--grid` is read at `scene.rs:487`). Name the symbols instead of lines.
- **Fix**: CLAUDE.md:45 -> "(radius clamped 1..=12, default 12)", game-loop.md:38 likewise, and replace the three line refs in exterior-grid-streaming.md with `setup_scene` / `parse_exterior_radius`. CLAUDE.md is the file every agent reads first, so do not leave it to the next sweep.
- **Also (orchestrator check)**: the comment above the parse at `byroredux/src/scene.rs:505-506` says "Defaults to 5 ... raised from 3", and the constant it describes (`DEFAULT_EXTERIOR_RADIUS`, `scene.rs:39`) is 12. Fix it in the same commit.

##### TD3-2026-11-15-02: `shader-pipeline.md` says the depth buffer is standard (0 = near, clear 1.0, LESS_OR_EQUAL); reversed-Z has shipped
- **Severity**: LOW (promotion candidate: it is the "authoritative" renderer reference; I could not show it misled an audit, `audit-renderer/SKILL.md:78` already states reversed-Z correctly). **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Age**: 09940503b (2026-11-07, #6642 DLOD-DEPTH-002) flipped the mapping; the commit updated `exterior-readiness-plan.md` and ROADMAP only.
- **Evidence**: `crates/core/src/ecs/components/camera.rs:192` (`ACTIVE_DEPTH_MAPPING = DepthMapping::Reversed`), `crates/renderer/src/vulkan/pipeline.rs:174` (`DEPTH_CLEAR_VALUE = ACTIVE_DEPTH_MAPPING.clear_value()`, 0.0 under reversed), test `the_shipped_depth_mapping_is_reversed` at `camera.rs:1378`. Stale:
  - `docs/engine/shader-pipeline.md:490` ("Standard depth (0.0 = near, 1.0 = far), `LESS_OR_EQUAL`, clear = 1.0")
  - `docs/engine/coordinate-system.md:320-321` ("shipped: Conventional")
  - `docs/prd/distant-lod-completion.md:158` ("shipping `DepthMapping::Conventional` ... pinned by `the_shipped_depth_mapping_is_conventional`") and `:509` (the test no longer exists)
- **Fix**: shader-pipeline.md:490 -> reversed (1.0 = near, clear 0.0, the compare op the pipeline derives for it); coordinate-system.md:321 -> "shipped: Reversed"; PRD row 158 -> Reversed, `the_shipped_depth_mapping_is_reversed`. Name `ACTIVE_DEPTH_MAPPING` and let the number follow it.

##### TD3-2026-11-15-03: `shader-pipeline.md` binding table omits Set 0 binding 2 (`TextureFirstLevels`)
- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Age**: f754eee28 (2026-11-14, #6629) added the binding and updated `texture-streaming.md` and `memory-budget.md`, not the table.
- **Evidence**: `crates/renderer/shaders/include/bindings.glsl:16` (`layout(std430, set = 0, binding = 2) readonly buffer TextureFirstLevels`), pinned at `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:8183`. `docs/engine/shader-pipeline.md` lists Set 0 bindings 0 and 1 only (rows at ~856-857) and never names `TextureFirstLevels`. I diffed every other Set 0/1 binding in `bindings.glsl` against the table: all present (12 and 18 are declared outside `bindings.glsl` and are in the table).
- **Fix**: add the row `| 0 | 2 | STORAGE_BUFFER | TextureFirstLevels (per-handle first resident mip, #6629) | triangle, water, groundcover_blade |`. The Set-1 lockstep guards do not cover Set 0, so nothing else will catch the next one.

##### TD3-2026-11-15-04: `dependencies.md` "Internal Crate Dependencies" table lists 24 of 33 crates and gets five rows wrong; `architecture.md` says 35 members and omits `video`, `bink2`, `binkaudio`
- **Severity**: LOW. **Dimension**: 3. **Effort**: small. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none open. Recurrence of closed #6164 ("27 members, it is 34") and #5599.
- **Evidence** (`[dependencies]` of each `crates/*/Cargo.toml` vs the doc):
  - Missing from the table entirely: `bink2` (leaf), `binkaudio` (leaf), `video` (-> bink2, binkaudio), `boot-request` (-> core), `game-detect` (-> bsa, core, plugin), `menuxml` (leaf), `sdk` (-> core), `settings-io` (-> core), `terrain-height` (leaf).
  - Wrong rows (`docs/engine/dependencies.md:137-170`): `byroredux-plugin` (doc: core; actual core, sdk, bsa), `byroredux-scripting` (doc omits save, sdk, hkx), `byroredux-debug-server` (doc omits scripting), `byroredux-debug-ui` (doc: core; actual core, sdk), `byroredux-mod-runtime` (doc: "none, leaf"; actual -> sdk).
  - `docs/engine/architecture.md:17,24` "35 members (2026-10-22)": 38 today (ROADMAP Project Stats is right); the tree at `architecture.md:26+` has no `video`, `bink2`, `binkaudio` (added 2026-11-11..13: 6e64c453b, 8e11d49a4, 3ebecb7ec). README, `workspace-map.md` and `index.md` do list them.
- **Fix**: regenerate the table from `cargo metadata` (or `rg '^byroredux' crates/*/Cargo.toml`) and add a source-scan test beside `workspace_hygiene_tests.rs` that every `crates/*` name appears in the table, the way `every_registered_system_is_named_in_the_live_schedule_table` does for the schedule. Replace the hand-typed member count in architecture.md with a pointer to ROADMAP Project Stats (it already says "live count is in ROADMAP").

##### TD3-2026-11-15-05: `music.md` names two types that do not exist (`MusicDirective`, `PendingMusicEvents`)
- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none (00472c4a1 / #6934 swept the music commits' doc rot but missed these).
- **Evidence**: `docs/engine/music.md:15` ("become a `MusicDirective`") and `:43` ("a level-up (`PendingMusicEvents`, pushed where the level is awarded)"). Code: the type is `Directive` (`byroredux/src/music/directive.rs:75`); level-up stings go through `raise_sting` into `MusicCatalog::stings` (`byroredux/src/music/mod.rs:94,155-160`). The gate listed both as unresolved symbols.
- **Fix**: `Directive`; `raise_sting` (`MusicCatalog.stings`).
- **Advisory triage, other window-rename hits**: `docs/engine/exterior-readiness-plan.md:562,1013,1228-1265` still name `apply_cell_climate_override` (replaced by `apply_cell_weather`, ea5417dc7) and `AudioWorld::play_music`/`stop_music` (now `play_music_voice`, `crates/audio/src/world.rs:553`) as live; the plan is a dated narrative, so italicise rather than rewrite.

##### TD3-2026-11-15-06: `packal.md` describes `from_skyrim_procedure_tree` as the live Skyrim fallback; 88c239b35 deleted it
- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Age**: 88c239b35 (2026-11-09, #6660) replaced it with `byroredux/src/npc_spawn/ai_package/tree.rs` ("New npc_spawn/ai_package/tree.rs replaces from_skyrim_procedure_tree").
- **Evidence**: `docs/engine/packal.md:175` ("and a `from_skyrim_procedure_tree` fallback") and `:254` ("`from_skyrim_procedure_tree` now tries `Sandbox` first, then `Patrol`"); the symbol has no definition at HEAD. Also listed in the gate's advisory.
- **Fix**: say "the procedure-tree walk (`ai_package/tree.rs`, AIP-TREE-001..008)" and, since the section is a "what shipped" history of the pre-tree stage, mark it as superseded rather than present tense.

##### TD3-2026-11-15-07: `crates/nif/src/anim/keys.rs:201` cites the deleted `byroredux/src/cell_loader/euler.rs`
- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Age**: 13647f400 (2026-11-06, #6713) deleted `euler.rs`, moving the helpers to `byroredux/src/zup_yup.rs`.
- **Evidence**: `crates/nif/src/anim/keys.rs:199-202` (comment "`--rotation-mode 3` (`byroredux/src/cell_loader/euler.rs`), a non-shipping diagnostic"); `--rotation-mode N` is documented at `byroredux/src/zup_yup.rs:26`. The deleted-file sweep (6 files deleted since 10-31) found this as the only live mention of a deleted path; `audit-legacy-compat/SKILL.md:31` names it correctly as moved.
- **Fix**: `byroredux/src/zup_yup.rs`. The same 12-line block is also history narration ("the prior `euler_to_quat_wxyz` composed...", #2434), see TD3-08; rewrite it once to state the rule (Gamebryo Euler is CW-positive XYZ, routed through `euler_zup_to_quat_yup`).

##### TD3-2026-11-15-08: ROADMAP.md still lists #6608 (Skyrim exterior->interior device loss) as an open known issue; it closed 2026-11-05
- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Evidence**: `ROADMAP.md:638-640` ("Skyrim exterior -> interior door loses the Vulkan device (#6608, 2026-10-31) ... not bisected"). `gh issue view 6608`: CLOSED COMPLETED 2026-11-05, "Fixed in 7c72057be" (a loading-stage frame stops dispatching stale ground-cover scatter). The section's own rule (`ROADMAP.md:553`) is "each closed item is removed at the session close that closes it". The closing comment mentions "remaining P1 flakiness"; if that is still wanted, restate it as its own item with its own tracker.
- **Cross-check of the other 19 issue refs in the Known Issues block** (`ROADMAP.md:553-700`): the 3 open (#3398, #5764, #6350) are correctly described; the other 16 closed ones are cited as fixed or historical. Milestone rows (M39, M24.2, M42, M48, M65 trackers #6626/#6635/#6634/#6641/#6908) match open trackers.
- **Fix**: delete the bullet (or rewrite per above).

##### TD3-2026-11-15-09: history-narrating comments (#6719 rule) concentrate in three files touched in the window
- **Severity**: LOW. **Dimension**: 3. **Effort**: small (one file per commit, comment-only diffs). **Labels**: `tech-debt`, `doc-rot`. **Existing**: none (#6719 is the rule; the first pass was `skinned_blas_refit.rs`).
- **Method**: of 817 `.rs` files touched in 670f81917..HEAD, counted comment lines with "used to", "pre-#N", "previously", "formerly", "was replaced", "before this fix" and similar; the three below are the worst real ones (counts are matching comment lines at HEAD; `resize.rs` and `shader_contract_tests.rs` scored high on noise such as "the old surface" and were not counted).
- **Evidence**:
  - `crates/renderer/src/vulkan/context/frame_params.rs` (17): `:108-120` ("the VP-matrix limb used to BE the cut signal ... It was replaced by two signals"), `:184` ("already had before this fix"), `:219` ("the path it used before #2219"), `:398` ("comment previously claimed Halton(2)/Halton(3)...").
  - `crates/core/src/ecs/components/material.rs` (22): `:269-273` ("pre-#398 it was extracted but never reached the GPU", "the value used pre-#398"), `:364-368` ("this doc used to justify that shape by citing ... At the time there was no such field"), `:502-505` ("this doc used to teach that the default was `0`").
  - `crates/plugin/src/esm/records/index.rs` (12): `:4-5` ("Lifted out of the pre-#1118 monolithic `records/mod.rs`"), `:213` ("The pre-#5644 one-INFO-at-a-time fold"), `:334`, `:358` ("pre-#458 fell through").
  - Same pattern in `crates/nif/src/anim/keys.rs:190-203` (TD3-07).
- **Fix**: per file, rewrite each block to the rule the code follows today plus the issue number; check for tests that pin comment text first (the skill lists the known ones). Not a bulk sweep.

##### TD3-2026-11-15-10: An lstring-reader doc block sits on `read_u32_sub`; the function it describes does not exist
- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Age**: 470ddd2f1 (2026-04-07).
- **Location**: `crates/plugin/src/esm/records/common.rs:303-308`
- **Description**: The doc block reads "Read a sub-record as an lstring-or-zstring. Returns `None` if absent. Collapses the `find_sub(subs, code).map(read_lstring_or_zstring)` two-liner that appears at every FULL / DESC / RNAM site. Use this instead of the open-coded pair. See TD3-007 / #1045." It is attached to `pub fn read_u32_sub` (`:309`), which reads a little-endian `u32`. No `read_lstring_sub`-style function exists anywhere in `crates/plugin/src` (`grep` is empty). So `read_u32_sub`'s rendered doc is wrong, and the contract describes a helper that was either never added or was deleted. `no_doc_block_is_spliced_onto_the_wrong_item` cannot see it, because the block's first backticked name is not a declared sibling: this is the narrow guard's documented blind spot. (Found by the Dim 2 agent and verified by the orchestrator.)
- **Fix**: Either delete the block and give `read_u32_sub` its own one-line doc, or add the helper it describes and route the FULL / DESC / RNAM sites through it. That second option is a Dim 2 consolidation; check how many open-coded pairs remain before choosing it.

#### Dimension 4: Audit-Finding Rot

##### TD4-2026-11-15-01: `audit-oblivion` and `audit-fnv` cite `placement_lod_supported_is_oblivion_only` as a live guard; cc39d7792 deleted it
- **Severity**: LOW. **Dimension**: 4. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Age**: cc39d7792 (#6637, DLOD-GATE-008) replaced `placement_lod_supported_is_oblivion_only`, `combined_and_legacy_lod_capabilities_are_disjoint` and `object_lod_scheme_table`'s scheme rows with `lod_family_table_covers_every_game` (`byroredux/src/cell_loader/lod_support.rs:681`, doc at `:675-680`).
- **Evidence**: `.claude/commands/audit-oblivion/SKILL.md:98` ("`placement_lod_supported_is_oblivion_only`; FO3/FNV ship none ..."), `.claude/commands/audit-fnv/SKILL.md:24` ("`placement_lod_supported` is Oblivion-only (`placement_lod_supported_is_oblivion_only`)"). No `fn placement_lod_supported_is_oblivion_only` at HEAD; the function `placement_lod_supported` (`placement_lod.rs:359`) still exists.
- **Fix**: name `lod_family_table_covers_every_game` in both. The gate's symbol check reports only `docs/engine`; extending it to skills (backticked names of 4+ snake words with no `fn`/`const`/`struct` definition and not in `_audit-validate-symbol-exempt.txt`) found exactly this one real hit among 24 candidates (the rest are ash/std names, struct fields, TSV keys, or file stems), so it is cheap to add.

##### TD4-2026-11-15-02: the tech-debt SKILL.md Phase-1 snapshot paragraph: figures that have drifted
- **Severity**: LOW. **Dimension**: 4. **Effort**: trivial (one `/audit-sync` pass). **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Method**: re-ran the Phase-1 greps and `prod_loc` at HEAD (the paragraph says "diff direction only, re-run, never quote", so these are listed for the next author, not as errors).
- **Drifted** (`.claude/commands/audit-tech-debt/SKILL.md:80-135`):
  - `allow(dead_code)` quoted 43, now 46: new at `crates/renderer/src/vulkan/context/mod.rs`, `crates/renderer/src/vulkan/shadow_filter.rs` and `byroredux/src/render/skin_offsets_hasher_tests.rs` (0 -> 1 each since 8ff82d514). The paragraph's list of the "five new" is therefore incomplete.
  - Audio split: `crates/audio/src/` "largest child ~610" is now `world.rs` 736 prod LOC (+21 %, 719 at 8ff82d514); hub `lib.rs` 202 (said ~200).
  - Largest children: `byroredux/src/inventory/` ~790 -> `catalog.rs` 852 (HEAD, +8 %); `byroredux/src/npc_spawn/` ~1460 -> `ai_package.rs` 1538 (+5 %); `crates/debug-ui/src/panels/game_menu.rs` 749 -> 757.
  - Watch list: `byroredux/src/app_step.rs` 1899 -> 1935 (now 65 from the band; the row text says "within 5 %"); `crates/plugin/src/esm/records/misc/world.rs` 2054 -> 2052; `byroredux/src/systems/character.rs` (1834 at 8ff82d514, 1842 now) and `crates/physics/src/world/mod.rs` (1807) are over 1800 production LOC and named nowhere in the paragraph.
  - ROADMAP.md quoted 797, now 798.
- **Verified unchanged**: markers 40 (18 `XXXX` + 18 `HACK` + 3 `FIXME` + 1 `TBD`), `unimplemented!/todo!()` 0, `#[ignore]` 436 (quoted 435), files >2000 production LOC 2 (`shader_constants_data.rs` 3502, `world.rs`), test files >2000 total LOC 95, `studio.rs` 883, `gpu_timers.rs` 1196, `context/init.rs` 1131, `mesh.rs` 1412, `fragment/vm` largest child ~1400 (1422), `nif/import/types` 575, `obscript_effects` ~550 (556), `mq101_conformance` ~530 (532), `panels/studio.rs` and the two other children.
- **Fix**: refresh on the next `/audit-sync`; add `character.rs` and `physics/src/world/mod.rs` to the watch list.

##### TD4-2026-11-15-03: `_audit-common.md` hub-size range "~100-230 LOC" no longer holds
- **Severity**: LOW. **Dimension**: 4. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Evidence**: `.claude/commands/_audit-common.md:9` ("**split hubs** (a ~100-230-LOC `.rs` over the bulk in its directory"). Hubs listed there: `crates/scripting/src/condition.rs` 283 lines, `byroredux/src/cell_loader/exterior.rs` 247, `crates/plugin/src/esm/cell/mod.rs` 203, `translate/effects.rs` 177, `fragment/vm.rs` 136, `save_io.rs` 108. The upper bound is exceeded by two of six (`condition.rs` +23 %).
- **Fix**: "~100-290" or drop the number; the point of the bullet (scope to the `<stem>/` directory) does not depend on it. The other numeric claims in the file hold (`crates/platform` 60 LOC = 8 + 52, `cxx-bridge` 9-line C++ stub = `native_utils.cpp` 9 lines, `systems.rs` is a bare re-export at 125 lines).

#### Dimension 6: Stub & Placeholder (production-unreachable)

##### TD6-2026-11-15-01: The `EsmIndex` unconsumed-record inventories are stale in both directions
- **Severity**: LOW
- **Dimension**: 6 - Stub & Placeholder Implementations (production-unreachable) / doc rot
- **Location**: `docs/engine/esm-records.md:441-455`; `crates/plugin/src/esm/records/index.rs:640`; `ROADMAP.md:673-691`
- **Age**: the media_sets claim went stale at `45b3b0a64` (2026-11-14, the music controller); the inventories were last declared "complete" at `9398d788a` (2026-11-08, #6805 / #6806)
- **Effort**: small
- **Labels**: tech-debt, doc-rot
- **Status**: NEW (the earlier rows, #6387 / TD6-2026-10-25-01, are CLOSED; #6805 / #6806 are CLOSED as "inventory is complete")
- **Description**:
  1. **Wrongly listed as unconsumed.** `esm-records.md:441-446` lists `media_sets` (MSET) among nine maps that "nothing outside `records/` reads" and `index.rs:640` says "no runtime reads it yet; that is the open #3816". `byroredux/src/music/translate.rs:169,443,447,453` reads `index.media_sets` in production (non-test) code, and `media_locations` (ALOC) is read at `music/mod.rs:236` and `translate.rs:163,198,441`. The Fallout 3 / New Vegas / Oblivion music path consumes them; only Skyrim `MUST` and Starfield remain open.
  2. **Genuinely unconsumed but on neither list.** A scan of every `pub` `EsmIndex` map for readers outside `records/index.rs`, `records/dispatch_*` and tests finds these with none: `debris` (DEBR), `casinos` (CSNO), `music_tracks` (typed `MustRecord`, doc "what a Skyrim `MUSC.TNAM` lists"), `soul_gems` (typed `SlgmRecord`), and the Oblivion-unique stubs `clothing`, `apparatuses`, `sigil_stones` (only `sigil_stones` is on the ROADMAP list; the first three also dual-fill `cells.statics`, so their maps are unread). `ROADMAP.md:673` counts "Fifteen" maps and lists exactly fifteen, so the count is wrong by at least `debris` and `casinos`. Other fields that look unread in a grep (`movement_types`, `body_parts`, `navi_info`, `object_mod_loose_items`, `magic_effects_by_code`) are consumed through `EsmIndex` accessors or `systems/vats.rs:516` and are fine.
  3. The ROADMAP list is honest about the typed-but-unwired set otherwise (`enchantments`, `reputations`, `combat_styles`, `impact_data_sets`, `impacts`, `weather_settings`, `volumetric_lighting`, `god_rays` still have zero readers: re-verified).
- **Evidence**: `grep -rnE '\.(enchantments|reputations|combat_styles|impact_data_sets|impacts|media_sets|weather_settings|volumetric_lighting|god_rays)\b' crates byroredux` (the doc's own re-check recipe) minus tests and `records/` returns the `music/translate.rs` hits for `media_sets` and nothing for the other eight.
- **Impact**: the doc's own recipe now disagrees with its list, and the long-tail allocation cost the ROADMAP row exists to track (a `HashMap` plus `String`s per stub record on every FNV/FO3/Oblivion load) is under-counted by the unlisted maps.
- **Suggested Fix**: in `esm-records.md` move `media_sets` out of the unconsumed list (eight maps) and add `music_tracks` and `soul_gems` to it; fix `index.rs:640`'s comment to say what the music controller reads and that only Skyrim `MUST` remains; in `ROADMAP.md:673-691` add `debris` and `casinos` to the "candidates to drop" list and correct the count. A source-scan guard that derives the list the way the recipe does would keep it honest, but is optional.

#### Dimension 7: Magic Numbers & Hardcoded Constants

##### TD7-2026-11-15-01: the exterior cell edge `4096.0` was re-declared twice in the 11-07/11-08 window, and the 4x4 quad hole-mask layout is re-typed five times beside it
- **Severity**: LOW
- **Dimension**: 7 (magic numbers)
- **Location**: `crates/plugin/src/esm/records/misc/world.rs:830` (`REGION_CELL_UNITS`, used at `:925-928`); `byroredux/src/cell_loader/lod_coverage.rs:133` (`const CELL: f32 = 4096.0`); mask layout at `lod_coverage.rs:102-105`, `:120-123`, `:136`
- **Age**: `cf3ccc958` 2026-11-07 (`Refs #6725`), `8e7624256` 2026-11-08 (`Fix #6639`)
- **Effort**: trivial
- **Labels**: `tech-debt`
- **Status**: NEW (class history: closed #1112 "4096.0 literal scattered across 6+ files (divergent bug-fix history)", #2436, #4939; this is a new instance, not a regression of those lines)
- **Description**: `byroredux_core::math::coord::EXTERIOR_CELL_UNITS` (`crates/core/src/math/coord.rs:41`) is documented as the sole source of truth after #1112. Its doc comment records that the scattered literal had "at least one divergent bug-fix history (TD3-110 Z-flip sign disagreement)". Two production sites added since then re-type it. `crates/plugin` already depends on core and imports the constant at `esm/cell/index.rs:11` and `esm/cell/worldspace.rs:6`, so the plugin copy has no layering excuse. The only deliberate second copy, `bsa::uvd::EXTERIOR_CELL_UNITS`, is const-asserted against core from the bin crate (#4939) because bsa does not depend on core. Neither new copy has an assert. In `lod_coverage.rs` the same 4x4 quad layout is also spelled as bare `4`, `0..4`, `dy * 4 + dx` and `level == 4` in `find_overlaps`, `quad_resident_mask` and `quad_cell_bit`. `streaming_helpers.rs:438` names the same quantity `FINEST_QUAD_EDGE = 4` locally inside one function.
- **Evidence**:
  ```
  world.rs:830        const REGION_CELL_UNITS: f32 = 4096.0;
  lod_coverage.rs:133 const CELL: f32 = 4096.0;
  lod_coverage.rs:136 ((0..4).contains(&dx) && (0..4).contains(&dy)).then(|| 1u16 << (dy * 4 + dx))
  ```
- **Impact**: No divergence today because the value is invariant. If the cell edge were ever changed (a Starfield or FO76 grid, a test fixture scale), the region-polygon coverage and the LOD hole mask would silently disagree with the cell loader, which is the #1112 failure mode.
- **Related**: #1112, #2436, #4939 (closed, same constant); `crates/core/src/math/coord.rs:30-41`.
- **Suggested Fix**: delete both literals and use `byroredux_core::math::coord::EXTERIOR_CELL_UNITS`. Name the quad edge once (`const QUAD_EDGE_CELLS: i32 = 4` beside `quad_cell_bit`, reused by `streaming_helpers.rs:438`) and derive the `u16` mask width from `QUAD_EDGE_CELLS.pow(2)`.

##### TD7-2026-11-15-02: the Bink container ceilings are typed twice and linked only by a doc comment; the Bink 1 header offsets are bare
- **Severity**: LOW
- **Dimension**: 7 (magic numbers)
- **Location**: `crates/video/src/lib.rs:133` (`word(0x28).min(0x100)`, `word(0x10).min(1_000_000)`, `0x2c + 12 * … + 4 * …`); `crates/bink2/src/container.rs:60-72` (`MAX_AUDIO_TRACKS = 0x100`, `MAX_FRAMES = 1_000_000`)
- **Age**: `e13628ab4` 2026-11-12 (video probe); bink2 constants earlier
- **Effort**: trivial
- **Labels**: `tech-debt`
- **Status**: NEW
- **Description**: `bink2/container.rs:62-63` says "the track and frame limits are the ones `byroredux-video`'s Bink 1 probe clamps to", so a doc comment is the only link between two numeric copies. The Bink 2 copies are `pub const` but `mod container` is private (`bink2/src/lib.rs:14`; only `AudioPacket`, `AudioTrack` and `Bk2` are re-exported), so `video` cannot import them. Beside them, the Bink 1 header layout is bare in the same expression: `0x28` (track count), `0x10` (frame count), `0x2c` (header end), `12` (track record size), `4` (offset entry size), and a `[0u8; 0x2c]` buffer.
- **Evidence**: `let table_end = 0x2c + 12 * word(0x28).min(0x100) + 4 * (word(0x10).min(1_000_000) + 1);`
- **Impact**: This bounds an allocation from an untrusted header (`bytes.resize(table_end, 0)`, at most about 4 MB). If one copy is tightened (BIK-AUD-3 already moved bink2's limits once) and the other is not, the Bink 1 probe (which clamps) and the Bink 2 container (which refuses) stop agreeing about what an over-sized header is. This is bounded and cannot overflow today.
- **Related**: BIK-AUD-1, BIK-AUD-3 (cited in `video/src/lib.rs` and `bink2/src/container.rs`); `docs/clean-room-decoders.md`.
- **Suggested Fix**: re-export `MAX_AUDIO_TRACKS` and `MAX_FRAMES` from `byroredux-bink2` (`pub use container::{…, MAX_AUDIO_TRACKS, MAX_FRAMES}`), use them in `probe`, and name the five Bink 1 header offsets as `const BIK1_*` next to `probe`.

##### TD7-2026-11-15-03: the close-range fire march caps (`160` steps, `64` brick jumps) are derived from `LOCAL_GRID_DIM` / `LOCAL_GRID_BRICK` in prose only (PLAUSIBLE)
- **Severity**: LOW
- **Dimension**: 7 (magic numbers)
- **Location**: `crates/renderer/shaders/composite.frag:314` and `:322`; exempted at `crates/renderer/src/shader_constants.rs:355,358`; the derivation inputs are `shader_constants_data.rs:2149-2153` and `shaders/include/local_grid_params.glsl:106`
- **Age**: M-LIGHT local-fire work (#6533/#6535), within the window
- **Effort**: trivial
- **Labels**: `tech-debt`
- **Status**: NEW
- **Description**: the comments state the derivations: "A box is 64 cells across, so the cap only matters for a ray running diagonally" (steps) and "at most 3 * 8 bricks are crossed along any axis-aligned path; diagonals cross a few more" (brick jumps). `LOCAL_GRID_DIM = 64` and `LOCAL_GRID_BRICK = 8` are generated defines, and `LOCAL_GRID_BRICKS_PER_AXIS` is already computed in `local_grid_params.glsl:106`, but the two caps are exempted as "stage-local" literals. `shader_constants_data.rs:2165-2167` const-asserts workgroup divisibility only, not the march bound.
- **Evidence**: `const int LOCAL_FIRE_MAX_STEPS_PER_BOX = 160;` and `const int LOCAL_FIRE_MAX_BRICK_JUMPS = 64;`. A diagonal through a 64-cell box is about 111 cells (64 x sqrt 3), so 160 steps leaves about 1.4x slack at the 1-cell step the loop falls back to in busy gas. The same diagonal at `LOCAL_GRID_DIM = 128` is about 222 cells.
- **Impact**: PLAUSIBLE, not reproduced. If `LOCAL_GRID_DIM` is raised the march silently truncates in busy fire and the flame looks clipped, with no test failing. The brick-jump cap holds until about 24 bricks per axis (3 x 24 = 72 > 64). Nothing is wrong at today's 64/8.
- **Related**: #6533, #6535, #6526 (closed, the sibling god-ray bounds); `shader_constants.rs:211` exemption table.
- **Suggested Fix**: derive both from the generated defines (`3 * LOCAL_GRID_BRICKS_PER_AXIS + 8`, and `int(ceil(1.74 * float(LOCAL_GRID_DIM))) + 48` or a named `LOCAL_FIRE_STEP_HEADROOM`), then drop the two exemption rows.

##### TD7-2026-11-15-04: `bindings.glsl` declares the sky-SH array and its loop with a bare `9` while `SKY_SH_COEFFICIENTS` is already in scope
- **Severity**: LOW
- **Dimension**: 7 (magic numbers; shader lockstep)
- **Location**: `crates/renderer/shaders/include/bindings.glsl:283` (`vec4 skyDiffuseCoefficients[9];`) and `:291` (`for (uint i = 0u; i < 9u; ++i)`)
- **Age**: SKYAL sky SH (pre-window)
- **Effort**: trivial
- **Labels**: `tech-debt`
- **Status**: NEW
- **Description**: `SKY_SH_COEFFICIENTS` (`shader_constants_data.rs:77`, emitted as `#define SKY_SH_COEFFICIENTS 9`, `shader_constants.glsl:278`) is the named count. `sky_irradiance.comp:9` uses it, and a source-scan test pins the producer (`shader_constants.rs:1188`, `:1215-1235`: the SSBO size, the dispatch count and the `sky_sh.glsl` case count). The consumer declaration and loop in `bindings.glsl` are outside that test, and `bindings.glsl` documents that it depends on `shader_constants.glsl`. The SSBO the consumer reads is sized by the Rust constant (`irradiance.rs`: `SKY_SH_COEFFICIENTS as u64 * 16`), so the consumer's `[9]` is a second copy of a lockstep number.
- **Impact**: the L2 SH count is mathematically fixed at 9, so a change is unlikely. If it ever changed, the consumer would read past the SSBO or ignore coefficients with no failing test, which is why this is listed at all.
- **Related**: #5620 (closed: the same constants for the compute passes); `docs/engine/shader-pipeline.md`.
- **Suggested Fix**: `vec4 skyDiffuseCoefficients[SKY_SH_COEFFICIENTS];` and `i < uint(SKY_SH_COEFFICIENTS)`; extend the `shader_constants.rs:1188` check to assert `bindings.glsl` contains the named form.

##### TD7-2026-11-15-05: SDK / mod-runtime policy ceilings are typed twice (`MAX_DISPLAY_NAME_BYTES`, and the 16 KiB / 64 KiB string and blob caps)
- **Severity**: LOW
- **Dimension**: 7 (magic numbers)
- **Location**: `crates/mod-runtime/src/identity.rs:5` and `crates/sdk/src/manifest.rs:26` (both private `const MAX_DISPLAY_NAME_BYTES: usize = 256`); `crates/sdk/src/storage.rs:228-229` and `crates/sdk/src/component.rs:179-180` (`max_string_bytes: 16 * 1024`, `max_blob_bytes: 64 * 1024` in two `Default` impls); prose copy in `crates/mod-runtime/src/limits.rs:37-38`
- **Age**: not dated (never-swept crates)
- **Effort**: trivial
- **Labels**: `tech-debt`
- **Status**: NEW
- **Description**: `mod-runtime` already depends on `byroredux-sdk` (`mod-runtime/Cargo.toml:8`) and imports from it in the same file as the display-name copy (`identity.rs:1`), so the SDK constant could be imported. The two stores (`PrincipalStorageLimits`, `ComponentStoreLimits`) take the same 16 KiB / 64 KiB defaults; `limits.rs:36-38` documents "storage strings 16 KiB and blobs 64 KiB by default" as the reason a 2 MiB hostcall lift budget "dwarfs" them, which makes the numbers a sized relationship stated only in a comment.
- **Impact**: low; defaults only (`ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default())` is the only production construction, `byroredux/src/extensions/systems.rs:20`). A policy change made in one place leaves the other store or the principal-name check on the old value. Note for whoever raises these limits: `SandboxConfig::hostcall_fuel` (`limits.rs:157-161`) considers only `MAX_HOSTCALL_LIFT_BYTES` and the log-message cap, not the configured store caps, so a store cap above about 2 MiB would trap guest writes. Unreachable today.
- **Related**: `crates/mod-runtime/src/limits.rs:33-44`.
- **Suggested Fix**: make the SDK constant `pub` and import it in `identity.rs`; define `DEFAULT_MAX_STRING_BYTES` / `DEFAULT_MAX_BLOB_BYTES` once in the SDK and use them in both `Default` impls; add a `const _: () = assert!(DEFAULT_MAX_BLOB_BYTES <= MAX_HOSTCALL_LIFT_BYTES)` style check on the mod-runtime side.

#### Dimension 8: Dead Code & Backwards-Compat Cruft

##### TD8-2026-11-15-01: The blocking `cargo clippy --workspace -- -D warnings` gate is red on `movt.rs:37`, a sixth recurrence of the class
- **Severity**: LOW (no promotion trigger applies; note it blocks CI)
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft (clippy gate)
- **Location**: `crates/plugin/src/esm/records/movt.rs:35-39`
- **Age**: `49c92afbc` 2026-11-15 (Fix #6910, M65 Phase 1, which created `movt.rs`); committed text, not in-flight
- **Effort**: trivial
- **Labels**: tech-debt
- **Status**: NEW. Regression class of #6222 / #6582 (both CLOSED, "the blocking clippy gate is red again"); #5308 is the same lint (`chunks_exact_to_as_chunks`)
- **Description**: `cargo clippy --workspace --keep-going -- -D warnings` at HEAD (rustc 1.100.0) exits 101 with exactly one error: `using chunks_exact with a constant chunk size` at `movt.rs:37`. `byroredux-plugin` (lib) fails to compile under `-D`, so under the gate the crates downstream of plugin go unlinted. I re-ran the downstream crates without `-D` (see TD8-02's lane) and they carry no lib-target lints, so nothing else is hidden today.
- **Evidence**:
  ```
  error: using `chunks_exact` with a constant chunk size
    --> crates/plugin/src/esm/records/movt.rs:37:22
  37 |                     .chunks_exact(4)
     = note: `-D clippy::chunks-exact-to-as-chunks` implied by `-D warnings`
  ```
- **Impact**: CI `Test + Check + Clippy` fails on every push until fixed; the gate was at zero after 3c9949ec9 (2026-11-14).
- **Related**: TD8-02 (same commit), #6222, #6582, #5308.
- **Suggested Fix**: `record.speeds = sub.data.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();` (the form 3c9949ec9 used for the other 140 sites).
- **Re-checked by the orchestrator at 995ed089b**: `movt.rs:37` still reads `.chunks_exact(4)`; the gate is still red.

##### TD8-2026-11-15-02: The non-blocking `--all-targets` lane is red: four deny-level `approx_constant` errors stop `byroredux-core`'s lib-test target linting, plus one `field_reassign_with_default`
- **Severity**: LOW
- **Dimension**: 8 (clippy lane)
- **Location**: `crates/core/src/character/stance.rs:157-158` (four errors), `crates/scripting/src/stance.rs:122-123` (one warning); the `movt.rs:37` warning of TD8-01 repeats here
- **Age**: all from `49c92afbc` 2026-11-15
- **Effort**: trivial
- **Labels**: tech-debt, test-gap
- **Status**: NEW (class of #6193 / #6492, both CLOSED)
- **Description**: `cargo clippy --workspace --all-targets --keep-going` (no `-D`) exits 101. `byroredux-core` (lib test) fails with four `clippy::approx_constant` errors, which are deny-level and so fail even without `-D`. The test table `skyrim_speeds` writes `1.57, 3.14, 3.14` (rotation speeds) where clippy reads `3.14` as an approximate `PI`. `byroredux-scripting` (lib test) warns `field_reassign_with_default`, where `controls` is built with `PlayerControlState::default()` and then has `sneaking_enabled` assigned.
- **Evidence**:
  ```
  error: approximate value of `f{32, 64}::consts::PI` found
     --> crates/core/src/character/stance.rs:157:105   (also :157:111, :158:101, :158:107)
  warning: field assignment outside of initializer ... crates/scripting/src/stance.rs:123:9
  ```
- **Impact**: core's own unit tests are not clippy-checked until fixed, and the lane's "expected zero" baseline is broken one day after it was re-zeroed.
- **Suggested Fix**: the `3.14` values are Skyrim SPED data, not a PI approximation. Add `#[allow(clippy::approx_constant)] // authored Skyrim SPED values, not an approximation of PI` on `skyrim_speeds`, or write them as `3.140_001` / `std::f32::consts::PI` if the test only checks ratios. For the scripting site use `PlayerControlState { sneaking_enabled: false, ..Default::default() }`.

##### TD8-2026-11-15-03: Four `cfg_attr(not(test), allow(dead_code))` items are production-dead and test-only
- **Severity**: LOW
- **Dimension**: 8
- **Location**: `byroredux/src/cell_loader/lod_coverage.rs:70` (`find_full_detail_overlaps`); `byroredux/src/cell_loader/lod_bands.rs:592` (`select_authored_lod_quads`); `byroredux/src/cell_loader/tree_lod.rs:180,189` (`DtlTreeType`, `parse_treetypes_lst`)
- **Age**: `8e7624256` (lod_coverage), `0419437ab` (lod_bands), `510e7dc4d` (tree_lod), all 2026-11-08
- **Effort**: trivial (small for the tree_lod pair if wiring is chosen)
- **Labels**: tech-debt
- **Status**: NEW (the recipe's own "known" list; none was ever filed)
- **Description**: each is `pub(crate)`, carries `#[cfg_attr(not(test), allow(dead_code))]`, and has no production caller. `grep -rnw` at HEAD:
  - `find_full_detail_overlaps`: 4 hits in `lod_coverage.rs` tests (`:273-285`); production counts via `find_terrain_full_detail_overlaps` (the allow's own comment says so). Doc references: `lod_coverage.rs:19` and a comment at `streaming_helpers.rs:412`.
  - `select_authored_lod_quads`: a one-line wrapper over `select_authored_lod_quads_refining(.., -1, None)`. Callers are 8 tests in `lod_bands.rs` plus `legacy_lod_index_tests.rs:64,171`. Production calls `_refining` (the allow says so).
  - `parse_treetypes_lst` / `DtlTreeType`: callers are `tree_lod_tests.rs:443,449,484` (incl. the A6 corpus test). The struct doc says "nothing draws from them yet", so the FO3 `treetypes.lst` is parsed to a decoded-but-unexplained seven-float row that no draw path reads.
- **Impact**: three of the four allows exist only to silence a warning a `#[cfg(test)]` gate would turn into correct structure. Nothing is broken.
- **Related**: TD8-2026-10-31-01 (#6587) fixed the same pattern on `LootOutcome.stolen`.
- **Suggested Fix**: put `#[cfg(test)]` on `find_full_detail_overlaps` and `select_authored_lod_quads` (the latter's tests can keep calling it). For the treetypes pair either gate the parser under `#[cfg(test)]` with the A6 note kept, or open the follow-up that gives the seven floats a consumer. Drop the three `cfg_attr` lines.
- **Sites added since the skill snapshot (answer to the brief)**: the three `allow(dead_code)` sites added 2026-11-15 are `crates/renderer/src/vulkan/context/mod.rs:2026` (`hot_path_maps_are_fx_hashed`), `crates/renderer/src/vulkan/shadow_filter.rs:583` (`frame_images`) and `byroredux/src/render/skin_offsets_hasher_tests.rs:26` (`skin_offsets_is_fx_hashed`). All are never-called type-ascription guards inside test code (d84a67cd9 #6687, f5210e6e6 #6686) and the allow is correct. The four items above (2026-11-08) are the other recent additions. Other non-test sites checked: `LootSelection::Stack` (`inventory/loot.rs:136`, #4464, documented latent), `INHERIT_MAP` (`env_translate/mod.rs:97`, documented "parsed but unconsumed"), `LightHeader.count` (`buffers.rs:28`, GPU byte-copy), `QueryRead.guard` (`query.rs:26`, lock holder), the three `VF_*` constants in `bs_tri_shape.rs` (schema bits, reason given). All justified.

##### TD8-2026-11-15-04: Twenty-two `pub` items are referenced nowhere in the workspace; ten were reported on 2026-10-12 and #5745 closed without them, twelve were never listed
- **Severity**: LOW
- **Dimension**: 8
- **Age**: ten items from the TD8-2026-10-12-03 list (oldest `f03a7711e`, 2026-04-12); twelve not on that list: `end_journey` `4b71155ca` (2026-11-13), `body_perk_count` `18063a780` (2026-11-08), `request_linear_capture` `75b4024df` (2026-11-02), `run_console_command` `4b99e2393` (2026-10-18); older ones `refracts` `f62d4bd54`, `try_add_to_with_access` `b362e8812`, `resolve_lstring` `f43f84c71`, `enqueue_dds_with_clamp` `7c6c156bc` (2026-04/05)
- **Effort**: trivial
- **Labels**: tech-debt
- **Status**: Regression of #5745 (CLOSED; its Suggested Fix covered these ten and they are still present); #6189 and #6397 are the same pattern and show the sibling deletions keep getting partially done
- **Description**: a whole-repo identifier count (`.rs` under `crates`, `byroredux`, `tools`; comments excluded; tests and examples included) is exactly 1 (the definition) for each item below. Still-live from the 10-12 list: `b_from_base` (`crates/core/src/character/derived.rs:310`), `allocated_count` / `free_list_depth` (`crates/core/src/ecs/resources/skin_slot_pool.rs:365,371`), `requiring_restart` (`crates/core/src/settings/mod.rs:141`), `set_override_str` / `clear_override` / `set_screen` (`crates/menuxml/src/menu.rs:280,287,374`), `count_by_kind` (`crates/spt/src/scene.rs:178`), `is_avm2` (`crates/ui/src/profile.rs:59`), `OBLIVION_WORLDSPACE` (`crates/plugin/src/esm/cell/worldspace.rs:26`). New or never listed:
  - `VulkanContext::request_linear_capture` and `linear_capture_pending` (`crates/renderer/src/vulkan/context/linear_capture.rs:99,119`): the CLI uses `request_linear_capture_at` (`byroredux/src/linear_capture.rs:154,192`), yet the CLI module's own doc (`:8`) names `request_linear_capture` as the renderer entry point. Doc and code disagree.
  - `WaterKind::refracts` (`crates/core/src/ecs/components/water.rs:139`): `docs/engine/watal.md:1094` says the canonical type lets translate "disable rays for opaque waterfalls (`WaterKind::refracts`)". The shader instead branches on the `WATER_WATERFALL` / `WATER_LAVA` kind constants, so the CPU helper restates a decision made elsewhere and nothing calls it.
  - `Scheduler::try_add_to_with_access` (`crates/core/src/ecs/scheduler.rs:546`), `resolve_lstring` (`crates/plugin/src/esm/records/common.rs:129`; only the `_in` variant is called), `enqueue_dds_with_clamp` (`crates/renderer/src/texture_registry/upload.rs:182`), `color_subresource_mips` (`crates/renderer/src/vulkan/descriptors.rs:231`), `DEFAULT_BLOOM_INTENSITY` (`crates/renderer/src/vulkan/bloom.rs:148`), `IDENTITY_BONE_SLOT` (`crates/renderer/src/vulkan/scene_buffer/constants.rs:73`, kept alive by one intra-doc link), `run_console_command` (`crates/scripting/src/obscript_quests.rs:1072`; the `obscript.cmd` command it documents calls `_with_text`), `end_journey` (`crates/scripting/src/created_references.rs:318`), `body_perk_count` (`crates/scripting/src/perk_entry_points.rs:534`).
- **Impact**: dead surface, a few of which document themselves as the entry point (`request_linear_capture`, `refracts`). `end_journey` and `body_perk_count` are under two weeks old and read as speculative accessors.
- **Related**: #5745, #6189, #6397, TD8-2026-10-12-03.
- **Suggested Fix**: delete the twenty-two (the crate-public ones are the skill's "note, do not delete" class, so route `is_avm2`, `set_screen`, `count_by_kind`, `requiring_restart` and the `skin_slot_pool` pair as one decision; the rest are plain deletions). Fix the `byroredux/src/linear_capture.rs:8` doc to name `request_linear_capture_at`. Either delete `refracts` or have the water translate call it so the shader constant and the CPU helper cannot diverge. Not counted here: the bink2 `decode_frame` / `decode_frame_with_maps` / `parse_line_map` / `LineMaps` / `Frame::quadrant_mv` set, which is labelled "Diagnostics (P32)" and serves the research harness that lives in another repository.

##### TD8-2026-11-15-05: Two `_`-prefixed parameters survived refactors: `player_detection_events(_player)` and `Menu::texture(_zoom)`
- **Severity**: LOW
- **Dimension**: 8
- **Location**: `byroredux/src/detection/tick.rs:257` (called at `:119`); `crates/menuxml/src/menu.rs:463` (called at `:418` as `self.texture(assets, filename, *zoom)`)
- **Age**: `995ed089b` 2026-11-15 (M65 Phase 2); `67d9b3b27` 2026-09-18 (the menuxml crate's introduction)
- **Effort**: trivial
- **Labels**: tech-debt
- **Status**: NEW
- **Description**: both are private/crate-private inherent functions (not trait impls), so the underscore is not forced by a signature. `player_detection_events` takes `world` and `_player` but only uses `world` (it filters on `PLAYER_REFERENCE`). `Menu::texture` is a private method of `Menu` that never reads its `zoom`. Both callers pass an argument the callee ignores.
- **Impact**: noise; `_zoom` hints at a zoom-aware texture lookup that does not exist.
- **Suggested Fix**: delete the parameter and the argument at the call site (the skill says delete, do not rename). `detection/` is M65 work another session is still touching; check `git diff HEAD` on that file before editing.

#### Dimension 9: Test Hygiene

##### TD9-2026-11-15-02: three source scans added in the window sit outside every `.claude/issues/<N>/CLASSIFICATION.md` record
- **Severity**: LOW
- **Dimension**: 9 (source-scan tests)
- **Location**: `byroredux/src/detection/sight.rs:170-213` (`the_detection_module_is_the_only_caller_of_the_line_test_and_the_range`); `byroredux/src/detection/tick.rs:413-427` (`get_detection_level_toward_the_player_answers_the_real_level`, last line only); `crates/video/src/lib.rs:654-665` (`every_reader_thread_runs_under_the_decoder_guard`); `crates/audio/src/tests.rs:2256` (`the_movie_hold_skips_sounds_already_stopping`)
- **Age**: `995ed089b` and `49c92afbc` 2026-11-15 (detection); `0565c491d` 2026-11-15 (video); audio earlier in the window
- **Effort**: small
- **Labels**: `tech-debt`, `test-gap`
- **Status**: NEW; belongs in the #6686 record (binary crate, OPEN) for the first two. `video` and `audio` have no record.
- **Description**: `.claude/issues/6686/CLASSIFICATION.md` names neither `detection/sight.rs` nor `detection/tick.rs` (the detection module landed after the record's last pass on 11-10 and f5210e6e6 on 11-15). `sight.rs` is the strongest instance: it walks every `.rs` under `byroredux/src` and `crates/` with `std::fs::read_dir` and asserts `!source.contains(needle)` on whole-file text for `line_of_sight_blocked(`, `config.range(` and `.range(exterior)`. It does not use `byroredux_core::source_scan::production_text`, so a comment, doc line or test body in any crate that spells a needle fails it, and it reads the whole tree on every `cargo test`. This is a "single caller" property, the shape #6685 turned into a `syn` walk (`byroredux/src/acquisition_scan.rs`). `tick.rs` ends a behavioural test with an `include_str!("../boot/world.rs")` substring check that boot installs `detects`. `video` pins "two named reader threads each call `guard_decoder(` within 260 bytes" by splitting `lib.rs` on `thread::Builder::new()`; `audio` slices `world.rs` by `"\n    }\n"` and looks for a `.filter(|sound| !sound.stop_issued)` substring, with the doc stating the runtime form needs an audio device (a legitimate device/data class).
- **Evidence**: `sight.rs:196` `for needle in ["line_of_sight_blocked(", "config.range(", ".range(exterior)"] { if source.contains(needle) {`; `video/src/lib.rs:659-663` `let head = &body[..body.len().min(260)]; assert!(head.contains("guard_decoder("), …)`.
- **Impact**: no escaped defect found. The cost is false failures on a comment, silent blindness if `guard_decoder` is renamed in the reader but still mentioned in the window, and a test that reads about 1,500 source files per run.
- **Related**: #6686, #6685 (`acquisition_scan.rs` precedent), #6687/#6688/#6689 (closed siblings), `crates/core/src/source_scan.rs`.
- **Suggested Fix**: add the four tests to the #6686 record with their class. For `sight.rs`, restrict the walk with `production_text` or move the "single caller" property to a `syn` pass reusing `acquisition_scan`; for `tick.rs`, assert the installed query by running `boot` world construction or drop the substring line; for `video`, replace the 260-byte window with a function-level call (`fn run_reader(...)` that both threads must call).

##### TD9-2026-11-15-03: the `parallel-scheduler`-off lane only compiles the tests; the serial scheduler arm is run by no job
- **Severity**: LOW
- **Dimension**: 9 (feature gates vs CI lanes)
- **Location**: `.github/workflows/ci.yml:205-206` (`cargo check -p byroredux-core --no-default-features --lib --tests`); the serial arm is `crates/core/src/ecs/scheduler.rs:790`
- **Age**: lane added by #5624 (closed)
- **Effort**: trivial
- **Labels**: `tech-debt`, `test-gap`
- **Status**: NEW (narrower than closed #5624, which asked for the arm to be compiled)
- **Description**: every workspace consumer takes `byroredux-core` with default features, so `rayon` is always on and the `#[cfg(not(feature = "parallel-scheduler"))]` arm of `Scheduler::run` (`scheduler.rs:790`) exists only in the lane's build. The lane type-checks it and its tests but does not execute them. `docs/engine/ecs.md` documents the flag as a runtime-disable knob, so the serial arm is a supported mode.
- **Evidence**: `cargo test -p byroredux-core --no-default-features --lib` on a clean HEAD worktree (`CARGO_TARGET_DIR=/mnt/data/tmp/td-target-7`): `test result: ok. 916 passed; 0 failed; 1 ignored`; `ecs::scheduler` alone: 38 passed. So the lane can run today and would be green; it takes no GPU and no game data.
- **Impact**: a regression in the serial arm (ordering, stage barriers, timing resources) passes CI until someone disables the feature.
- **Related**: #5624, `docs/engine/ecs.md`.
- **Suggested Fix**: change the step to `cargo test -p byroredux-core --no-default-features --lib` (keep `--tests` for the compile of integration targets if wanted).

##### TD9-2026-11-15-04: nine data-gated ignores in four crates run in no lane, and one `ui` test skips green under the strict-data gate
- **Severity**: LOW
- **Dimension**: 9 (ignore triage / lanes)
- **Location**: `crates/spt/tests/parse_real_spt.rs:146,174,200,271`; `crates/ui/src/avm1_host/tests.rs:255-262`, `crates/ui/src/host/tests.rs:652,670`; `crates/pex/tests/r5_fidelity.rs:153`; `crates/video/src/lib.rs:819`. Not run by: `real-data-gates.yml` parsers lane (`bsa bgsm sfmaterial hkx facegen menuxml`), the corpus job (nif only), the pin lanes (no row).
- **Age**: various; `video` test 2026-11-12
- **Effort**: small
- **Labels**: `tech-debt`, `test-gap`
- **Status**: NEW. The `video` row is already recorded in `audit-parsers/SKILL.md:85` ("absent from `real-data-gates.yml`"), so it is listed for completeness only.
- **Description**: by crate: `spt` 4 (`parse_rate_*_spt`, `walker_stops_on_true_tlv_boundary`, citing closed #3808/#4122), `ui` 3 (`installed_skyrim_host_calls_are_all_cataloged`, citing closed #2966/#3103; plus two installed-corpus protocol tests), `pex` 1 (`da10_main_door_decompiles_to_the_r5_reference_shape`), `video` 1 (the Bink movie sweep, which also reads its own `BYROREDUX_STEAM_COMMON` instead of the `BYROREDUX_*_DATA` variables). `audio` adds 9 device-gated ignores (hardware, expected). The `spt` file does honour `BYROREDUX_REQUIRE_GAME_DATA` (`parse_real_spt.rs:52-63`). The `ui` catalog test does not: `installed_skyrim_host_calls_are_all_cataloged` re-inlines `/mnt/data/SteamLibrary/...` as its default and, when `BsaArchive::open` fails, prints "skipping" and returns green (`avm1_host/tests.rs:257-263`), the #6184 pattern that #3850 closed elsewhere.
- **Evidence**: `avm1_host/tests.rs:261-264`: `let Ok(archive) = BsaArchive::open(&archive_path) else { eprintln!("skipping: …"); return; };`
- **Impact**: the cited issues are MEDIUM or unlabelled, not HIGH, so this stays LOW; the decoders they cover (SpeedTree TLV walker, AVM1 host catalog, `.pex` decompiler) have no automated real-corpus gate, and a missing archive cannot fail the ui catalog test even under the strict flag (its two siblings `expect` the archive and do fail).
- **Related**: #5905, #6184, #3850, #6182.
- **Suggested Fix**: add `spt ui pex video` to the `parsers` lane crate list, route the `ui` catalog test through `byroredux_plugin::esm::test_paths` (or a `require_or_skip` equivalent that honours `BYROREDUX_REQUIRE_GAME_DATA`), and give `video` the standard `BYROREDUX_*_DATA` variables.

## Deferred

These are gated on **M65 (sneak and detection, #6908 tracker)**, which another session was committing during this audit and is still editing (`byroredux/src/detection/tick.rs` is modified in the working tree at the time of writing):
- **TD8-05**, the `player_detection_events(_player)` half. The `Menu::texture(_zoom)` half is independent and can go now.
- **TD9-02**, the two `detection/` source scans (`sight.rs`, `tick.rs`). The `video` and `audio` rows are independent.
- **TD8-01 / TD8-02** are M65 Phase 1 code, but they break CI for everyone, so they are **not** deferred. If the M65 session is about to touch those files anyway, hand them to it.

## Clean / no finding (condensed; per-dimension detail was in the agent scratch files)

- **Dim 1**
  - Function-size pins hold. `render_one_frame` is exactly at its pin (494, zero headroom).
  - The four match arms over 50 cases are exhaustive one-to-one enum maps.
  - Every `pub use` hub over 20 statements is a pure hub with no functions.
  - Previously split files show no regrowth: `debug-ui/panels/`, `obscript_effects/`, `nif/import/types/`, `audio/`, `save_io/`, `npc_spawn/`, `inventory/`.
  - About 20 never-swept crates (bgsm, bink2, binkaudio, boot-request, debug-protocol, facegen, fsr3-sys, game-detect, hkx, menuxml, mod-runtime, papyrus, pex, settings-io, spt, terrain-height, video, byro-detect, texture-upscale) have no production function over 186 code lines.
- **Dim 2**
  - These single homes are respected: `PassClock` (#6579), `collect_subtree`, `World::init_resource`, the descriptor helpers, `submit_initial_layouts`, `atomic_write`, the DDS BC4 decode, `half_to_f32`, coord conversion, `source_scan`, the `asset_provider` layer and `paths`.
  - The new ESM parsers (`calendar.rs`, `god_rays.rs`, `movt.rs`, `media_location.rs`) use `SubReader` / `CommonNamedFields`.
- **Dim 3**
  - GPU struct sizes the guard cannot see agree with code everywhere: `Vertex` 104, `GpuWaterParams` 368 and six others.
  - No spliced doc block among the window's added items.
  - `ecs.md`, `memory-budget.md`, the `shader-pipeline.md` Set 1 table, README flags, `feature-matrix.md` and the `game-compatibility.md` NIF rates all match code.
- **Dim 4**
  - No skill STALE refs.
  - All 84 `/audit-x Dim N` cross-references resolve.
  - None of the 41 open/tracked issue callouts describes a closed issue as open.
  - The `.agents/skills/` stubs are complete.
  - The audit-safety unsafe census is within tolerance.
- **Dim 5**: zero real markers. The `triangle.frag` MIT / Burley attribution block is intact.
- **Dim 6**
  - No production `unimplemented!` / `todo!()`.
  - All 181 console commands are registered, and all 170 `*_system` fns are scheduled.
  - Every non-test `impl Component` has an insert site.
  - `cxx-bridge` (26 LOC) and `platform` (60 LOC) are unchanged in the window, each with its one known consumer.
- **Dim 7**
  - No bare NIF version literals.
  - Every shader `#define` outside `shader_constants.glsl` is a push-constant alias or a binding index.
  - The window's ESM parsers use named lengths.
  - Hostile-header bounds in `video` / `bink2` / `binkaudio` are named and tested.
- **Dim 8**
  - No `#[deprecated]`, no `// removed:` breadcrumbs, no `_unused` names.
  - Every feature flag has both branches live.
  - Every `allow(dead_code)` outside TD8-03 is justified.
- **Dim 9**
  - The hygiene guards exist and none is ignored.
  - `golden_frames.rs` is wired (`rt-correctness.yml:137`).
  - Every non-default feature state has a CI lane except TD9-03.
  - A vacuous-test sample over the window found none.

## Watch list (cross-dimension)

- **README.md is at exactly 600 lines.** The next added line fails `readme_and_roadmap_hold_their_line_ceilings`. ROADMAP.md is 798 of 800.
- **Dim 1, files nearing 2000 production LOC:** `app_events.rs` (1994) and `app_step.rs` (1935); axes are given above.
- **Dim 1, unpinned functions of 300+ code lines:**
  - `refresh_scene_actor_bindings` 457
  - `apply_buoyancy_with_scratch` 433, parked under #6143
  - `invoke_storage_util_list` 388
  - `capture_entity_projections` 381
  - `assemble_camera_and_lights` 369
  - `papyrus_provider_system` 363
  - `animation_system_inner` 352
  - `volumetrics::dispatch`, newly at 302
- **Dim 1, near the match-arm threshold:** `evaluate_function` in `scripting/src/condition/eval.rs` has 49 arms (86 patterns).
- **Dim 2**
  - **Bink bit readers:** the same window-load body sits in two clean-room crates. They have no dependencies by design, so this is not filed.
  - **BLAS scratch grow:** three sites, with history aligned today.
  - **Settings rows:** studio and game_menu each spell the row layout.
  - **The facegen header guard** is repeated ×3.
  - **The LOD NIF-import prologue** is repeated ×5; fold it into TD2-02.
  - **Leaf-crate little-endian reads:** `no_production_code_reads_le_ints_by_hand` polices only the bin, and the plugin parsers alone have 137 hand reads. Extending the scan to core-dependent crates is the cheap first step.
- **Dim 3**
  - **The `_audit-validate.sh` symbol advisory has 321 entries, and only 2 were real.** Italicising design-sketch names in `docs/engine` would make the next real hit visible.
  - `docs/prd/distant-lod-completion.md:286-289,576` cites tests replaced by `lod_family_table_covers_every_game`; fix it at the next PRD edit.
- **Dim 7**
  - `NS_TO_MS` is declared function-local four times.
  - `GROUND_VERTICES_PER_CELL` / `GROUND_INDICES_PER_CELL` could derive their terrain half from `LAND_GRID_VERTS`.
  - Two identical `CAP: usize = 4096` dedup sets live in `scripting/src/magic.rs` and `perk_entry_points/bodies.rs`.
- **Dim 8**
  - `LootSelection::Stack` (#4464, latent until a selective-take UI exists).
  - `INHERIT_MAP`.
  - `gun_spread_arm_penalty`.
  - The Skyrim `MUST` reader (`music_tracks`, #3816).
  - The eight typed-but-unwired `EsmIndex` maps.
- **Dim 9**
  - 25 test files still re-inline `/mnt/data/SteamLibrary/...` defaults.
  - The `vats.rs:634` ground-truth placeholder stays ignored until #6644 / #6645.

## Not done

- **HISTORY.md reverted-work check** (Dim 3): not run.
- **`docs/legacy/` Gamebryo source-path check** (Dim 3): not run; the source tree is outside the repo.
- **`cargo +nightly clippy -W clippy::cognitive_complexity`** (Dim 1): not run. The brace-matching scanner stood in for it.
