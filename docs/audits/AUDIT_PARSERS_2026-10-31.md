# Parser Discipline Audit: 2026-10-31

**HEAD**: 670f81917 · **Baseline**: `AUDIT_PARSERS_2026-10-27.md` (HEAD fa6be4a6b) · **Audited**: Dim 1 and Dim 4 in depth (the only skill paths with source changes: `crates/hkx/src/behavior.rs`, `crates/bsa/tests/bsa_real.rs`); Dim 5/6 consumer files re-read for the two small `asset_provider` / `npc_spawn/resumable` diffs · **Unchanged since baseline (skimmed)**: Dim 2 (error semantics), Dim 3 (version gating), Dim 6 (I/O and paths), bgsm, sfmaterial, facegen, menuxml, game-detect, `crates/audio/src/fuz.rs`, `asset_provider/{archive,texture,material,loose_mat,...}`, `.github/workflows/real-data-gates.yml`.

Part of `/audit-suite --preset comprehensive`. Run alone in order, no sub-agents, no engine launched, no source edits. Nothing in the tree changed apart from this report.

## Commits since fa6be4a6b on this skill's paths

`git log fa6be4a6b..HEAD` is 136 commits. Only these touch this skill's paths; the rest are M43 gameplay (`crafting`, `lockpick`, `mines`, `attraction`, ...), ESM and NIF work outside it.

| Commit | What landed |
|---|---|
| ec8f8af6f | **Fix #6479** (baseline PAR-D1-01). `machine_weight` now weighs a clip by its triggers (`clip_weight`) and a modifier by its leaf tables and every nested modifier (`modifier_weight`). Adds `a_modifier_list_of_expression_tables_is_weighed_by_what_it_holds`. |
| 90c36f4c5 | **Fix #6517**. The Skyrim LE BSA sweep (`bsa_real.rs`) goes through `data_dir`, so it gets the dev fallback path and the strict lane. Adds the pin `every_game_data_dir_helper_routes_through_data_dir`. |
| 4d33e0365, 9ee802d57, 477124477, 31aca901d, ... | Comment or call-site edits in `asset_provider/{audio,script}.rs` and `npc_spawn/resumable/{prebaked,runtime}.rs`. No parser logic. |

Baseline issues: #6479 (PAR-D1-01) fixed and re-read below. #6480 (PAR-D2-01), #6481 (PAR-D4-01), #6482 (PAR-D4-02) and #6483 (PAR-D6-01) are **still open and still present**; none of their files changed (`git diff fa6be4a6b..HEAD` is empty for `fuz.rs`, `asset_provider/archive.rs`, `corpus_tests.rs`, `parse_all.rs`, `ba2_real.rs`). They are cited, not re-filed.

## Method and evidence

- **Probes.** Two throwaway crates in the session scratchpad (`bwprobe/`, `fo4inl/`), path-depending on `crates/hkx` and `crates/bsa`. They were built into the repo `target/` (`CARGO_TARGET_DIR=<repo>/target`, `CARGO_INCREMENTAL=0`), so there is no private target directory. They left `target/release/bwprobe` and `target/release/fo4inl`.
- **Default lane** (`cargo test -p byroredux-{bsa,bgsm,sfmaterial,hkx,facegen,menuxml,game-detect}`): all green.

