# ByroRedux Tech-Debt Audit — 2026-07-16

Comprehensive preset run, audit #13 of 21. Depth: deep (per-instance triage
with concrete fix proposals across all 9 dimensions). Prior report:
`docs/audits/AUDIT_TECH_DEBT_2026-07-05.md`.

## Executive Summary

**40 findings** across 9 dimensions. No CRITICAL. **2 HIGH**, **6 MEDIUM**,
**32 LOW**. Four findings are delta/tracking notes against already-open
issues (`TD1-001`/`#1857`, `TD1-002`/`#1749`, `TD8-004`/`#1761`,
`TD8-005`/`#1762`) rather than new distinct issues — 36 findings are net-new
and unfiled.

**Headline result:** this codebase's debt-hygiene machinery is working.
Dimensions 5 (Stale Markers), 6 (Stub Implementations), and 9 (Test Hygiene)
came back **entirely clean** — zero new findings across all three, with
every prior TD5/TD6/TD9 issue re-verified still fixed and no drift from the
five newly-landed AI-behavior commits (Wander→Patrol, M42.3–M42.8). The
`unimplemented!()`/`todo!()` baseline remains 0 repo-wide.

**One finding is a live, currently-reachable bug, not just maintainability
debt** — flagging it here because Dimension 2 (Logic Duplication) surfaced
it as a divergent-bugfix-history case:

> **TD2-103** (MEDIUM): `finish_partial_import`, the exterior-streaming REFR
> drain, never received the game-aware BSXFlags bit-5 fix that landed in the
> synchronous REFR-import path (commit `6feac029`). FO4/Skyrim exterior
> cells streamed through the async worker path — the default exterior-load
> path — can still silently drop architecture NIFs with bit 5 set.

The two HIGH findings are both **lockstep-drift / self-misdirection risks**,
not immediate breakage:
- **TD4-003**: `audit-scripting/SKILL.md` claims no prior scripting audit
  exists and preloads 7 issue numbers as "known open" that are all closed —
  actively misdirects the dedup step of the very audit type this session
  also ran.
- **TD7-101**: `triangle.frag` hand-writes `INST_RENDER_LAYER_SHIFT`/`_MASK`
  instead of sourcing them from the generated shader-constants header — the
  sole exception among every other renderer numeric-define family, with zero
  lockstep guard test.

**Dimension yield**: Dim 1 (Complexity) 12, Dim 2 (Duplication) 14, Dim 3
(Doc Rot) 2, Dim 4 (Audit-Finding Rot) 5, Dim 5 (Stale Markers) 0, Dim 6
(Stubs) 0, Dim 7 (Magic Numbers) 2, Dim 8 (Dead Code) 5, Dim 9 (Test Hygiene) 0.

### Delta vs 2026-07-05 baseline

