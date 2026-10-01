=== ISSUE 5110 ===
5110 | TD3-2026-09-29-04: game-loop.md's live-schedule table predates M42.10 and the hostility, dialogue and player-body systems | state=OPEN
labels: documentation ecs low tech-debt doc-rot 
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 3 · **Status**: NEW · **Effort**: small · **Kind**: doc-rot
- **Location**: `docs/engine/game-loop.md:136-160` (last edited `d10c84338`, 2026-09-05)
- **Evidence**:
  - The PostUpdate row still reads "opt-in sandbox/wander/travel/follow/escort/guard/patrol systems |
    Environment-gated NPC locomotion experiments".
  - Since M42.10 (09-18) these systems run by default, with `BYRO_NO_AI_LOCOMOTION` as the single
    kill-switch (`boot/schedule/post_update.rs:70-90`, pinned at `schedule/mod.rs:256-292`).
  - The table has no row for `npc_walk_animation_system`, which is registered last by rule.
  - No row covers these window additions:
    - `make_faction_hostility_system` (update.rs:243)
    - `make_npc_combat_ai_system` (:272)
    - `player_body_facing_system` (:586)
    - `make_npc_dialogue_selection_system` (late.rs:428)
    - `fragment_activation_flush_system`
  - These older systems are also absent: `restoration_system`, `equipment_appearance_system`, and the
    `extension_*` dispatch family (late.rs:485).
- **Impact**: `_audit-common.md` lists game-loop.md as a code-verified runtime trace.
- **Suggested Fix**:
  - Rewrite the locomotion row as default-on, with the kill-switch.
  - Add group rows for Update combat/hostility, Late dialogue selection + equipment appearance +
    extension dispatch, and player-body facing.

**Validated at HEAD 9fcfdc3fc**: `docs/engine/game-loop.md` PostUpdate row still reads "opt-in sandbox/wander/…" / "Environment-gated NPC locomotion experiments" while `boot/schedule/post_update.rs:90` enables locomotion unless `BYRO_NO_AI_LOCOMOTION` is set; `grep` finds none of `npc_walk_animation_system`, `faction_hostility`, `npc_combat_ai`, `player_body_facing`, `dialogue_selection`, `equipment_appearance`, `restoration_system` in game-loop.md.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files

=== ISSUE 5111 ===
5111 | TD4-2026-09-29-02: session-close SKILL's README (<120 lines) and ROADMAP (~500) budgets are both broken | state=OPEN
labels: bug low tech-debt 
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 4 · **Status**: NEW · **Effort**: small
- **Location**: `.claude/commands/session-close/SKILL.md:258` and `:317` (last edited `da3a437bf` 09-01)
- **Evidence**:
  - README.md is 533 lines. It was 382 on 08-01, 492 on 09-01 and 527 on 09-15.
  - ROADMAP.md is 759 lines, trimmed from 1657 on 09-22, and still 1.5× its budget.
  - The ritual runs every session (`8b334c102` today) and never enforces either rule.
- **Suggested Fix**:
  - Either trim README to quick start + pointers, or restate the budgets as measured ceilings.
  - Add a `wc -l` check to Step 6.

**Validated at HEAD 9fcfdc3fc**: `.claude/commands/session-close/SKILL.md` still says "README should stay < 120 lines" and "Don't grow ROADMAP past ~500 lines"; `wc -l` gives README.md 533, ROADMAP.md 759.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files

=== ISSUE 5112 ===
5112 | TD4-2026-09-29-03: Two audit skills backtick a nonexistent `triangle_early.frag` | state=OPEN
labels: documentation low tech-debt doc-rot 
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 4 · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Location**: `.claude/commands/audit-performance/SKILL.md:55,128,129`;
  `.claude/commands/audit-renderer/SKILL.md:38,110` (introduced by `9fcfdc3fc`, today)
- **Evidence**:
  - The early-test variant is `triangle.frag` compiled to `triangle_early.frag.spv`
    (`scripts/check-shader-artifacts.sh:59`).
  - `_audit-validate.sh` lists all five as deleted-file basename advisories.
- **Suggested Fix**: write `triangle_early.frag.spv`, or "the early-test variant of `triangle.frag`".