| Crate | Unit | Other |
|---|---|---|
| bsa | 114 (+ 11 ignored) | `ba2_real` 1, `bsa_real` 1 (the new #6517 pin), 0 run in `csg_real` |
| bgsm | 35 | `parse_all` 1 |
| sfmaterial | 51 | `header_smoke` 6 |
| hkx | 71 (+10 ignored) | |
| facegen | 31 | |
| menuxml | 62 | |
| game-detect | 60 | |

- **Real-data suites, strict lane** (`BYROREDUX_REQUIRE_GAME_DATA=1 cargo test --release`). Only the two suites whose code changed were run. The rest were not rerun, because their sources are untouched since the 10-27 run.

| Suite | Result |
|---|---|
| hkx `--ignored` (10 tests, both behavior-graph censuses, FO4 clip census, SE spline census, LE parity) | 10 / 10 green |
| bsa `bsa_real` `skyrimle_bsa_corpus_full_extract_zero_errors` | green, 19.5 s. It really ran: the #6517 fix makes it resolve through `data_dir`. |
| bgsm / bsa FO76 | not rerun; red since 10-24 (#6482 open) |
| sfmaterial, facegen, menuxml, rest of bsa | not rerun (sources unchanged) |

- **#6479 re-measured against the real corpus.** `fo4inl` decodes all 328 behaviors of `Fallout4 - Animations.ba2` and inlines each through an archive-backed resolver (same path join as `actor_graph.rs`). With the heavier weights, 52 graphs carry references, **0** graphs end with a reference left un-inlined by the budget, and the worst graph is still `meleebehavior.hkx` at 49 of 64 resolves. The fix does not starve vanilla content.

## Executive Summary

| Severity | NEW | Existing (open) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 1 | 0 |
| LOW | 0 | 4 (#6480 M, #6481 L, #6482 L, #6483 L) |

- #6479 holds as written for every table it names (clip triggers, assign-variable bindings, event ranges, expressions, nested list / modify-once / event-driven modifiers). The 4.9 GiB case from the 10-27 probe is closed.
- **MEDIUM, PAR-D1-02 (new).** The same inline multiplier survives through the one owned table the weigh still skips: a blender child's per-bone-weight binding list (`BoneWeights::bound`). 64 inlines of a ~200 KB sub-behavior cost +1.5 GiB and 2 s on the main thread (measured on an in-memory graph).
- There is no overlap with the suite's other finished audits (§ 6 of the suite rules was re-read before this report was written).

## Findings

### MEDIUM

#### PAR-D1-02: `machine_weight` still ignores a blender child's `BoneWeights::bound` table, so 64 inlines of one bone-binding-heavy sub-behavior are 64 deep copies (+1.5 GiB, 2 s on the main thread).
- **Severity**: MEDIUM (bounded memory amplification reachable from a mod file; not an OOM on the 6 GB RT baseline, but it stalls the main thread and the copies are retained in `ActorGraphCache`)
- **Dimension**: Size Discipline
- **Location**: `crates/hkx/src/behavior.rs`. The weight is `machine_weight` (:889-910), `clip_weight` (:861), `modifier_weight` (:871). The table is `BoneWeights.bound` (:581), filled by `read_bone_weights` (:1285-1307). `for_each_clip_mut` / `for_each_modifier_mut` (:586-613) walk blender children but expose neither the child nor its bone weights.
- **Status**: NEW. A residual of #6479 / #6212 (both closed); the same defect class as baseline PAR-D1-01 through a table #6479 did not enumerate.
- **Description**: #6479 weighs a unit by the leaf tables the decode charged a step each. `read_bone_weights` charges `len` (<= `MAX_BONES` 1024) plus `count` (<= `MAX_VARIABLES` 4096) and then pushes one `(bone, variable)` pair per `boneWeights` binding. Nothing deduplicates the pairs, so one child can own up to 4,096 of them (64 KiB), and that table is deep-copied with the cached graph on every inline. The weigh sees only what `for_each_clip_mut` / `for_each_modifier_mut` visit, so a blender child whose generator is a sequence or a nested blender contributes 0 (and a sequence child contributes 1 through the flattened `content.sequences`), however large its `bound`. The 64-resolve cap is then the only limit on the multiplier.
- **Trigger Input**: a Fallout 4 (packfile v11) or Skyrim sub-behavior with one `hkbBlenderGenerator` of ~390 children that all point at one `hkbBoneWeightArray` (1,024 weights) whose binding set carries 4,096 `boneWeights` bindings (variable index >= 0, `offsetInArrayPlusOne` in 1..=1024).
  - File size: about 160 KiB of bindings plus 390 child entries.
  - Decode cost: about 5,120 steps per child, about 2.0 M in total, which is at the `MAX_GRAPH_STEPS` ceiling (this step arithmetic is derived from `read_bone_weights`, not measured on a real packfile).
  - Weight: about 390, against `MAX_INLINED_WEIGHT` 2 M. A root graph that names it from 64 states inlines it 64 times.
- **Evidence** (probe `bwprobe`: in-memory graph, the real `inline_references`, 64 references to one heavy graph; N children each with 4,096 pairs and a `Sequence` generator):

| Children | Resolved / inlined | One copy | RSS growth | Time |
|---|---|---|---|---|
| 390 | 64 / 64 | 24.4 MiB | +1,567 MiB | 2.0 s |
| 1,000 (beyond the step budget; shows linearity) | 64 / 64 | 62.5 MiB | +4,017 MiB | 5.1 s |

  The 390-child row is what the step budget admits. The 1,000-child row is not reachable from a real packfile; it only shows linear growth.
- **Impact**: `populate_actor_graphs` -> `build` runs on the main thread (`byroredux/src/asset_provider/actor_graph.rs:106-115`). One FO4 mod behavior stalls the engine about 2 s and pins about 1.5 GiB for as long as the actor graph is cached.
- **Related**: #6479, #6212, #6211 (all closed); #6481 (no real-data inline pin).
- **Suggested Fix**:
  - Count `bound.len()` per blender child in `machine_weight`, plus the child itself. This needs a `for_each_blend_child_mut` visitor, or a `Generator::weight()` method that recurses blenders and selectors.
  - Or charge each inline the decode's own `Walk::steps`, recorded on the graph, which covers every table by construction and cannot drift as new readers are added. That is the more robust of the two, because this is the second time a hand-enumerated list missed a table.
  - Either way, deduplicate `bound` by bone at decode (a bone has one driving variable), which also removes the 4,096-entries-for-1,024-bones case.
  - Pin it with the `bwprobe` shape as a default-lane test beside `a_modifier_list_of_expression_tables_is_weighed_by_what_it_holds`.

## Existing findings still open (not re-filed)

| Issue | ID (10-27) | Sev | State at HEAD |
|---|---|---|---|
| #6480 | PAR-D2-01 `.fuz` WAV header from the last xWMA `fmt ` chunk, unchecked `channels * 2` | MEDIUM | `crates/audio/src/fuz.rs` unchanged. Still present. |
| #6481 | PAR-D4-01 no real-data guard pins that vanilla graphs inline under the budgets | LOW | `corpus_tests.rs` unchanged. Re-measured above, now against #6479's heavier weights: 0 graphs skipped, worst 49 / 64. |
| #6482 | PAR-D4-02 FO76 exact pins (bgsm 29,991; BA2 40) red after the 10-24 patch | LOW | `parse_all.rs` and `ba2_real.rs` unchanged. Still red. |
| #6483 | PAR-D6-01 loose-walker global visited set picks a link by `read_dir` order | LOW | `asset_provider/archive.rs` unchanged. Still present. |

Housekeeping: #6479 and #6517 were fixed in code by ec8f8af6f and 90c36f4c5. If their issues are still open at publish time, they can be closed.

## Dimension notes

- **Dim 1 (size).** Guards re-read at HEAD: `capacity_hint`, `inflate_bounded*`, `MAX_ENTRY_COUNT`, the hkx step budget, `MAX_STATIC_CLIP_SAMPLES`, `MAX_TRANSFORM_SAMPLES` and `safe-decode` are unchanged. The only delta is the weigh in `behavior.rs`: PAR-D1-02.
- **Dim 2 (error semantics).** No commits on the readers or the `Err` arms. The `npc_spawn/resumable` edits only drop a string-pool snapshot and gate the facetint path by game.
- **Dim 3 (version gating).** No commits on `archive/open.rs`, `ba2.rs` gates, bgsm, sfmaterial `reader.rs`, hkx `packfile.rs` or facegen.
- **Dim 4 (corpus).** #6517 closes a vacuous-green site: the LE BSA sweep returned `ok` on an absent install before. It now runs, and the new `every_game_data_dir_helper_routes_through_data_dir` pin keeps the other helpers honest. Remaining gaps: FO76 pins (#6482) and the missing inline census (#6481). The `parsers` job in `real-data-gates.yml` is still manual-only with no runner registered.
- **Dim 5 (decode -> consumer).** No change. `asset_provider/script.rs` only gained M43 `install_*` calls.
- **Dim 6 (I/O and paths).** No change (#6483 stands).

## Gate Matrix

| Crate | Size caps | Error policy | Version gate | Corpus sweep (strict lane) | CI lane |
|---|---|---|---|---|---|
| bsa (BSA/BA2/CSG) | `safety.rs` ceilings, `capacity_hint` (unchanged) | short-final-chunk `Ok`, otherwise `Err` (unchanged) | allowlists (unchanged) | LE sweep green and no longer vacuous (#6517); FO76 census red (#6482) | `real-data-gates.yml` `parsers` job, manual only (no runner registered) |
| bgsm | template depth 16 + visited set | `parse_*_diag` everywhere | per-field gates, ceiling 22 | FO76 count pin red (#6482); FO4 green on 10-27 | same |
| sfmaterial | `MAX_NESTING_DEPTH` 64, streamed index | typed join errors | header errors pinned | 5 / 5 on 10-27 | same |
| hkx | step budget per leaf table (#6211); inline weight covers triggers and modifier tables (#6479), **not blender-child bone bindings (PAR-D1-02)** | partial results surfaced | v8 / v11 gates | 10 / 10 today; no inline census (#6481) | same |
| facegen | morph cap, exact size | `Err` | magics | 5 / 5 on 10-27 | same |
| menuxml | include, tile and decoded-texture budgets | warn-once budgets | n/a | 11 / 11 on 10-27 | same |
| game-detect | `MAX_VDF_DEPTH` 32 | malformed input -> `Err` | n/a | n/a | default lane only |
| audio `fuz.rs` (shared) | ffmpeg stdout capped (#6307) | header follows the wrong `fmt ` chunk (#6480) | tag 0x0161 check | none | default lane only |

Suggested next step: `/audit-publish docs/audits/AUDIT_PARSERS_2026-10-31.md`. Label for PAR-D1-02: `animation` + `game:fo4`.