| Metric | 07-05 | 07-16 | Delta |
|---|---|---|---|
| TODO/FIXME/HACK/XXX | 17 | 17 | unchanged (17/17 re-verified false positives both times) |
| `allow(dead_code)` | 20 | 20 (18 attrs + 2 prose) | unchanged count; same 2 tracked-stale (#1761/#1762), 0 new |
| `unimplemented!`/`todo!()` | 0 | 0 | unchanged |
| `#[ignore]` tests | 134 | 135 (96 actual attrs) | flat |
| files >2000 LOC | 7 | 7 | **same count, membership turned over** — see below |
| path-validation gate | RED (7 stale refs) | **GREEN** (1062/26) | fixed since 07-05 (TD3-2026-07-05-01 resolved) |

**Oversized-file membership churn**: `crates/nif/src/import/collision.rs`
(was 2587), `byroredux/src/cell_loader/references.rs` (was 2078), and
`byroredux/src/main.rs` (was 2955) all dropped below threshold via splits
landed since 07-05. Three new crossings replaced them: `byroredux/src/npc_spawn.rs`
(2400), `crates/plugin/src/esm/records/misc/ai.rs` (2260),
`crates/nif/src/blocks/shader_tests.rs` (2055) — all driven by the M42
AI-behavior arc and its test volume. `draw.rs` (4265→4732) and `context/mod.rs`
(3348→3533) continue growing past their tracked issues (#1857, #1749).

## Baseline Snapshot (for the next audit to diff)

```
TODO/FIXME/HACK/XXX:    17   (all false positives — protocol XXXX tag / ref-impl FIXME docs / closed-issue breadcrumb)
allow(dead_code):       20   (18 attribute sites + 2 prose mentions; 15 justified, 2 existing-tracked stale #1761/#1762)
unimplemented!/todo!(): 0
#[ignore] tests:        135  (96 actual attributes, all legitimately Vulkan/game-data/benchmark gated)
files >2000 LOC:        7
path gate:              GREEN — 1062 refs / 26 skill files
```

Oversized set (live, today):
```
4732  crates/renderer/src/vulkan/context/draw.rs        (Existing: #1857 / TD1-001, grew +467 since 07-05)
3533  crates/renderer/src/vulkan/context/mod.rs          (Existing: #1749 / TD1-002, grew +185 since 07-05)
2400  byroredux/src/npc_spawn.rs                          (TD1-003, NEW crossing)
2273  crates/nif/src/blocks/particle.rs                   (TD1-004, NEW — 867 LOC is embedded tests)
2260  crates/plugin/src/esm/records/misc/ai.rs             (TD1-005, NEW crossing)
2140  crates/plugin/src/esm/records/actor.rs               (TD1-006, pre-existing, no open split issue)
2055  crates/nif/src/blocks/shader_tests.rs                (TD1-007, NEW crossing, test-only)
```

## Top 10 Quick Wins

1. **TD7-101** (small) — add `INSTANCE_RENDER_LAYER_SHIFT`/`_MASK` to `shader_constants_data.rs`, delete 2 hand-written lines in `triangle.frag`, add 1 lockstep guard test. Closes the one real HIGH-severity risk in this report.
2. **TD4-001/002/004/005** (trivial each) — delete the hardcoded "as of this writing" latest-report pointers and stale issue-status callouts in `audit-audio`, `audit-scripting`, `audit-save`, `audit-speedtree` SKILL.md files; replace with the standard "sort docs/audits/ by date" instruction already used elsewhere.
3. **TD2-104** (trivial, ~15 min) — collapse the duplicate `EMPTY_ABSORBED` static in `load.rs`/`exterior.rs` into one `absorbed_refs_or_empty()` helper in `precombined.rs`.
4. **TD2-107** (trivial, ~15 min) — `compressed_mesh.rs` calls `read_vec4()` instead of hand-rolling the same 6-line read 7×.
5. **TD8-101** (trivial) — remove the unused `log = { workspace = true }` dependency from 7 crates (bgsm, sfmaterial, debug-ui, spt, facegen, pex, papyrus) that never call it.
6. **TD8-103** (trivial) — change two misleading `pub use` re-exports in `npc_spawn.rs` to plain `use`; the "existing call sites" they claim to preserve don't exist.
7. **TD3-102** (trivial) — fix the MB/MiB unit mismatch between two adjacent VRAM-budget comments in `scene_buffer/constants.rs`.
8. **TD2-110** (small, ~15-20 min) — extract `accumulate_efid_efit()` in `misc/magic.rs`; `parse_spel`/`parse_ench` currently duplicate the EFID/EFIT decode verbatim.
9. **TD2-108** (small, ~30 min) — 4 controller parsers should call `NiSingleInterpController::parse()` instead of re-typing its 8-line prologue.
10. **TD1-004** (trivial) — extract `particle.rs`'s 867-line embedded test module into `particle_tests.rs`, mirroring the already-proven `shader.rs`/`shader_tests.rs` split; drops the file under threshold immediately.

## Top 5 Medium Investments

1. **TD2-103** (medium, but time-sensitive — this is a live bug) — thread `bsver` through `PartialNifImport`/the streaming payload chain and extract a shared `build_cached_nif_import()` helper covering the BSXFlags gate + import + BGSM merge, called from both `parse_and_import_nif` and `finish_partial_import`. Closes the silent-architecture-drop gap on the FO4/Skyrim exterior-streaming path.
2. **TD1-006** (medium) — split `records/actor.rs`'s 332-line, 29-arm `parse_npc` into per-data-group helpers (identity/faction, inventory+AI-package, runtime-FaceGen, FO4-FaceGen, actor-values). This is the highest-traffic ESM parser in the codebase and the exact shape that produced the closed #1996 divergent-branch bug.
3. **TD1-005** (medium) — split `records/misc/ai.rs` (2260 LOC, bundles 6 unrelated record families) into `misc/pack.rs`, `misc/quest.rs`, `misc/dialogue.rs`, folding CSTY/IDLE into `misc/character.rs`, matching the established one-family-per-file `misc/` convention every sibling file already follows.
4. **TD1-003** (medium) — extract `npc_spawn.rs`'s 1045-line `spawn_npc_entity` into per-phase helpers (placement root / skeleton / body+head / equipment / idle animation / AI-package gating); lets `spawn_prebaked_npc_entity` share the same helpers instead of maintaining a parallel, already-diverging phase sequence.
5. **TD2-106** (small-medium) — route the 6 REFR/SCOL/LOD placement call sites (2 of them in the same file, `spawn.rs`) through the existing, already-tested `GlobalTransform::compose()` instead of hand-rolling `rot * (scale * local) + pos` at each site; sits on the hot REFR-spawn path.

## Findings

### HIGH

#### TD4-003: audit-scripting SKILL.md's Phase-1 baseline is fully stale — "no prior audit" is false, all 7 preloaded "open" issues are closed
- **Severity**: HIGH
- **Dimension**: 4 (Audit-Finding Rot)
- **Location**: `.claude/commands/audit-scripting/SKILL.md:103,146-152`
- **Status**: NEW
- **Description**: The skill claims "No prior scripting audit exists" — false, six reports exist (`_2026-06-23.md` through `_07-16.md`). It also preloads `#1663, #1664, #1665, #1666, #1667, #1668, #1316` as "known open" scripting-domain issues to dedup against. All seven are CLOSED — the condition evaluator now implements the full 13-function catalog with real match arms, confirmed by `AUDIT_SCRIPTING_2026-07-16.md`.
- **Evidence**: `for n in 1663 1664 1665 1666 1667 1668 1316; do gh issue view $n --json state -q .state; done` → `CLOSED` ×7.
- **Impact**: An agent following the SKILL.md literally skips the "read prior report, diff direction" dedup step (believing this is greenfield) and treats any re-discovered condition-stub behavior as a dedup-skip against a closed issue rather than correctly recognizing fixed code or filing a regression.
- **Related**: Same pattern class as TD4-001/002/004/005.
- **Suggested Fix**: Delete the "no prior audit" sentence and the hardcoded issue-preload list; replace with the standard `_audit-common.md` "read the most recent `docs/audits/AUDIT_SCRIPTING_*.md`, diff direction" instruction.
- **Age**: SKILL.md dates to ~2026-06-23; all 7 issues closed 2026-06-29→07-04, text never updated.
- **Effort**: small

#### TD7-101: triangle.frag hand-writes INST_RENDER_LAYER_SHIFT/_MASK instead of sourcing them from the generated shader-constants header
- **Severity**: HIGH
- **Dimension**: 7 (Magic Numbers & Hardcoded Constants)
- **Location**: `crates/renderer/shaders/triangle.frag:80-81,402`
- **Status**: NEW
- **Description**: `scene_buffer/constants.rs:216-217` defines the authoritative `INSTANCE_RENDER_LAYER_SHIFT`/`_MASK`, whose doc comment explicitly names the fragment shader's debug-viz branch as a consumer — but unlike every sibling `INSTANCE_FLAG_*`/`MAT_FLAG_*`/`MATERIAL_KIND_*` define (all emitted via the generated header and pinned by a `*_match_*` guard test in `shader_constants.rs`), this pair was never added to `shader_constants_data.rs`. `triangle.frag` hand-declares the same two constants instead.
- **Evidence**: `const uint INST_RENDER_LAYER_SHIFT = 4u; const uint INST_RENDER_LAYER_MASK = 0x3u;` at `triangle.frag:80-81`, consumed at `:402` inside the live `DBG_VIZ_RENDER_LAYER` branch (not dead code).
- **Impact**: If `RenderLayer`'s bit-packing ever changes, the Rust and shader sides can silently drift with no compiler or test error — invisible to `cargo test`, only visible as a wrong debug-viz color. Exactly the `feedback_shader_struct_sync.md` lockstep-drift pattern.
- **Related**: `#1190` (the `INSTANCE_FLAG_*` lockstep fix this pair should have followed); `feedback_shader_struct_sync.md`.
- **Suggested Fix**: Add the two consts to `shader_constants_data.rs`, let `build.rs` emit them, delete the 2 hand-written shader lines, add an `instance_render_layer_bits_match_scene_buffer_consts` test mirroring the existing `INSTANCE_FLAG_*` guard.
- **Age**: introduced `088696e9` (2026-05-03), never migrated when siblings got their lockstep test.
- **Effort**: small

### MEDIUM

#### TD2-103: finish_partial_import (exterior-streaming REFR drain) never received the game-aware BSXFlags bit-5 fix — LIVE BUG
- **Severity**: MEDIUM (promoted: divergent bug-fix history + currently-reachable defect, not just duplication)
- **Dimension**: 2 (Logic Duplication)
- **Location**: `byroredux/src/cell_loader/partial.rs:54-59` vs. `byroredux/src/cell_loader/references/import.rs:69-100`
- **Status**: NEW (regression-shaped — the fix in `references/import.rs`, commit `6feac029`, never reached this sibling)
- **Description**: Both functions gate NIF import on BSXFlags bit 5. `references/import.rs` (sync REFR path) carries the fix: on Skyrim+/FO4/FO76/Starfield, bit 5 means `MultiBoundNode`, not "editor marker" — treating it as editor-marker on those games silently drops legitimate architecture (15 FO4 NIFs per the fix commit). `partial.rs` — the main-thread drain for the exterior-streaming worker, reachable on every game's exterior-cell streaming path — still applies the pre-fix unconditional gate. Its `PartialNifImport` struct never gained a `bsver` field, so it's structurally unable to apply the fix even if copied.
- **Evidence**: `references/import.rs:91`: `bsx & 0x20 != 0 && bsver < FALLOUT4`. `partial.rs:54`: `if partial.bsx & 0x20 != 0 { ...skip... }` — unconditional.
- **Impact**: Any FO4/Skyrim exterior cell streamed through the async worker path (the default exterior-loading path) can still silently drop architecture NIFs with bit 5 set.
- **Related**: fix commit `6feac029`; #1215 (zero-contribution warning, closed, also missing from `partial.rs`).
- **Suggested Fix**: Extract a shared `build_cached_nif_import(scene, bsx, bsver, ...)` helper covering the gate + import + BGSM merge + zero-contribution warning, called from both paths; thread `bsver` through `PartialNifImport`/the streaming payload chain.
- **Effort**: medium

#### TD3-101: feature-matrix.md still lists NPC AI/behavior as entirely unstarted despite 7 shipped M42 procedure runtimes
- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `docs/feature-matrix.md:73,172`
- **Status**: NEW
- **Description**: Last touched `1d3190fb` (2026-07-03). Since then, seven M42 AI-package procedure runtimes shipped (Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol, all opt-in via `BYRO_*` env flags) — documented in ROADMAP.md and `docs/engine/npc-spawn-ai-packages.md` (kept current), but not here. The NPC Spawning table's "AI / behavior" row still reads `✗ | ✗ | ✗ | ✗`; the "What Doesn't Work Yet" table still lists the whole category as a live gap. Same failure mode as 4 already-closed feature-matrix issues (#1699/#1703/#1756/#1818) — this specific gap is new/unreported.
- **Evidence**: `docs/feature-matrix.md:73`: `| AI / behavior | ✗ | ✗ | ✗ | ✗ |`; `ROADMAP.md`'s M42 row documents Guard+Patrol landing 2026-07-16.
- **Impact**: A reader using feature-matrix.md as the "what works today" reference would wrongly conclude NPCs have zero behavior/AI.
- **Related**: #1699, #1703, #1756, #1818 (closed, same file, different rows).
- **Suggested Fix**: Update both rows to `~` (partial) with a footnote naming the 6-7 opt-in-gated procedures and their v0 scope limits (spawn-time-only selection, no per-frame re-evaluation); name the still-genuinely-missing ~10 procedures in the gaps table instead of the whole category.
- **Age**: doc last touched 2026-07-03; drift accumulated over the 07-15/16 M42.3–M42.8 commits.
- **Effort**: small

#### TD4-001: audit-audio SKILL.md's "latest report" pointer is 4 reports stale
- **Severity**: MEDIUM
- **Dimension**: 4 (Audit-Finding Rot)
- **Location**: `.claude/commands/audit-audio/SKILL.md:78-80`
- **Status**: NEW
- **Description**: Hardcodes "the latest is `_2026-07-02.md`" — four newer reports now exist (`_07-03.md`, `_07-14.md`, `_07-16.md`). The instruction to "sort by date, do not hardcode" is violated by the very next sentence.
- **Evidence**: `ls docs/audits/ | grep AUDIT_AUDIO` → 7 files, latest `_2026-07-16.md`.
- **Impact**: A future `/audit-audio` run trusting the prose reads a 2-week-stale baseline.
- **Related**: TD4-002 (same block), TD4-005 (identical pattern in audit-speedtree).
- **Suggested Fix**: Delete the hardcoded filename/supersession list; keep only the "sort by date" instruction.
- **Effort**: trivial

#### TD4-002: audit-audio SKILL.md cites #1859 as "still open" — closed 2026-07-15
- **Severity**: MEDIUM
- **Dimension**: 4 (Audit-Finding Rot)
- **Location**: `.claude/commands/audit-audio/SKILL.md:85-93`
- **Status**: NEW
- **Description**: Tells the next audit agent that a `SoundCache` docstring path is "still open" as AUD-2026-07-02-01/#1859. Fixed in `37394005` (2026-07-14), closed on GitHub 2026-07-15, and `AUDIT_AUDIO_2026-07-16.md` already confirms it FIXED.
- **Evidence**: `gh issue view 1859 --json state,closedAt` → `CLOSED, 2026-07-15T02:29:32Z`; `crates/audio/src/lib.rs:1177` now reads the corrected path.
- **Impact**: A future agent following the prose literally would spend a cycle "confirming" an already-fixed bug.
- **Related**: TD4-001 (same Phase-1 block); commit `37394005`.
- **Suggested Fix**: Replace with a closed-regression-guard note, only re-flag if the path drifts again.
- **Age**: fix landed 2026-07-14; text unmodified since.
- **Effort**: trivial

#### TD4-004: audit-save SKILL.md claims "no prior save audit exists" — 4 reports now exist
- **Severity**: MEDIUM
- **Dimension**: 4 (Audit-Finding Rot)
- **Location**: `.claude/commands/audit-save/SKILL.md:107-109`
- **Status**: NEW
- **Description**: States this would be the first save audit; `AUDIT_SAVE_2026-06-23.md`, `_07-02.md`, `_07-03.md`, `_07-16.md` all exist. Narrower blast radius than TD4-003 (no stale issue-preload list here), but still tells the agent to skip reading prior reports.
- **Evidence**: `ls docs/audits/ | grep AUDIT_SAVE` → 4 files.
- **Impact**: A `/audit-save` run wouldn't diff against 3 existing follow-ups, risking re-filing already-triaged findings.
- **Suggested Fix**: Replace with the standard "read most recent, diff direction" instruction.
- **Age**: first report landed 2026-06-23; text unchanged since.
- **Effort**: trivial

#### TD4-005: audit-speedtree SKILL.md — stale "latest report" pointer AND stale "still-unfiled" claim
- **Severity**: MEDIUM
- **Dimension**: 4 (Audit-Finding Rot)
- **Location**: `.claude/commands/audit-speedtree/SKILL.md:97-103`
- **Status**: NEW
- **Description**: Cites `_2026-07-02.md` as latest (actual latest is `_07-16.md`) and calls three findings (SPT-NEW-01/06/07) "still-unfiled" — all three were filed (`#1820`/`#1821`/`#1822`) and two are now closed (`#1820` fixed 2026-07-04, `#1821` fixed 2026-07-04/07-16). Only `#1822` remains open.
- **Evidence**: `for n in 1820 1821 1822; do gh issue view $n --json state -q .state; done` → `CLOSED CLOSED OPEN`.
- **Impact**: An agent told these are "still-unfiled" would waste effort re-deriving/re-filing two already-closed findings, or file duplicates.
- **Related**: TD4-001 (identical staleness pattern — likely shared-template origin).
- **Suggested Fix**: Replace with the standard "read most recent, diff direction" instruction; drop the hardcoded finding-status list.
- **Age**: source pair from 07-01/02; issues closed 07-04, re-confirmed 07-16.
- **Effort**: small

### LOW

#### TD1-001: context/draw.rs continues to grow past its tracked issue (4265 → 4732 LOC)
- **Severity**: LOW (tracking note)
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs`
- **Status**: Existing: #1857 (delta note only — do not open a new issue)
- **Description**: File now 4732 LOC (+467 since #1857 filed); `draw_frame` now 1892 LOC (+48). Two more large functions in the same file: `record_skinned_blas_refit` (619 LOC), `record_geometry_pass` (613 LOC), `record_post_passes` (411 LOC).
- **Suggested Fix**: No new proposal — #1857 already covers the split axis (per-pass recording groups). Update #1857's body with current LOC figures.
- **Effort**: n/a (tracking-only)

#### TD1-002: VulkanContext::new() still 1025+ LOC, file grew to 3533 LOC
- **Severity**: LOW (tracking note)
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs`
- **Status**: Existing: #1749 (delta note only)
- **Description**: Constructor now 1046 LOC (+21); `drop` (294 LOC) mirrors the same per-subsystem ordering and would benefit from the same split.
- **Suggested Fix**: Update #1749 with current LOC; consider mirroring the split for `drop` in reverse-teardown order once `new` is split.
- **Effort**: n/a (tracking-only)

#### TD1-003: npc_spawn.rs crossed 2000 LOC — spawn_npc_entity is a 1045-LOC function mixing 6 unrelated concerns
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `byroredux/src/npc_spawn.rs:671-1715` (`spawn_npc_entity`), `:1813-2400` (`spawn_prebaked_npc_entity`)
- **Status**: NEW
- **Description**: Six numbered phases (placement root / skeleton / body / head+hair / equipment / idle animation / AI-package gating) live in one function body, threaded through shared local state. A second function duplicates a parallel, shorter phase sequence for the pre-baked-mesh path — the two have already partially diverged (kf path handles AI-package gating; prebaked path currently does not).
- **Impact**: Any change to one phase requires reviewing the entire 1045-line function for side effects on shared state.
- **Suggested Fix**: Extract each phase into a private helper; let `spawn_prebaked_npc_entity` share the equipment/skeleton helpers instead of re-implementing a parallel list.
- **Age**: file created 2026-04-28, last touched 2026-07-16 — actively growing.
- **Effort**: medium

#### TD1-004: particle.rs crossed 2000 LOC — 867 lines of embedded tests, unlike its shader.rs sibling
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/nif/src/blocks/particle.rs` (production ~1400 LOC, tests 867 LOC)
- **Status**: NEW
- **Description**: Production code is well-organized; the file only trips threshold on test volume. The sibling `shader.rs`/`shader_tests.rs` split already establishes the pattern; `particle.rs` hasn't received it.
- **Suggested Fix**: Extract `mod tests` verbatim into `particle_tests.rs`, mechanical, no logic change.
- **Age**: file created 2026-04-05, last touched 2026-07-06.
- **Effort**: trivial

#### TD1-005: records/misc/ai.rs crossed 2000 LOC — bundles 6 unrelated record families + 1013 lines of tests
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/plugin/src/esm/records/misc/ai.rs`
- **Status**: NEW
- **Description**: Every other `misc/` sibling holds exactly one record family. `ai.rs` bundles PACK (605 LOC, incl. the 7 `active_package_is_*` selectors), QUST (240 LOC), DIAL/INFO/MESG (325 LOC), CSTY, IDLE, plus a combined 1013-line test module.
- **Impact**: A change to quest-stage parsing requires navigating a 2260-line file that also holds the hot, frequently-touched AI-package selector logic.
- **Suggested Fix**: Split into `misc/pack.rs`, `misc/quest.rs`, `misc/dialogue.rs`; fold CSTY+IDLE into `misc/character.rs`.
- **Age**: file created 2026-05-12, last touched 2026-07-16 (same day as npc_spawn.rs).
- **Effort**: medium

#### TD1-006: records/actor.rs crossed 2000 LOC — parse_npc is a 332-line, 29-arm sub-record match
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/plugin/src/esm/records/actor.rs:505-836`
- **Status**: NEW
- **Description**: `parse_npc` interleaves 4+ separately-gated data groups (identity/faction, inventory, runtime FaceGen, FO4 pre-baked FaceGen, actor-value properties) in one 29-arm match — the shape that produced the closed #1996 divergent-branch bug.
- **Impact**: Highest-traffic ESM record parser; every placed NPC touches it.
- **Related**: #1996 (closed) — precedent for why the split has real correctness value.
- **Suggested Fix**: Extract each data group into a `parse_npc_<group>` helper called from a slim dispatch loop; extract the 960-line test module separately.
- **Age**: file created 2026-04-07, last touched 2026-07-15.
- **Effort**: medium (group extraction) + trivial (test split)

#### TD1-007: shader_tests.rs crossed 2000 LOC — test file, lower priority
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/nif/src/blocks/shader_tests.rs`
- **Status**: NEW
- **Description**: Already-split test module (per TD1-004's precedent) has organically grown past 2000 LOC via per-game-era regression tests. Loosely grouped by era in file order already; not disorganized, just accumulated volume. Not on any hot edit path.
- **Suggested Fix**: If/when next touched, split along existing era boundaries (legacy/Skyrim/FO4/FO76/Starfield). Not urgent.
- **Effort**: small, deferrable

#### TD1-008: cell_loader/spawn.rs — spawn_placed_instances is a 1065-line function (81% of the file)
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `byroredux/src/cell_loader/spawn.rs:180-1244`
- **Status**: NEW
- **Description**: Per-REFR mesh-spawn entry point handling placement-root setup and a per-mesh loop (mesh-handle registration, material/texture resolution, skinning, physics/collision, BLAS registration) in one function. File itself is only 1316 LOC, so it's invisible to the file-count discovery command.
- **Suggested Fix**: Split into `spawn_placement_root(...)` + a per-mesh `spawn_mesh_instance(...)` helper.
- **Effort**: medium

#### TD1-009: cell_loader/references/mod.rs — load_references is a 1015-line function (69% of the file)
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `byroredux/src/cell_loader/references/mod.rs:92-1106`
- **Status**: NEW (follow-on to closed #1877, which split the file but left this function monolithic)
- **Description**: The #1877 split reduced the *file* below threshold but not the *function* below the 200-line guidance.
- **Related**: Existing: #1877 (closed, file-size fix) — not a regression, a follow-on.
- **Suggested Fix**: Continue the #1877 split one level deeper — extract per-record-kind dispatch (static mesh / light / door+teleport / precombined-skip).
- **Effort**: medium

#### TD1-010: nif/import/material/walker.rs — extract_material_info_from_refs is a 1008-line function (91% of the file)
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/nif/src/import/material/walker.rs:103-1110`
- **Status**: NEW
- **Description**: The NIFAL material-translation single-sink boundary is essentially one function. Any per-game material quirk fix touches this whole function.
- **Related**: #1454/#1455 (closed BGSM field-forwarding fixes touched this exact path).
- **Suggested Fix**: Split by property-source axis: shader-property / texturing-property / alpha-property extraction / BGSM-BGEM merge, feeding one small aggregator.
- **Effort**: medium

#### TD1-011: esm/records/mod.rs — parse_esm_with_load_order is a 949-line, 110-arm record-type dispatch
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/plugin/src/esm/records/mod.rs:126-1074`
- **Status**: NEW
- **Description**: Large-but-inherent dispatch table over a wire-format tag space, analogous to `blocks/mod.rs::parse_block_inner`. Idiomatic shape, still exceeds the 200-line guidance.
- **Suggested Fix**: Group into per-domain dispatch tables mirroring the `records/{actor,world,misc/*}.rs` split. Low urgency — arm-per-line ratio is loose (~8.6).
- **Effort**: medium, low urgency

#### TD1-012: nif/blocks/mod.rs — parse_block_inner is a 1036-line, 260-arm block-type dispatch
- **Severity**: LOW
- **Dimension**: 1 (File/Function/Module Complexity)
- **Location**: `crates/nif/src/blocks/mod.rs:255-1290`
- **Status**: NEW (reported for completeness per the >50-arm criterion, not neglected debt)
- **Description**: CLAUDE.md itself calls this the "live block-dispatcher arm count" — expected and tracked deliberately. Per-arm logic is minimal (1-4 lines).
- **Suggested Fix**: No action recommended.
- **Effort**: n/a — not recommended for action

#### TD2-101: terrain_lod.rs reimplements the canonical Z-up→Y-up swizzle inline instead of calling zup_to_yup_pos
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:447,464,483-486,500,591,594`
- **Status**: NEW
- **Description**: Third recurrence of the same bug class (#1318 → #1617 → #1753, now `terrain_lod.rs`). The sibling full-detail builder `terrain.rs` was fixed under #1753 and now calls `zup_to_yup_pos`; the LOD variant, whose own comment says it mirrors `terrain.rs`, was never swept.
- **Impact**: Bit-equivalent today, but signals a process gap — new coordinate-conversion sites keep bypassing the canonical helper.
- **Related**: #1318, #1617, #1753/TD2-005 (all closed) — new site not covered by any.
- **Suggested Fix**: Replace manual swizzle literals with `zup_to_yup_pos(...)` calls at all 4 forward sites.
- **Effort**: small

#### TD2-102: DalcCubeYup::from_skyrim_zup hand-derives the same axis-permutation knowledge as zup_to_yup_pos
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `byroredux/src/components.rs:656-682`
- **Status**: NEW (borderline — single call site, well-commented)
- **Description**: Same `(x,y,z)→(x,z,-y)` permutation, applied to named cube faces instead of array indices — not found by grepping `zup_to_yup_pos` callers.
- **Suggested Fix**: At minimum, add a cross-reference comment; full refactor optional.
- **Effort**: trivial (comment) to small (refactor)

#### TD2-104: EMPTY_ABSORBED precombine-absorption fallback duplicated verbatim between interior and exterior cell loaders
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `byroredux/src/cell_loader/load.rs:380-386` vs. `byroredux/src/cell_loader/exterior.rs:415-421`
- **Status**: NEW
- **Description**: Both independently declare an identical `static EMPTY_ABSORBED: OnceLock<HashSet<u32>>` plus conditional.
- **Suggested Fix**: Move to one `absorbed_refs_or_empty()` fn in `precombined.rs`.
- **Effort**: trivial

#### TD2-105: ImportedMesh → Vertex + local-AABB conversion copy-pasted between object_lod.rs and placement_lod.rs
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `byroredux/src/cell_loader/object_lod.rs:264-279,306-315` vs. `placement_lod.rs:444-459,483-491`
- **Status**: NEW
- **Description**: The format-specific streaming logic (`.bto` vs `.lod`) is deliberately separate — that part is fine. This specific mesh-to-vertex/AABB conversion has nothing to do with the format difference and was evidently copied.
- **Suggested Fix**: Extract `imported_mesh_to_vertices()` and `local_aabb_center_radius()` into a shared LOD-support module.
- **Effort**: small

#### TD2-106: Parent-child TRS composition hand-rolled at 6 sites instead of calling GlobalTransform::compose
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `byroredux/src/cell_loader/spawn.rs:480-482,819-821` (duplicated within the same file), `refr.rs:499-501`, `placement_lod.rs:513-515`, plus position-only variants at `spawn.rs:350,422`
- **Status**: NEW
- **Description**: `GlobalTransform::compose` already implements this formula and is well-tested, but none of the REFR/SCOL/LOD placement sites use it.
- **Impact**: Sits on the hot REFR-spawn path; a future composition-order fix would need 6 hand-applications.
- **Suggested Fix**: Call `GlobalTransform::compose` directly where available, or add a `compose_trs()` free function for loose-component callers; route all 6 sites through it.
- **Effort**: small-medium

#### TD2-107: bhkCompressedMeshShapeData reimplements read_vec4 inline 7× instead of calling the collision module's shared reader
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `crates/nif/src/blocks/collision/compressed_mesh.rs` (7 sites) vs. `collision/mod.rs:83-90`
- **Status**: NEW
- **Description**: 6 of 9 collision siblings correctly reuse `read_vec4`; `compressed_mesh.rs` never imports it.
- **Suggested Fix**: `use super::read_vec4;`, replace all 7 sites.
- **Effort**: trivial

#### TD2-108: NiSingleInterpController prologue reimplemented inline at 4 sites instead of calling NiSingleInterpController::parse
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: canonical `controller/mod.rs:253-267`; duplicated at `controller/shader.rs:56-63,180-186,212-219`, `controller/mod.rs:594-600`
- **Status**: NEW
- **Description**: Two family siblings correctly call the shared parser; 4 others re-type the identical 8-line prologue.
- **Suggested Fix**: Call `NiSingleInterpController::parse(stream)?` and destructure at each of the 4 sites.
- **Effort**: small

#### TD2-109: misc/world.rs's parse_acti/parse_term hand-roll the EDID/FULL/MODL/SCRI(/VMAD) bundle that CommonNamedFields::from_subs already centralizes
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `crates/plugin/src/esm/records/misc/world.rs:283-310,347-376` vs. `common.rs:268-296`
- **Status**: NEW
- **Description**: `CommonNamedFields::from_subs` is used correctly at 9 other sites; `misc/world.rs` reimplements it byte-for-byte, including the VMAD decode call.
- **Suggested Fix**: `let common = CommonNamedFields::from_subs(subs);` in both functions, keep only the ACTI/TERM-specific arms.
- **Effort**: small

#### TD2-110: EFID/EFIT → MagicEffectItem decode duplicated verbatim between parse_spel and parse_ench
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `crates/plugin/src/esm/records/misc/magic.rs:522-544,680-702`
- **Status**: NEW
- **Description**: Both implement an identical ~20-line manual decode, despite the same file having a schema-decoder convention used immediately adjacent.
- **Suggested Fix**: Extract `accumulate_efid_efit()`, call from both.
- **Effort**: small

#### TD2-111: CTDA→ConditionList push-and-remap triplet copy-pasted at 4 sites across two files
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `misc/ai.rs:603-607,842-848,1018-1023`, `misc/magic.rs:438-444`
- **Status**: NEW
- **Description**: `parse_ctda` itself is correctly centralized; the 3-statement wrapper (parse → remap → push) is what's duplicated.
- **Suggested Fix**: Add a `ConditionList::push_ctda_sub()` helper next to `parse_ctda`/`remap_condition_form_ids`.
- **Effort**: small

#### TD2-112: GENERAL→GENERAL compute-write-to-shader-read ImageMemoryBarrier hand-rolled 7× across compute passes
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `svgf.rs:1227,1304`, `taa.rs:784`, `caustic.rs:908`, `volumetrics.rs:927,977`, `water_caustic.rs:371`
- **Status**: NEW
- **Description**: `descriptors.rs` has a helper for the UNDEFINED→GENERAL init shape but nothing for this write→read shape; all 7 sites build the byte-identical struct.
- **Related**: #1751/TD2-002, #1752/TD2-003-004 (closed, fixed adjacent duplication, didn't cover this barrier shape).
- **Suggested Fix**: Add `image_barrier_general_write_to_read(image)` to `descriptors.rs`, swap all 7 sites; stage masks stay caller-owned (they legitimately vary).
- **Effort**: small

#### TD2-113: Compute-pipeline-create helper (#1751) never migrated to caustic.rs/taa.rs/svgf.rs/compute.rs
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `caustic.rs:374`, `taa.rs:371`, `svgf.rs:587,934`, `compute.rs:185`
- **Status**: NEW-scoped extension of #1751 (CLOSED), not a regression — those four files were out of scope for the original fix.
- **Description**: `caustic.rs`/`taa.rs`/`svgf.rs` keep the shader module on their pipeline struct (freed later in `destroy()`), unlike the helper's immediate-free semantics — that lifecycle difference is why they weren't swapped originally. `compute.rs` already matches the helper's semantics exactly.
- **Suggested Fix**: Migrate `compute.rs` first (trivial). For the other three, removing the `shader_module` struct field is a small, isolated refactor per file.
- **Effort**: `compute.rs` trivial; others small each

#### TD2-114: Bindless COMBINED_IMAGE_SAMPLER WriteDescriptorSet hand-rolled 2× in texture_registry.rs
- **Severity**: LOW
- **Dimension**: 2 (Logic Duplication)
- **Location**: `crates/renderer/src/texture_registry.rs:1165,1200`
- **Status**: NEW (out of scope for closed #1752)
- **Description**: Both sites need `.dst_array_element()`, which the existing `write_combined_image_sampler` helper doesn't expose, so they hand-roll the full builder.
- **Suggested Fix**: Add a `write_combined_image_sampler_at()` variant with an array-element parameter. Low value in isolation — bundle with any other `descriptors.rs` touch (e.g. TD2-112).
- **Effort**: small

#### TD3-102: MaterialTable VRAM-budget comment mixes decimal-MB and binary-MiB arithmetic inconsistently
- **Severity**: LOW
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:114-116,167-169`
- **Status**: NEW
- **Description**: `MAX_INSTANCES` comment uses decimal MB (29.4 MB); the materials-table comment two blocks down computes the same way but the arithmetic is actually binary MiB (4.7 MB where decimal MB would read 4.9 MB). Both struct sizes are correctly pinned; only the comment's unit convention is inconsistent between the two blocks.
- **Impact**: Cosmetic — doesn't change the "well within budget" conclusion.
- **Suggested Fix**: Recompute the materials-table comment in decimal MB: `16384 × 300 B ≈ 4.9 MB per frame × 2 ≈ 9.8 MB total`.
- **Effort**: trivial

#### TD8-004: Dx10Chunk::start_mip / end_mip — allow(dead_code) drift
- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `crates/bsa/src/ba2.rs:148-151`
- **Status**: Existing: #1761 (OPEN) — re-verified, still accurate
- **Description**: `start_mip` is now read (monotonicity check + diagnostic); its `#[allow(dead_code)]` is redundant. `end_mip` is set once at construction and never read — genuinely dead pending #1049 (M40 streaming).
- **Suggested Fix**: Drop the annotation on `start_mip`; keep it on `end_mip` with a comment noting it's write-only until M40 lands.
- **Effort**: trivial

#### TD8-005: RawDependency.name masked-dead field
- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `crates/plugin/src/manifest.rs:69-74`
- **Status**: Existing: #1762 (OPEN) — re-verified, still accurate, unchanged since 2026-04-04
- **Description**: Deserialized from the manifest TOML but dropped immediately; dependency resolution keys exclusively on `uuid`.
- **Impact**: A manifest author's `name = "..."` under `[[dependencies]]` is silently accepted and silently ignored.
- **Suggested Fix**: Wire it into a resolver diagnostic, or delete the field and update the schema doc.
- **Effort**: small

#### TD8-101: log declared as a dependency in 7 crates that never call it
- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `crates/{bgsm,sfmaterial,debug-ui,spt,facegen,pex,papyrus}/Cargo.toml`
- **Status**: NEW
- **Description**: Each lists `log = { workspace = true }` with zero `log::`/`warn!`/`info!`/etc. call sites anywhere in `src/`. Reads like an untrimmed crate-template dependency.
- **Impact**: None at runtime (tiny facade crate) — pure housekeeping.
- **Suggested Fix**: Remove the dependency from the 7 Cargo.tomls. If any plan to add logging soon (facegen/sfmaterial/bgsm parsers are plausible candidates), leave a one-line comment instead of silently deleting.
- **Effort**: trivial

#### TD8-102: spawn_water_plane's blas_specs output parameter is dead inside the function; interior call site fabricates a throwaway Vec just to satisfy it
- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `byroredux/src/cell_loader/water.rs:77-182` (`let _ = blas_specs;`); `byroredux/src/cell_loader/load.rs:416,439`
- **Status**: NEW
- **Description**: Water meshes are deliberately excluded from BLAS/TLAS, so the function discards the parameter unconditionally. Reasonable for the exterior call site (which has a real shared accumulator), but the interior path allocates a `_blas_dummy` Vec purely to have something to pass, never read again.
- **Impact**: None functionally; misleads a future reader into thinking `_blas_dummy` matters.
- **Suggested Fix**: Drop the `blas_specs` parameter from `spawn_water_plane` entirely; update both call sites.
- **Effort**: small

#### TD8-103: npc_spawn.rs's two pub use re-exports claim "existing call sites" that don't exist
- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `byroredux/src/npc_spawn.rs:29-33,431-435`
- **Status**: NEW
- **Description**: Both re-exports (`Gender`, `normalize_mesh_path`) carry comments justifying `pub use` for "existing call sites" that don't exist anywhere in the tree; `byroredux` is a single binary crate with no external consumers by definition.
- **Suggested Fix**: Change both to plain `use`; delete the misleading comments.
- **Effort**: trivial

## Verified Clean (selected highlights — see per-dimension detail in agent transcripts)

- **Dimension 5 (Stale Markers)**: 17/17 markers re-verified false positives (protocol `XXXX` tag, ref-impl FIXME documentation, closed-issue breadcrumb). Zero new markers in the 5 freshly-landed AI-behavior files.
- **Dimension 6 (Stub Implementations)**: `unimplemented!()`/`todo!()`/`panic!("not …)` still 0 repo-wide, confirmed across the same 5 new commits. 45 stub/placeholder-pattern grep hits all read in context as historical narration, intentionally-tracked minimal-decode records, or Vulkan SAFETY-comment false positives.
- **Dimension 9 (Test Hygiene)**: all 96 actual `#[ignore]` attributes triaged individually — every one legitimately gated on Vulkan/GPU, on-disk proprietary game data, or an explicitly-labeled benchmark. `golden_frames.rs` healthy. All "must not regress" tests named by other audit skills confirmed present and un-ignored.
- **Dimension 7**: NIF version gates, Vulkan `MAX_*`/`MIN_*` constants, GPU struct sizes, and frame/ray/cache budgets are all properly named and single-sourced; shader `#define` provenance is clean except for TD7-101.
- **Dimension 2**: texture-upload chains, `WriteDescriptorSet` boilerplate (outside TD2-114's bindless gap), NIF collision shared readers (outside TD2-107), and ESM common-field adoption (outside TD2-109) are all correctly consolidated.
- **Dimension 8**: 15 of 18 `allow(dead_code)` sites justified (RAII guards, GPU byte-copy fields, forward-looking scaffolding); zero `#[deprecated]` items; zero `// removed:` breadcrumbs anywhere in the tree — the project's "delete completely" policy is holding.

## Process Note (not a tech-debt finding)

Commit `c3e09bb5` ("feat: Enhance AI package behavior components and procedure
runtimes for NPCs") touches only 3 `.claude/commands/*.md` files — no engine
code. The commit message doesn't match its diff, likely a mislabeled/squashed
commit from this session. Flagged for awareness; worth a `git log --stat`
sanity check next session.

## Deferred

None. Every finding in this report is actionable now; no in-progress
milestone gates any of them.