**Validated at HEAD 9fcfdc3fc**: `crates/renderer/shaders/triangle_early.frag` does not exist (only `triangle_early.frag.spv`, built from `triangle.frag` by `scripts/check-shader-artifacts.sh:59`); the bare backticked `triangle_early.frag` is at `.claude/commands/audit-performance/SKILL.md:55,128` and `.claude/commands/audit-renderer/SKILL.md:110` (perf :129 and renderer :38 already say `.spv`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files

=== ISSUE 5113 ===
5113 | TD7-2026-09-29-01: The 8-lane terrain splat budget is a bare `8` in about 17 places; #4056 added a fourth shader loop after #4496 closed | state=OPEN
labels: bug renderer low tech-debt terrain-exterior shaders 
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 7 · **Status**: NEW · **Effort**: small
- **Location**:
  - GLSL:
    - `crates/renderer/shaders/include/bindings.glsl:469-471`
    - `triangle.frag:396/529/662` (`for (uint i = 0u; i < 8u; ++i)`)
    - `groundcover_blade.frag:155` (the same loop, `c14f5361a`, 2026-09-24)
  - Rust: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:13/16/19` (`[u32; 8]` ×3)
  - CPU packer and budget: `byroredux/src/cell_loader/terrain.rs:378-379` (`[Vec<u32>; 8]`), `:468`
    (`8 - base_transition_count.min(8)`), `:935` (`splat1[i - 4]`)
- **Description**:
  - The value comes from `Vertex.splat_weights_0/1` (2 × `[u8; 4]`).
  - #4496 (CLOSED 09-20) pinned the packer's premise but never named the constant. Each new consumer
    re-types `8` and the `i < 4u ? splat0 : splat1[i-4]` split.
  - The GLSL array length and the Rust `[u32; 8]` are tied only through the struct-size pin.
- **Related**: #4496, #4027
- **Suggested Fix**:
  - Add `TERRAIN_SPLAT_LAYERS = 8` and `TERRAIN_SPLAT_LANES_PER_WORD = 4` to `shader_constants_data.rs`,
    emitted to GLSL.
  - Size the `GpuTerrainTile` arrays from it and replace the loops and budget arithmetic.
  - Pin it against `2 * size_of_val(&Vertex.splat_weights_0)`.

**Validated at HEAD 9fcfdc3fc**: bare `for (uint i = 0u; i < 8u; ++i)` at `triangle.frag:396/529/662` and `groundcover_blade.frag:155`; `uint layer*Index[8]` ×3 in `bindings.glsl` `GpuTerrainTile`; `[u32; 8]` ×3 in `scene_buffer/gpu_types.rs`; `[Vec<u32>; 8]` (`terrain.rs:378-379`), `8 - base_transition_count.min(8)` (:468), `splat1[i - 4]` (:935); no `TERRAIN_SPLAT_LAYERS` constant exists.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

=== ISSUE 5114 ===
5114 | TD8-2026-09-29-01: Eleven self-described "Throwaway" probe examples stay committed; the `_tmp_` guard misses `tmp_` | state=OPEN
labels: bug low tech-debt 
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 8 · **Status**: NEW · **Effort**: small
- **Location**:
  - `crates/nif/examples/tmp_fo4_d4_{psglod,lodoverlap,lodsize}.rs` (`f74f8f68a`, 2026-09-12). Header:
    "Throwaway (FO4 audit D4): census …".
  - `crates/bsa/examples/{obl_sweep,probe_substring,probe_extensions}.rs`,
    `crates/plugin/examples/{find_ext_cell,roof_probe,qust_alias_rawdump}.rs`,
    `crates/nif/examples/lod_probe.rs` and `crates/bgsm/examples/dump_bgsm.rs`. These date from
    2026-05-05 → 07-21, and each module doc says "Throwaway" or "One-off diagnostic".
  - 669 LOC in total.
  - Guard: `byroredux/src/workspace_hygiene_tests.rs:26-29`.
- **Evidence**:
  - The guard matches only `starts_with("_tmp_")`.
  - NIFAL reports 09-14, 09-16 and 09-21 each routed the three `tmp_fo4_d4_*` files here. No tech-debt
    report picked them up.
  - `clippy --all-targets` already fails on all three.
  - Each one is an example target that links on every workspace test run, which is the cost #3746
    measured.
- **Related**: #3746, #3150
- **Suggested Fix**:
  - Delete the probes, or give a keeper a real documented purpose.
  - Widen the guard to `tmp_`/`_tmp_` plus a `^//! *(Throwaway|One-off|TEMP scratch)` module-doc check.

**Validated at HEAD 9fcfdc3fc**: all 11 listed example files exist (669 LOC total); `byroredux/src/workspace_hygiene_tests.rs` matches only `name.starts_with("_tmp_")`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

=== ISSUE 5115 ===
5115 | TD8-2026-09-29-02: `clippy --all-targets` debt grew from 698 to 824 sites; 2 new `too_many_arguments` allows lack a reason | state=OPEN
labels: bug low tech-debt esm-plugin 
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW (lower-priority bucket, outside the CI gate) · **Dimension**: 8 · **Status**: NEW
- **Effort**: small (plugin, mechanical) / medium (the rest)
- **Location**: workspace test and example targets; `byroredux/src/cell_loader/terrain.rs:282`,
  `byroredux/src/helpers.rs:94`
- **Evidence**:
  - `cargo clippy --workspace --all-targets --keep-going -- -D warnings` on rustc 1.96 reports 824 unique
    sites (806 errors in 39 targets). The memory note *Clippy --keep-going* measured 698 on 09-16.
  - By crate:
    - plugin 654: 522 `needless_borrows_for_generic_args`, 79 `unnecessary_to_owned`, 46
      `field_reassign_with_default`
    - byroredux/src 90
    - renderer 22
    - nif 18
    - scripting 11
    - ui 9
  - Real signal inside the bucket:
    - An unused `bridge` at `crates/ui/tests/fallout4_hudmenu_protocol.rs:36`.
    - `empty line after doc comment` at `context/geometry_pass.rs:787`, `papyrus/src/parser/script.rs:849`,
      `commands/view.rs:233` and `nif/tests/common/mod.rs:326`. This is the doc-splice class that
      UI-D5-2026-09-29-01 and PHYS-D2-2026-09-29-03 describe.
  - The two allows listed under Location have no reason comment; the other 11 added in the window do.
- **Suggested Fix**:
  - Run `cargo clippy --fix --all-targets -p byroredux-plugin`.
  - Fix the doc-splice lints by hand.
  - Add the two reason comments.
  - Consider a non-blocking `--all-targets` count lane.

**Validated at HEAD 9fcfdc3fc**: `#[allow(clippy::too_many_arguments)]` without a reason comment at `byroredux/src/cell_loader/terrain.rs:282` and `byroredux/src/helpers.rs:94`; the 824-site count comes from the audit's `clippy --all-targets --keep-going` log (`/tmp/audit/tech-debt/clippy_all.log`), not re-run here (publish rules forbid builds).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files

=== ISSUE 5116 ===
5116 | TD9-2026-09-29-02: #2835's FSR bench-report self-test has never run in CI because the shader job's container has no python3 | state=OPEN
labels: bug renderer low tech-debt shaders test-gap 
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 9 · **Status**: NEW · **Effort**: trivial · **Kind**: test-gap
- **Location**: `.github/workflows/ci.yml:98-120`. The job runs in `container: ubuntu:26.04` and its apt
  list is ca-certificates, git, glslang-tools and ripgrep. The failing step is "FSR bench report reads
  both TSV schemas".
- **Evidence**:
  - The HEAD job log shows `python3: not found` → exit 127.
  - The step also failed in the sampled runs from 09-02, 09-09, 09-15 and 09-25.
  - The step was added by `4de5e78ee` (08-14); the container has been in place since `ca7a4e0ea` (07-25).
  - Locally: `ok — fsr_bench_report self-test passed (7 schemas)`.
  - The shader recompile/compare step passes. The job is red only because of this step.
- **Impact**:
  - #2835's guard has had no CI coverage for 6 weeks.
  - Shader parity has been permanently red for a non-shader reason, so a real SPIR-V drift would land
    on an already-red check.
- **Suggested Fix**: add `python3` to the apt list, or move the step into its own job.

**Validated at HEAD 9fcfdc3fc**: `.github/workflows/ci.yml` shader-artifacts job (`container: ubuntu:26.04`) installs ca-certificates, git, glslang-tools, ripgrep only, then runs `python3 scripts/fsr_bench_report.py --self-test`; HEAD run 36609043348 "Shader source/artifact parity" failed with `python3: not found`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix

=== ISSUE 5117 ===
5117 | REG-2026-09-29-01: #4606 was closed on its doc half; the GPU-idle throughput defect is still in the code and has no open issue (regression of #4606) | state=OPEN
labels: bug renderer medium sync performance 
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-09-29.md`

**Regression of #4606** (#4606 is CLOSED; filed as a new issue rather than reopening it).

Filed as a new issue per the audit-publish rule (do not reopen closed issues). #4606's close comment itself says "The wait-narrowing itself remains open perf work gated on that plan — this issue's doc-rot half is closed"; this issue tracks that throughput half.

- **Severity**: MEDIUM
- **Dimension**: Closed-issue verification (GPU pipeline throughput)
- **Location**: `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:59-80`; `crates/renderer/src/vulkan/sync.rs` (rider list)
- **Status**: NEW. #4606 was closed without its fix; this is a candidate to reopen it. It is not a regression, because the fix never landed.
- **Description**: #4606 has two parts: a MEDIUM throughput defect (on a GPU-bound frame, the all-slots fence wait drains the queue before the CPU records anything) and a comment claiming the cost was zero. The close comment says "this issue's doc-rot half is closed" and "the wait-narrowing itself remains open perf work gated on that plan". The issue was closed as COMPLETED anyway. None of the 163 open issues tracks the narrowing. A search of `/tmp/audit/issues.json` for fence/wait titles finds only #3429, which is unrelated.
- **Evidence**: The corrected comment itself says the defect is live:
  > Narrowing to `in_flight[frame]` alone is the fix, but ONLY after every rider on the all-slots wait is migrated per-FIF or defer-destroyed … then validated with `BYRO_VALIDATION=1`.

  The bench numbers it cites give fence_ms / wall_ms of 10.59/13.08 (Prospector TAA), 7.53/10.74 (Whiterun) and 16.12/37.64 (MedTek). Today's performance report still carries a "fence-bound" bench regression (R6a-regress-22) in ROADMAP. Nothing in the issue tracker covers it.
- **Impact**: Most of each GPU-bound frame is a stalled GPU. This is the largest measured frame-time sink, and with no open issue it is invisible to `/audit-performance` dedup and to `/fix-issue` planning.
- **Related**: #4606, #4601 (rider pin), #282, #3442, R6a-regress-22 (ROADMAP)
- **Suggested Fix**: Reopen #4606, or file a successor, for the throughput half. Carry the documented preconditions as acceptance criteria: all riders migrated, the #282 in-buffer barrier, and a validation run on both upscaler modes.

**Validated at HEAD 9fcfdc3fc**: `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:81` still calls `wait_for_fences(&self.frame_sync.in_flight, true, u64::MAX)` over every FIF slot; the comment at :68-70 still says narrowing to `in_flight[frame]` "is the fix"; no open issue tracks the narrowing (live search `all-slots fence wait narrowing`, `fence wait idles GPU`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

=== ISSUE 5118 ===
5118 | REG-2026-09-29-02: `check-playable-smoke-contracts.sh` does not neutralise `BYROREDUX_OBLIVION_DATA`; it runs the real P0 gate locally and then fails | state=OPEN
labels: bug medium tech-debt game:oblivion 
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-09-29.md`

- **Severity**: MEDIUM
- **Dimension**: Guard integrity (smoke-gate contract lane)
- **Location**: `scripts/check-playable-smoke-contracts.sh:37-40` (env override block); `docs/smoke-tests/fixtures/oblivion.env` (`FIXTURE_GATES=(p0-door-interaction)`, `FIXTURE_DATA_DEFAULT=/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data`)
- **Status**: NEW. It is described as unverified in `.claude/commands/audit-runtime/SKILL.md` Dim 1 ("Verify against the script before filing"). This sweep verified it, empirically. No open issue covers it; #4547, which is closed, made the Oblivion route dispatchable but did not touch this script.
- **Description**: The contract loop runs each gate with the Skyrim SE / FNV / FO3 / FO4 data variables pointed at an empty temp dir, and expects exit 77 (SKIP). The Oblivion fixture (`84bbc44ed`, 2026-09-17) declares `p0-door-interaction` and reads `BYROREDUX_OBLIVION_DATA`, which the script never overrides. On any machine with Oblivion at the default path, the "missing-data" probe runs the real gate: `cargo run --release`, the engine, and `byro-dbg`. The gate then passes (exit 0), and the contract reports `FAIL -- p0-door-interaction[oblivion] missing-data path exited 0 instead of SKIP=77`. `set -e` plus `fail` aborts the script there, so every later contract never runs. That includes the #4730 literal-`run_gate` scan, the W1 route checks and the m47 self-test.
- **Evidence**: While verifying #4730, this sweep ran the script as the audit-runtime skill describes it ("runs each gate with data neutralised"):
  - It exited 1 with the FAIL line above.
  - `target/release/byroredux` was rebuilt at 17:39:59, which proves the real gate ran.
  - A re-run with `BYROREDUX_OBLIVION_DATA=<empty dir>` exited 0, and every remaining contract passed, including #4730's.
  - CI is not affected, because the runner has no Oblivion data.
- **Impact**:
  - The script is documented and CI-labelled as data-free, but on a normal dev machine it launches a GPU process. That breaks the project's "don't launch the engine beside the user's instance" rule, and it breaks the no-engine constraint of audit runs such as this one.
  - Every local run reads red.
  - The early abort hides the contracts that come after it.
- **Related**: #3039 (per-fixture SKIP≠PASS contract), #4547, #4730
- **Suggested Fix**: Build the override list from the fixtures, not a hand-kept list. For each `fixtures/*.env`, export its `FIXTURE_DATA_ENV` pointing at `$MISSING_DATA`. Add a self-check that fails when a fixture declares a data variable the loop does not neutralise.

**Validated at HEAD 9fcfdc3fc**: `scripts/check-playable-smoke-contracts.sh` overrides only `BYROREDUX_SKYRIMSE_DATA`, `_FNV_DATA`, `_FO3_DATA`, `_FO4_DATA`; `docs/smoke-tests/fixtures/oblivion.env` declares `FIXTURE_DATA_ENV=BYROREDUX_OBLIVION_DATA` and `FIXTURE_GATES=(p0-door-interaction)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

=== ISSUE 5119 ===
5119 | REG-2026-09-29-03: Two verified fixes have no regression guard (#4607 hot-path hashing, #1042 bare version literals) | state=OPEN
labels: bug nif-parser low performance nif test-gap 
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-09-29.md`

- **Severity**: LOW
- **Dimension**: Guard coverage (hardening gap)
- **Location**: `byroredux/src/render/groundcover.rs` (#4607); `crates/nif/src/blocks/` and `crates/nif/src/version.rs` `bsver` (#1042)
- **Status**: NEW (PARTIAL results for #4607 and #1042)
- **Description**:
  - **#4607**: `render/groundcover.rs` is FxHash end to end. The only std `HashSet` is at `:1013`, inside `#[cfg(test)]`. But unlike `skin_offsets`, `light_history`, `SkinSlotPool` and `rigid_motion_history`, no `*_not_siphash` / `does_not_use_siphash` source-scan test covers the file. The fix commit's only CI edit (`c010c9fb9`) was the unrelated #4603 lock-order lane.
  - **#1042**: the sweep holds. There are 0 bare `NifVersion(0x…)` in non-test `blocks/`, and all 37 bare `bsver <op> N` hits in `crates/nif/src` are comments or strings. No test enforces this.
- **Evidence**: `git grep -n "HashMap\|HashSet" byroredux/src/render/groundcover.rs` finds `:18/89/90/261` (Fx) and `:1013` (test). `git grep -nE "bsver(\(\))?\s*(>=|<=|>|<|==|!=)\s*[0-9]+" crates/nif/src ':!*test*'` returns 37 lines, all in comments or strings.
- **Impact**: A reintroduced std map in the per-frame ground-cover path would break the #2923 hot-path rule, or a new bare BSVER literal would appear, and nothing would fail.
- **Related**: #4607, #2923, #1042, #1336
- **Suggested Fix**: Add a `source_scan::production_text`-based pin for `render/groundcover.rs` next to `skin_offsets_hasher_tests.rs`. Add a NIF-crate source scan that rejects `NifVersion(0x` and bare `bsver` comparisons outside `version.rs`.

**Validated at HEAD 9fcfdc3fc**: `byroredux/src/render/groundcover.rs` uses `FxHashMap`/`FxHashSet` (:18/89/90/261) and std `HashSet` only in test code (:1013), but no `*_not_siphash` / `does_not_use_siphash` scan covers it (`skin_offsets_hasher_tests.rs` does not reference it; the one `include_str!("render/groundcover.rs")` in `app_events.rs:1657` is a scratch-shrink scan); no NIF-crate source scan rejects `NifVersion(0x` or bare `bsver` comparisons.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

