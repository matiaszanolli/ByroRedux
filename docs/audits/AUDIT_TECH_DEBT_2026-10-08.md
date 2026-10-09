**HEAD**: 00f580e09 · **Baseline**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (HEAD `a2c24b16e`, 116 commits / 531 files / +38 099 −10 082 ago) · **Audited**: all 9 dimensions, delta-first (deep) · **Unchanged since baseline (skimmed)**: none (every dimension's `Paths:` had commits: Dims 1/5/6/8 95, Dim 2 47, Dim 7 35, Dim 3 29, Dim 4 4, Dim 9 3)

# Tech-Debt Audit — 2026-10-08

One agent wrote this report as one leg of `/audit-suite --preset comprehensive`. It used no sub-agents, launched no
engine and changed nothing in the tree. Findings were deduplicated against:

- the 113 open issues in `/tmp/audit/issues.json`;
- every `tech-debt`-labelled issue in any state (500 issues, 17 open);
- closed issues via `gh --search`;
- the sibling `*_2026-10-08.md` reports, plus the doc-rot list the orchestrator handed over (CONC-D3-04, TOOL-CI-01/02,
  NIF-D3-04, NIF-D6-01, PHYS-D2-01, SAFE-D4-01/REN-D5-03, NIFAL-D1-01, GAME-D5-05, UI-D1-02, SAVE-D2-02, CHAR-D4-02,
  PERF-D8-01). None of those is re-filed here.

## Executive Summary

| Severity | NEW | Regression | Existing (grew, not re-filed) |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 0 | 0 | 0 |
| LOW | 9 | 0 | 1 (#5332) |

- **The 10-05 debt list was mostly paid down in the window.**
  - Closed: #5092, #5093 and #5094 (files split), #5096 (per-frame driver size pins), #5097 (single shader-constant table),
    #5100 (source-scan helper), #5308 (pinned toolchain; CI clippy was green until today), #5309 (CELL walker dedup),
    #5311 (physics `world.rs` split), and #5315/#5317/#5318/#5321 (doc fixes).
  - Production files over 2000 LOC dropped from 5 to 3. Two of the three are new crossers (TD1-01, TD1-02); the third is the
    data table that #5097 deliberately made the single home.
- **#5100's consolidation is incomplete (TD2-01).** The fix commit says "the copies delegate or die". At HEAD there are
  still 25 inline production cuts and 2 full brace-stripper copies, and three of the cuts keep the whole-file fallback
  #5100 removed from `anim_convert`.
- **A recurring authoring artefact: collapsed line continuations (TD3-01).**
  - 48 string literals carry runs of 6–18 embedded spaces where a `\` continuation was dropped. They came from 27
    commits between 2026-08-27 and 2026-10-07.
  - 8 of them are production text, including a user-facing CLI warning and a console help string.
  - The cinematic.rs instances the orchestrator flagged are two of these. UI-D1-02 owns only `prepare.rs:79`.
- **Status docs trail the procedures that shipped (TD3-02).** ROADMAP, feature-matrix and npc-spawn-ai-packages.md all say
  the Eat, Sleep and Dialogue procedures have no runtime. Dialogue (force-greet) shipped on 10-07 and Eat/Sleep at HEAD.
- **The split wave rotted skill pathspecs that the validator cannot see (TD4-01).** Three skills' First-step `git log`
  pathspecs name files the splits deleted, so delta scoping reports those dimensions as unchanged.
- **`--all-targets` clippy debt went from 0 to 9 lints (TD8-01)**, all from today's commits. One of them is in a committed
  `//! TEMP:` probe that the #5114 guard does not catch (TD8-02).

## Baseline Snapshot (re-measured at `00f580e09`; `prod_loc` self-test ok)

```text
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 24   (10-05: 22)   15 XXXX + new FO4 `HACK` mnemonic false positive ×2
allow(dead_code):                             34   (10-05: 34)   same set
unimplemented!/todo!():                        0   (10-05: 0)
#[ignore] tests:                             281   (10-05: 265)  +16, all data/device-gated
files >2000 production LOC:                    3   (10-05: 5)    4 retired by splits, 2 new crossers (TD1-01/02)
test files >2000 total LOC:                   69   (10-05: 66)
production fns >200 LOC:                     175   (same scanner on the a2c24b16e tree: 172)
clippy --workspace -D warnings (1.96.0):       1   eat_sleep.rs:54 type_complexity = TOOL-CI-2026-10-08-01
clippy --workspace --all-targets (1.96.0):     9   (10-05: 0) — TD8-01
_audit-validate.sh:                           OK — 0 STALE; 72 basename / 3 skill / 280 docs-engine / 3 tree advisories
```

## Top 10 Quick Wins (trivial / small)

1. **TD4-01**: re-point the pathspecs at `audit-fnv:48`, `audit-performance:149` and `audit-physics:80` to the new
   directories. Rename the guard cited at `audit-tooling:72`.
2. **TD8-01**: fix the 9 `--all-targets` lints (5 in a `pack.rs` test, 1 in a `dialogue_voice.rs` test, 2 in `cond_dump.rs`).
3. **TD8-02**: delete `crates/plugin/examples/cond_dump.rs`, or give it a keeper doc. Add `"temp:"` and `"temp "` to the
   guard's line-start markers, together with #5330's `"scratch"`.
4. **TD3-01**: re-wrap the 8 production literals with `\` continuations. Then sweep the ~40 assertion messages, and add a
   hygiene check for `[^ ] {6,}[^ ]` inside a non-raw literal.
5. **TD3-02**: update the M42 row and the procedure lists to 10 procedures (adding Eat, Sleep and Dialogue). Fix ROADMAP's
   "Phases 0–1" line and index `m42-eat-sleep.sh`.
6. **TD3-03**: re-point the split-wave doc and comment references (physics.md tree, `config.rs`, the npc-spawn and
   per-game-survey actor cites, the exterior-readiness `streaming.rs` cite).
7. **#5332**: trim ROADMAP.md (now 831 lines against the 800 cap).

## Top 5 Medium Investments

1. **TD2-01**: finish #5100. Route the 25 inline cuts through `source_scan::production_text`, and the two stripper copies
   through `source_scan::strip_test_modules`. Then add a hygiene test that bans `find("#[cfg(test)]")` outside
   `crates/core/src/source_scan.rs`.
2. **TD1-01**: split `byroredux/src/components.rs` by domain (render markers / environment resources / audio / AI-animation
   / navmesh).
3. **TD1-02**: split `crates/scripting/src/fragment/effects.rs` into resolvers, the deferred-effect queue and one file per
   applier family.
4. Turn the TD3-01 pattern into a gate, since it has recurred in 27 commits.
5. Extend `_audit-validate.sh` to check paths inside multi-token backtick spans (TD4-01).

---

## Findings

### LOW

### TD1-2026-10-08-01: `byroredux/src/components.rs` crossed 2000 production LOC (1997 → 2035)
- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/components.rs` (2597 total lines, 2035 production)
- **Status**: NEW (first crossing; it was on the 10-05 watch list at 1997)
- **Age**: growth came from `c8c0fe868` (#5306, `LoadingCoverClock`), `c2f28e06c` (#4277), `1162236fc` and `63bf3347f`
  (#3817, `CinematicReAdoption`), all 2026-10-05/06.
- **Effort**: medium
- **Description**: the file was meant to hold "marker components + app resources". It now holds six unrelated domains, in
  contiguous blocks:
  - render and material markers plus texture handles (25–460);
  - terrain components (460–570);
  - cell lighting / weather / sky / cloud resources and their three test modules (570–1640);
  - entity indices (`NameIndex`, `SubtreeCache`, `CellRootIndex`, `CinematicReAdoption`, 1640–1720);
  - input, footstep and water-audio components and resources (1724–1935);
  - tuning resources, then the AI / animation / combat-clip / navmesh block (1940–2360+).
- **Evidence**: `prod_loc byroredux/src/components.rs` → 2035. On the `a2c24b16e` tree it was 1997.
- **Impact**: every gameplay, render or audio change touches one file, which taxes merges.
- **Related**: `byroredux/src/components/game_time.rs` shows the directory already exists.
- **Suggested Fix**:
  - Move the domains into `components/{render,terrain,environment,indices,audio,ai_anim,navmesh}.rs` and re-export them
    from `components.rs`.
  - First update the two self-scans: `components.rs:2555` reads `include_str!("components.rs")` and
    `commands/env_health_tests.rs:466` reads `"../components.rs"` (*feedback_file_split_include_str*).

### TD1-2026-10-08-02: `crates/scripting/src/fragment/effects.rs` crossed 2000 production LOC (1940 → 2018)
- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/scripting/src/fragment/effects.rs` (2018 lines, all production; tests live in `fragment/tests.rs`)
- **Status**: NEW (first crossing; it was on the 10-05 watch list)
- **Age**: `59980e64e` (#5298, 10-05), `9813af435` (#4470/#4415, 10-06) and `f82b4a0de` (#5071, 10-06).
- **Effort**: medium
- **Description**: the file has three layers:
  - target resolvers (`resolve_quest*`, `resolve_object`, `resolve_actor`, `resolve_npc_actor`, …, lines 11–230);
  - the deferred-effect queue (`DeferredFragmentEffects` plus its two enums, the guard-free apply and the poll, 233–620);
  - one applier per effect family: global, inventory, placement, scene, lock, player-control, vehicle/cinematic,
    AI/combat and quest-scoped (713–1856), driven by `apply_effects` (1857).
- **Evidence**: `prod_loc` → 2018. On the baseline tree it was 1940.
- **Suggested Fix**:
  - Split into `fragment/effects/{resolve,deferred}.rs` plus `fragment/effects/apply_<family>.rs`.
  - Repoint the two source scans that read the file whole: `crates/scripting/src/fragment.rs:87` and
    `byroredux/src/boot/schedule/mod.rs:551`.

### TD2-2026-10-08-01: #5100 closed with 25 inline source-scan cuts and 2 full brace-stripper copies still outside `core::source_scan`; three keep the whole-file fallback #5100 removed elsewhere
- **Severity**: LOW. The duplicates share no live divergent bug today: every scanned file's first `#[cfg(test)]` is a
  trailing `mod`.
- **Dimension**: 2 — Logic Duplication
- **Location**:
  - **renderer (21)**:
    - `vulkan/allocator.rs:746,787`
    - `vulkan/context/mod.rs:1890,2093`
    - `vulkan/context/skinned_blas_refit.rs:1121,1429`
    - `vulkan/buffer.rs:2007,2173`
    - `vulkan/image.rs:491`
    - `vulkan/context/dispatch_skin_and_cluster.rs:828,890`
    - `vulkan/skin_compute.rs:1867`
    - `vulkan/device.rs:1332,1435`
    - `vulkan/context/assemble_camera_and_lights.rs:725`
    - `vulkan/context/post_passes.rs:1812,1862,2024,2106,2164`
    - `vulkan/scene_buffer/shader_contract_tests.rs:2922`
  - **bin**:
    - `cell_loader/object_lod.rs:1379`
    - `render/groundcover_hasher_tests.rs:19-25`
    - `cell_loader/unload.rs:1115-1121`
    - `workspace_hygiene_tests.rs:439`
  - **pex**: `decompile/boolean.rs:845`
  - **full stripper copies**:
    - `crates/plugin/src/esm/records/tests.rs:2678` (`strip_test_modules`)
    - `crates/renderer/src/vulkan/context/draw.rs:1388` (`production_lines`)
- **Status**: NEW. It is the residue of CLOSED #5100 (`655b317c9`, 10-06, "one source-scan cut in core; the copies
  delegate or die"). That commit edited `post_passes.rs` yet left five inline cuts in it.
- **Age**: the surviving cuts date from 2026-08-30 (pex) to 2026-10-01 (`draw.rs`, `c57e5cc4a`).
- **Effort**: small
- **Description**:
  - `crates/core/src/source_scan.rs` provides `production_text` (cut at the first `#[cfg(test)]\nmod `) and
    `strip_test_modules`. The latter is brace-matched and skips braces in strings, raw strings, chars and comments.
  - The renderer's `source_scan` re-exports both.
  - Most of the inline cuts use the weaker `find("#[cfg(test)]")`. That needle also truncates at a `#[cfg(test)] fn` or
    `use`, which is exactly the hazard the core doc warns about.
  - `object_lod.rs:1379` (`map_or(src, …)`), `skin_compute.rs:1867` and `device.rs:1435` (`unwrap_or(len)`) keep the
    whole-file fallback. #5100 removed that fallback from `anim_convert` as the #4842 vacuous-guard class.
  - The two stripper copies count `{`/`}` naively, including braces inside string literals. Core's merged matcher was
    written to stop exactly that.
- **Evidence**: `grep -rn 'find("#\[cfg(test)\]")\|split_once("\\n#\[cfg(test)\]' --include='*.rs' crates byroredux tools`
  returns 27 lines outside core. Two of them are `boot/mod.rs:655` (a module-name lister, not a cut) and the hygiene scan
  of files without tests, which is legitimate.
- **Impact**: the skill and `_audit-common.md` tell auditors that the helper has "ONE home". A reader trusting #5100's
  closure will miss these. The next guard that copies a neighbour inherits the weak needle.
- **Related**: #5100 (CLOSED), #4842, #5164.
- **Suggested Fix**:
  - Replace each cut with `crate::source_scan::production_text(src)` (renderer, via its re-export) or
    `byroredux_core::source_scan::production_text`.
  - Replace the two strippers with `strip_test_modules`. `draw.rs` can then count `.lines()` on the result.
  - Add a `workspace_hygiene_tests` case that fails on `find("#[cfg(test)]")` outside `crates/core/src/source_scan.rs`.

### TD3-2026-10-08-01: 48 string literals embed runs of 6–18 spaces from collapsed `\` line continuations; 8 are production text (CLI warning, console help, logs, an ESM error)
- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments (user-visible text hygiene)
- **Location**: the 8 production sites:
  - `byroredux/src/cli_args.rs:101`: the warning printed when `--flag=value` is ignored.
  - `byroredux/src/commands/depth.rs:32`: the `depth.stats` help text shown by `help`.
  - `byroredux/src/scene/nif_loader.rs:1017`
  - `byroredux/src/systems/cinematic.rs:626,631`
  - `crates/bsa/src/archive/open.rs:445`
  - `crates/plugin/src/esm/reader.rs:946`: the decompression-bomb `ensure!` error.
  - `crates/renderer/src/vulkan/acceleration/blas_static.rs:562`: the BLAS-budget `warn!`.

  About 40 more are test assertion messages, for example:
  - `crates/renderer/src/vulkan/acceleration/tests/blas_static_tests.rs:358-383` (×5)
  - `crates/renderer/src/vulkan/bloom.rs:1472,1577,1595`
  - `byroredux/src/scheduler_access_tests.rs:1138`
  - `crates/ui/src/catalog.rs:733-734`
- **Status**: NEW. UI-D1-02 covers only `crates/ui/src/prepare.rs:79`, which is excluded from the counts.
- **Age**: 27 distinct commits from 2026-08-27 to 2026-10-07. The oldest group is `5d42e7226` (#3979, 7 sites). The newest
  are `63bf3347f` (#3817, 4 sites, 10-06) and `b7987d813` (10-07). It is an ongoing authoring pattern, not one bad commit.
- **Effort**: small (mechanical)
- **Description**:
  - A Rust `"…\⏎    …"` continuation strips the newline and the leading whitespace.
  - These literals were instead written, or re-flowed, as a single physical line with the continuation's indentation
    left inside the string.
  - The rendered text reads `stamped onto loaded              exterior cell roots`.
- **Evidence**:
  ```rust
  // byroredux/src/cli_args.rs:101
  "`{found}` was ignored — this CLI takes `{flag} {value}`              (space-separated); the `{flag}=value` form is not recognised"
  // byroredux/src/commands/depth.rs:32
  "Capture the depth buffer and report measured vs analytic depth resolution          (#3308); `depth.stats reversed` decodes a reversed-Z capture"
  ```
  Detection: `grep -rnE '"[^"]*[a-z0-9,;.)\`(-] {8,}[a-z(#\`+—-][^"]*"' --include='*.rs' crates byroredux tools`, then drop
  column-aligned help tables (`byro-dbg` `display.rs`, `nif_stats.rs`, `probe_form.rs`).
- **Impact**:
  - Cosmetic but user-facing: the CLI error, console help and warn-level logs.
  - It also defeats log grepping for the phrase.
  - No gate catches it, so it keeps recurring.
- **Related**: UI-D1-02 (same pattern, `prepare.rs:79`).
- **Suggested Fix**:
  - Re-wrap the 8 production literals with `\` continuations, then the test messages.
  - Add a `workspace_hygiene_tests` scan that flags a non-raw string literal containing `[^ ] {6,}[^ ]`. Exempt the help
    tables, or require them to be column-aligned with a leading two-space indent.

### TD3-2026-10-08-02: ROADMAP / feature-matrix / npc-spawn-ai-packages.md still say Eat, Sleep and Dialogue procedures have no runtime; Dialogue (force-greet) shipped 10-07, Eat/Sleep at HEAD
- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**:
  - `ROADMAP.md:320` (the M42 row: "Seven procedures …"; "Open: the 10 non-locomotion procedures
    (Find/Eat/Sleep/…/Dialogue/UseWeapon)")
  - `docs/feature-matrix.md:101-106` ("The remaining 10 procedures (Find/Eat/Sleep/…/Dialogue/UseWeapon) are parse-only")
  - `docs/engine/npc-spawn-ai-packages.md:172-180` (only seven procedures "have a name and a consumer") and `:518-526`
    ("Seven procedures of ~17 execute … No Find/Eat/Sleep/…/Dialogue … runtime exists anywhere")
  - `ROADMAP.md:79` says "Story Manager (#5366 Phases 0–1, 2026-10-07)", while `:120` says "Phases 0–4 shipped" and #5366
    is CLOSED.
  - The new smoke script `docs/smoke-tests/m42-eat-sleep.sh` is in neither `docs/smoke-tests/README.md` nor the CLAUDE.md
    smoke list. Its siblings `dt1`/`dt2`/`sm1` are in both.
- **Status**: NEW
- **Age**:
  - Dialogue procedure: `14cff35ae` (#5367 Phase F, 2026-10-07). `byroredux/src/systems/forcegreet.rs` is "the Dialogue AI
    package procedure (FO3/FNV `PKDT` procedure 15)". `5a1eecf6b` updated the ROADMAP's dialogue text the same day but not
    the M42 row.
  - Eat/Sleep: `00f580e09` (HEAD). `PROCEDURE_EAT`/`PROCEDURE_SLEEP` are at `crates/plugin/src/esm/records/misc/pack.rs:323,328`,
    dispatched from `byroredux/src/npc_spawn/ai_package.rs` and run by `eat_sleep_system` (Stage PostUpdate).
- **Effort**: trivial
- **Impact**:
  - `feature-matrix.md` is the status floor, and `npc-spawn-ai-packages.md` is the reference the gameplay and scripting audits
    are told to believe.
  - Both now under-state shipped procedures, so an auditor would treat `eat_sleep_system` / `forcegreet_system` as
    unexpected code.
  - The M42 Dialogue entry also contradicts M43's own dialogue text.
- **Related**: #5367 (OPEN, the dialogue-trees tracker).
- **Suggested Fix**:
  - Say "10 procedures" (adding Eat/Sleep v0 and Dialogue/force-greet) in the three docs, and drop Eat/Sleep/Dialogue from
    the "open" lists.
  - Update ROADMAP:79 to Phases 0–4.
  - Index `m42-eat-sleep.sh` in the smoke README and CLAUDE.md.

### TD3-2026-10-08-03: The window's split wave (#5092 / #5093 / #5311) left docs and comments citing deleted files and stale line ranges
- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**:
  - `docs/engine/physics.md:70`: the module tree still lists `world.rs` as "PhysicsWorld resource + KCC move_character /
    cast_ray_down helpers". It is now `world/{mod,queries,recovery}.rs`, and `move_character` is at `world/queries.rs:594`.
  - `crates/physics/src/config.rs:6,19,145` cite `world.rs::PhysicsWorld::move_character` and "`world.rs:285`". The last
    is inside an `assert_eq!` message.
  - `docs/engine/npc-spawn-ai-packages.md:141`: "`crates/plugin/src/esm/records/actor/mod.rs:190-191`, from `PKID`". The
    field and its parse arm moved to `actor/npc.rs:224` / `:637`.
  - `docs/engine/per-game-translation-survey.md:212,241`: link the NPC_ attribute and RACE DATA decoders to
    `records/actor/mod.rs`, which no longer contains them (they are in `npc.rs` / `race.rs` / `class.rs`).
  - `docs/engine/per-game-translation-survey.md:243`: cites `records/actor/mod.rs:1225`. The arm is at `actor/race.rs:415`.
  - `docs/engine/exterior-readiness-plan.md:303`: cites `streaming.rs:719-729`.
  - About 9 code comments still name `streaming.rs` as the exterior loader, which is now `streaming/mod.rs` and
    `streaming/pre_parse.rs`: `crates/plugin/src/esm/cell/mod.rs:427`, `byroredux/src/cell_loader/lod_bands.rs:95`,
    `byroredux/src/cell_loader/references/synth_child.rs:569,573`, `byroredux/src/cell_loader/nif_import_registry.rs:43`,
    `byroredux/src/scene/nif_loader.rs:323`, `byroredux/src/cell_loader/object_lod.rs:1102`,
    `crates/core/src/math/coord.rs:38`.
- **Status**: NEW. This is the same class as OPEN #5313, which holds the previous wave's instances. These are new
  instances from splits that landed after it was filed.
- **Effort**: trivial
- **Description**:
  - `_audit-validate.sh` resolves backticked paths only in skills and `docs/engine`.
  - The physics.md tree row is not backticked, and line-range cites into a file that still exists (`actor/mod.rs`) resolve.
  - So none of the above is gated.
- **Related**: #5313 (OPEN), TD4-2026-10-08-01.
- **Suggested Fix**: re-point each site. For the `streaming.rs` comments, say "the exterior streaming module
  (`streaming/`)".

### TD4-2026-10-08-01: Three skills' First-step `git log` pathspecs name files the split wave deleted, so delta scoping silently reports those dimensions unchanged; audit-tooling cites a renamed guard
- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**:
  - `.claude/commands/audit-fnv/SKILL.md:48` and `.claude/commands/audit-performance/SKILL.md:149`: name
    `byroredux/src/streaming.rs`, deleted by #5092 (`54d713dee`, 10-06).
  - `.claude/commands/audit-physics/SKILL.md:80` (Dim 4 First step): names `crates/physics/src/world.rs`, renamed to
    `world/mod.rs` by #5311 (`12ca34a70`, 10-05). `move_character`, which that dimension is about, now lives in
    `world/queries.rs`.
  - `.claude/commands/audit-tooling/SKILL.md:72`: cites `the_engine_is_invoked_with_the_boot_request_path`. #5294
    (`322c36626`, 10-05) renamed it to `the_engine_is_invoked_with_the_boot_request_path_and_profiles_env`
    (`tools/byro-launcher/src/engine.rs:225`). `_audit-validate.sh` lists it as a skill advisory.
- **Status**: NEW. The performance report notes its own pathspec under "Stale skill premises" but files nothing. The
  validator half is not tracked anywhere.
- **Effort**: trivial
- **Evidence**:
  - `git log --since=2026-10-06T12:00 --format=%h -- byroredux/src/streaming.rs` → 0 commits.
  - The same window on `byroredux/src/streaming` → 1 commit.
  - `_audit-validate.sh` extracts a path only when it starts right after a backtick
    (`grep -noE '\`[A-Za-z0-9_./{},-]+\.(rs|…)'`). A pathspec that is a later token inside a backticked command is never
    checked, so the gate printed `OK: all path references valid`.
- **Impact**:
  - `_audit-common.md` delta rule 2 lets a dimension with no commits get a one-line skim. Physics Dim 4, FNV's streaming
    dimension and Performance Dim 7 can now be skimmed while their code changes.
  - This is the "stale baseline misdirects the next audit" class. It is held at LOW because no audit has yet been shown to
    skip real changes this way: today's physics and performance auditors noticed the splits.
- **Related**: TD3-2026-10-08-03; PERF 2026-10-08 "Stale skill premises".
- **Suggested Fix**:
  - Replace the pathspecs with `byroredux/src/streaming` and `crates/physics/src/world` (directories), and rename the
    guard at `audit-tooling:72`.
  - Extend `_audit-validate.sh` to also scan whitespace-separated tokens inside backtick spans that begin with `git log`
    or `--`.

### TD8-2026-10-08-01: `clippy --workspace --all-targets` regressed 0 → 9 lints in three files, all from today's commits; the plugin lib-test target fails, so its downstream test targets go unlinted
- **Severity**: LOW. These are test and example targets, outside the CI gate.
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft (clippy, lower-priority bucket)
- **Location**:
  - `byroredux/src/systems/dialogue_voice.rs:235` (`unusual_byte_groupings`; test; `f8950e7cc`)
  - `crates/plugin/src/esm/records/misc/pack.rs:1020` (`unusual_byte_groupings`), `:1023,1030,1040` (`useless_vec` ×3),
    `:1030` (`needless_borrows_for_generic_args`). All in tests, from `287214103`.
  - `crates/plugin/examples/cond_dump.rs:10` (`format_in_format_args`) and `:13` (`print_literal`), from `00f580e09`.
- **Status**: NEW. The 10-05 baseline measured 0 after #5115.
- **Effort**: trivial
- **Evidence**:
  - Command: `cargo clippy --workspace --all-targets --keep-going -- -D warnings` on the 1.96.0 toolchain.
  - Result: `could not compile byroredux-plugin (lib test) due to 5 previous errors`, `(example "cond_dump") due to 2`,
    `byroredux (bin "byroredux" test) due to 2`. The bin-test count includes the TOOL-CI-01 `type_complexity`.
- **Impact**: #5115 brought this bucket to zero on 10-01. Without a lane it re-accumulates, and a lib-test failure stops
  linting of every dependent test target.
- **Related**: TOOL-CI-2026-10-08-01 (the main-gate red, same day), #5115 (CLOSED), TD8-2026-10-08-02.
- **Suggested Fix**:
  - Apply clippy's suggestions (`0x0000_01AB`, slices for `vec!`, drop the `&`), or delete `cond_dump.rs` (TD8-02).
  - Consider a non-blocking `--all-targets` clippy step, so the bucket stays at zero.

### TD8-2026-10-08-02: `crates/plugin/examples/cond_dump.rs` is a committed `//! TEMP:` probe that the #5114 disposable-example guard does not recognise
- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**:
  - `crates/plugin/examples/cond_dump.rs:1` ("`//! TEMP: dump one PACK's parsed CTDA conditions.`", 19 lines, 6 bare
    `unwrap()`s)
  - The guard: `byroredux/src/workspace_hygiene_tests.rs:55-58` (`LINE_START_MARKERS = ["throwaway", "one-off", "temp scratch"]`)
- **Status**: NEW. The same guard-gap class as OPEN #5330 (the "Scratch:" marker), with a different marker word.
- **Age**: `00f580e09` (HEAD, 2026-10-08), inside the "Eat and Sleep" feature commit.
- **Effort**: trivial
- **Description**:
  - #5114's commit message names "TEMP … not for commit" as one of the variants it was closing.
  - The guard only matches "temp scratch" at the start of a line, or "not for commit" anywhere. "TEMP:" passes.
  - The probe also carries two of the TD8-01 lints.
- **Related**: #5114 (CLOSED), #5330 (OPEN), TD8-2026-10-08-01.
- **Suggested Fix**:
  - Delete the probe. If the dump is worth keeping, fold it into a `cond` console subcommand or a documented example.
  - Add `"temp:"` and `"temp "` to `LINE_START_MARKERS` together with #5330's `"scratch"`.

---

## Existing issues that grew in the window (not re-filed)

| Issue | State | Growth since `a2c24b16e` |
|---|---|---|
| #5332 ROADMAP.md over its 800-line cap | OPEN | 818 → **831** lines |
| #5313 split-left code-comment rot | OPEN | new instances from the 10-05/06 splits are filed separately as TD3-03 |
| #5330 disposable-example guard gap | OPEN | the same gap now also misses "TEMP:" (TD8-02) |

## Watch list (within ~8% of the 2000 production-LOC line)

| File | prod LOC | Δ since `a2c24b16e` |
|---|---|---|
| `crates/nif/src/import/types.rs` | 1997 | +4 |
| `byroredux/src/save_io.rs` | 1964 | +64 |
| `crates/scripting/examples/mq101_conformance.rs` | 1927 | +4 (example) |
| `byroredux/src/env_translate.rs` | 1881 | +38 |
| `crates/renderer/src/vulkan/context/init.rs` | 1877 | 0 |
| `crates/plugin/src/esm/cell/mod.rs` | 1870 | +4 |
| `crates/plugin/src/esm/records/misc/world.rs` | 1851 | +17 |

`crates/renderer/src/shader_constants_data.rs` grew to **2641** production LOC (from 2004). It is not filed:
- #5097 (`3516618b9`) made it the single `SHADER_DEFINES` table that also declares the GLSL header, which is the
  "static table" shape this skill prescribes for data tables.
- A split would undo that fix.

Production functions over 200 lines: 175, against 172 on the baseline tree.
- **Newly over the line**, excluding moves:
  - `material_translate::translate_material_with_provenance` (287, the renamed `translate_material`)
  - `texture.rs::with_one_time_commands_inner` (247, #5270)
  - `resumable/prebaked.rs::advance_prebaked_unit` (225)
  - `misc/dialogue.rs::parse_info` (211, #5295)
  - `boot/schedule/post_update.rs::register_post_update_systems` (204)
- **Grew by 20 or more**:
  - `about_to_wait` 1009 → 1044 (now pinned at 1044 by #5096)
  - `register_update_systems` 613 → 655
  - `register_late_systems` 509 → 537
  - `load_references_budgeted` 538 → 559
  - `refresh_scene_actor_bindings` 228 → 279
  - `condition::evaluate_function` 332 → 354
- `pub use` hubs are unchanged in shape: `core/ecs/components/mod.rs` 44 (+1 `eat_sleep`), scripting `lib.rs` 27 (+1),
  `records/mod.rs` 23.

---

## Per-Dimension Notes (clean areas / what was checked)

- **Dim 1**:
  - `prod_loc` self-test ok.
  - All four 10-05 splits held: `streaming/`, `actor/{npc,race,class,faction}.rs`, `volumetrics/{fog_clusters,combustion}.rs`
    and `physics/src/world/`.
- **Dim 2**:
  - See TD2-01.
  - `dialogue_voice::plugin_file_for` re-implements FormID→plugin-file resolution with the slot order backwards. That is
    **GAME-D2-2026-10-08-02** (it names the duplication too), so it is not re-filed.
  - `EatBehavior`/`SleepBehavior` repeat `TravelBehavior`'s three fields. That is the marker-type idiom, so it is not filed.
  - `eat_sleep.rs` reuses the sandbox seating helpers rather than copying them.
  - The CELL walker dedup (#5309) holds: `CellSubrecordFields` plus `cell/tests/walker_equivalence.rs`.
  - Z-up→Y-up has no reimplementation.
- **Dim 3**:
  - `_audit-validate.sh` reported OK with 0 STALE.
  - Two of the three tree advisories (`CRHoldExpansion`, `CWChangeLocationScenes`) are Skyrim EDIDs in the CLAUDE.md smoke
    blurb, and `cranelift` is a crate name. All three are gate noise.
  - GPU-struct prose outside `pinned_sizes()` matches the layout tests: `GpuWaterParams` 368 B, `Vertex` 104 B, and the
    ground-cover SAFETY sizes.
  - Orchestrator-listed doc rot (CONC-D3-04, NIF-D3-04, NIFAL-D1-01, SAVE-D2-02, CHAR-D4-02) was excluded.
- **Dim 4**:
  - Cross-skill "Dim N" references: the only mismatch is audit-parsers → `/audit-scripting` Dim 8, which is OPEN #5324.
  - "Open #N" callouts on closed issues: only the speedtree #3740 line, which is also #5324.
  - Report newly past 90 days: `AUDIT_RENDERER_2026-07-09` has 0 CRITICAL/HIGH.
  - The `_audit-common.md` placeholder figures are exact: cxx-bridge `lib.rs` 26 lines plus a 9-line C++ stub; platform
    60 LOC.
- **Dim 5**: clean.
  - The 24 hits are: 15 `XXXX`; the new `HACK` hits in `parse_real_esm.rs:5076,5078` (the FO4 story-event mnemonic, a false
    positive); 3 upstream FIXME quotes; the documented `items.rs` TBD; historical `scene.rs:1331`; and the `loading_screen.rs`
    MOD2 TODOs, which are under 30 days old (`e60911864`, 10-02).
  - The shader grep returned 0 hits.
  - The `triangle.frag` MIT/Burley notice (lines 24–43) is intact.
- **Dim 6**: clean.
  - 0 `unimplemented!` / `todo!()`.
  - 51 stub comments (base 50), all descriptive.
  - Of the 179 new `pub` fns, none is production-unreachable. The zero-caller hits are test fixtures, and
    `streaming/pre_parse.rs::parse_model_keys` is `#[cfg(test)]`.
  - Eat, Sleep and Dialogue are dispatched from `npc_spawn/ai_package.rs`.
- **Dim 7**: clean.
  - Hand-written numeric `#define`s outside the generated header: still 2 (`pbr.glsl` `SPECULAR_AA_*`).
  - The EV100 S/K constants (#5254) and `MAX_TRANSMISSION_SELF_SKIPS` (#5368) live in `shader_constants_data.rs`.
  - The new `sky.glsl:429` literal `1.8` (the disc-contrast floor) has its documentation half owned by
    **EXT-D4-2026-10-08-01**.
  - The new ESM length gates are moved code that follows house style.
- **Dim 8**:
  - Main clippy gate: the single `type_complexity` error is TOOL-CI-2026-10-08-01, excluded.
  - `allow(dead_code)`: the same 34 sites. No `#[deprecated]` or `// removed:`. The `_unused` bindings are format byte-skips.
  - The CI `Unused dependencies` job is green.
  - Reasonless `#[allow(clippy::…)]` is at 31, up from 30. The heuristic is a same-line or previous-line comment, so the
    figure is not comparable with the 10-05 report's 86.
  - `cxx-bridge` and `platform` have no commits and no new consumers.
  - CI at HEAD: `Test + Check + Clippy` is red (TOOL-CI-01) and the ABBA detector is red; the latter belongs to the
    concurrency audit.
- **Dim 9**: clean.
  - The 16 new `#[ignore]`s are all data- or device-gated. All 5 non-data-regex hits are real-data gates.
  - The hygiene guards are present and not ignored.
  - `[features]` and the CI lanes are unchanged in kind (`ci.yml` changed only Miri `+nightly` and the Vulkan lane's
    `RUST_LOG`).
  - `golden_frames.rs` and `golden/` are present.

## Deferred

- `byroredux/src/loading_screen.rs:10,47` MOD2 camera-path TODOs: these are LSCR follow-on work. Re-triage them after
  2026-11-01, when they pass the 30-day floor.
- `tree_lod_*` `allow(dead_code)` (`cell_loader/object_lod.rs`): its consumer is open work in #4913.

## Next Step

`/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-10-08.md`

Label mapping:
- TD3-01/02/03 and TD4-01 → `doc-rot`. TD4-01 is audit infrastructure; flag that in the publish summary.
- TD8-02 → `test-gap` (a hygiene-guard gap).
- TD1-01/02, TD2-01 and TD8-01 → `tech-debt` + `bug`. TD1-02 also takes `scripting`.
