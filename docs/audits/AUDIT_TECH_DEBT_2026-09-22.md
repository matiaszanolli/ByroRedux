# Tech-Debt Audit — 2026-09-22

**HEAD**: `ee6d3fb39` · **Baseline**: `docs/audits/AUDIT_TECH_DEBT_2026-09-21.md` (HEAD `29a130e19`,
same day / 51 commits later) · **Audited**: all 9 dimensions, delta-first (deep) · **Unchanged
since baseline (skimmed)**: none — all 9 dimensions had commits in this delta window (a very
active period: 51 commits, 179 files, +15,890/-1,340 lines, spanning a dozen sibling audits'
`Fix #N` batches plus two new features) and were reviewed in full, not skimmed.

Orchestrator-run (sequential dimensions, no sub-agents, per this run's instructions). Nothing was
fixed inline — everything below is reported, not fixed, per this run's read-only constraint.

## Executive Summary

| Severity | NEW/regression count | Matched to existing OPEN issue |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 2 | 0 |
| LOW | 4 | 0 |

- **The `cargo clippy --workspace -- -D warnings` CI gate is red again** — the third consecutive
  audit day to find it red (09-19 predicted it, 09-21 found it red on rustc 1.96, today finds it
  red again from two of *today's own* fix commits: `#4607`/`#4608` left 3 mechanical clippy errors
  in the `byroredux` bin crate (`hud.rs`, `render/groundcover.rs`). None relate to the prior day's
  menuxml/plugin fixes, which held clean.
- **A regression test silently stopped running.** `5fea8f437` (Fix #4590) inserted a new test
  function between an existing test's doc comment and its `#[test]` attribute, orphaning that
  attribute onto the new function (duplicate) and leaving the old function
  (`adaptation_alpha_is_a_saturating_ramp`, `crates/renderer/src/vulkan/exposure.rs`) with no
  `#[test]` at all — dead code, absent from `cargo test`'s registry, invisible to both a normal
  green test run and the CI clippy gate (which never builds `#[cfg(test)]` code without
  `--all-targets`). The identical bug class was caught and fixed in a *different* file
  (`crates/pex/src/decompile/cfg.rs`) by a cleanup commit **19 minutes earlier the same evening**
  — this instance had no equivalent follow-up.
- **The oversized-file roster improved net**: two files dropped below the 2000-production-LOC
  line via real splits landed today (`groundcover.rs` 2067→1982 via #4568, `scene.rs` 2001→1405
  via #4569 — both close the exact watch-list rows the 09-21 report named), while one file newly
  crossed it (`context/draw.rs` 1985→2019, driven by the new AgX/exposure feature plus two small
  fixes) — net count **3 → 2**. `draw_frame` itself is a chronic regrowth site: six prior CLOSED
  issues (`#1052`/`#1748`/`#1857`/`#2197`/`#2255`/`#3282`) already closed the same "draw_frame
  regrew" finding across 2026-06 through 2026-08.
- **`storage_util.rs` (2320 LOC)**: its tracking issue `#4218` is CLOSED (2026-09-16) but the
  proposed split never landed — the 09-21 report's own dedup check called it "Existing: #4218
  (OPEN)" five days after it had actually closed. The file, and the debt, are unchanged.
- Every other dimension (2, 5, 6, 7) reviewed clean or found only already-fixed-in-delta items —
  see per-dimension detail below. `cargo machete`: no unused dependencies.

## Baseline Snapshot (2026-09-22, full re-measurement, not quoted)

```text
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 22   (09-21: 22 — flat)
allow(dead_code):                             28   (09-21: 28 — flat)
unimplemented!/todo!():                        0   (09-21: 0 — still zero)
#[ignore] tests:                             221   (09-21: 221 — flat, all data-gated on inspection)
files >2000 production LOC:                    2   (09-21: 3 — net -1: draw.rs newly crossed;
                                                     groundcover.rs and scene.rs dropped below)
test files >2000 total LOC:                   53   (09-21: 54 — noise, not tracked further)
```
`prod_loc` self-test: ok. `_audit-validate.sh`: **OK** — all path references valid; 267 advisory
symbols in `docs/engine/*.md` (09-21: 266 — +1, long-standing italicisation backlog, not
investigated further). `gpu_material_size_claims`: 4/4 pass. `no_source_file_frames_the_deleted_classify_pbr_as_live`:
pass. `GpuCamera`/`GpuInstance`/`GpuMaterial` prose figures (368 B / 160 B / 432 B) verified
against every occurrence in `docs/engine/*.md`, `ROADMAP.md`: all current, no drift.

## Top Quick Wins (trivial/small)

1. **`byroredux` bin clippy gate** (TD8-2026-09-22-01, trivial ×3): `hud.rs:689` question-mark
   rewrite, `groundcover.rs:242` derive `Default`, `groundcover.rs:350` drop one `&`. All three
   have a clippy-supplied fix.
2. **Restore the dropped `#[test]`** (TD9-2026-09-22-01, trivial): `exposure.rs:305/347` — delete
   the duplicate attribute + orphaned doc comment, restore `#[test]` on
   `adaptation_alpha_is_a_saturating_ramp`.
3. **Duplicate `use super::*;`** (TD9-2026-09-22-02, trivial): delete the redundant second import
   at `inventory.rs:1363`.
4. **Hierarchy-walk guard consistency** (TD2-2026-09-22-01, trivial-small): apply
   `HierarchyTraversalGuard` consistently across `anim_convert.rs`/`ragdoll.rs`/`water.rs`'s four
   #4572-guarded walks, or add one small `core::ecs` helper bundling guard+visited-set so the next
   walk has one precedent to copy, not three.
5. **`context/draw.rs` split** (TD1-2026-09-22-01, small): extract the exposure/tonemap
   orchestration glue `draw_frame` now inlines into a sibling file on the existing
   construct-vs-record-vs-teardown precedent (or fold into `telemetry.rs`).

## Medium Investments

1. **`storage_util.rs` (2320 LOC, closed-without-fix #4218)** (TD1-2026-09-22-02): the
   declaration-builder-stack vs adaptation-structs vs per-type-command-builders split axis from
   #4218 still stands; the issue closed without the code change landing. Recommend re-filing
   against the live axis rather than relying on the stale closed issue. Effort: medium.
2. **`context/draw.rs` / `draw_frame` chronic regrowth** (TD1-2026-09-22-01): six prior closes of
   the same finding suggest a structural fix (e.g. a lint/CI check that fails when `draw_frame`
   itself, not just the file, exceeds a line budget) would be more durable than the seventh manual
   extraction. Flagging the pattern for a process-level fix, not just the next split.

## Findings

### TD8-2026-09-22-01: `cargo clippy --workspace -- -D warnings` red — 3 errors, all fresh, all in the `byroredux` bin crate
- **Severity**: MEDIUM · **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/hud.rs:689` (`clippy::question_mark`);
  `byroredux/src/render/groundcover.rs:242` (`clippy::derivable_impls`);
  `byroredux/src/render/groundcover.rs:350` (`clippy::needless_borrow`)
- **Status**: NEW
- **Description**: the exact CI gate command (`cargo clippy -p byroredux -j4 -- -D warnings`, no
  `--all-targets`) fails with 3 errors, all introduced by this delta's own fix commits:
  `66471d4553` (Fix #4608, HUD refresh staging) added the question-mark-eligible block;
  `c010c9fb96` (Fix #4607, ground-cover persistent scratch) both added the hand-written
  `impl Default` that clippy wants derived, and — by turning `candidates` into
  `&mut scratch.candidates` — made a three-week-old `&candidates` call site (`fd0cd577ce`,
  2026-09-15, textually unchanged) newly a needless double-reference.
- **Evidence**: `cargo clippy -p byroredux -j4 -- -D warnings` →
  `error: could not compile \`byroredux\` (bin "byroredux") due to 3 previous errors`. Every site
  confirmed to postdate `29a130e19` via `git blame` + `git merge-base --is-ancestor`.
- **Impact**: any contributor on the CI toolchain (1.96.0) gets a red gate on code they didn't
  touch — the same impact class as 09-21's TD8-2026-09-21-01, now recurring a second consecutive
  day from same-day fix commits rather than a toolchain bump.
- **Suggested Fix**: apply clippy's own suggested rewrite at each site. Trivial ×3.
- **Related**: #4607, #4608 (introduced these); TD8-2026-09-21-01 (same gate class, prior cycle,
  independently occurring — not a regression of that fix, different lines).

### TD9-2026-09-22-01: a regression test silently stopped running — `adaptation_alpha_is_a_saturating_ramp`
- **Severity**: MEDIUM · **Dimension**: 9 — Test Hygiene
- **Location**: `crates/renderer/src/vulkan/exposure.rs:296-347`
- **Status**: NEW
- **Description**: `5fea8f437` (Fix #4590, "adapt each exposure slot over its real update
  interval") inserted a new test function's doc-comment + `#[test]` attribute between the
  pre-existing doc-comment and `#[test]` belonging to `adaptation_alpha_is_a_saturating_ramp` and
  that function's signature. The new function (`per_slot_adaptation_runs_at_the_authored_tau`)
  ends up with a duplicated `#[test]` (harmless) and a doc comment that describes the OLD
  function, not itself; `adaptation_alpha_is_a_saturating_ramp` loses its `#[test]` entirely and
  becomes dead code.
- **Evidence**:
  - `cargo clippy -p byroredux-renderer --all-targets -j4 -- -D warnings`: `error: duplicated
    attribute` at `:305`; `error: function \`adaptation_alpha_is_a_saturating_ramp\` is never
    used` at `:347`.
  - `cargo test -p byroredux-renderer --lib exposure:: -- --list` does not list the function —
    confirmed absent from the compiled test registry, not merely silent.
  - Neither a plain `cargo test` (no `-D warnings`) nor the CI's `cargo clippy --workspace --
    -D warnings` (never builds `#[cfg(test)]` without `--all-targets`) surfaces this — both of
    the project's standard safety nets miss this defect class.
  - **Same-night precedent, fixed elsewhere, not here**: `ad96380be` ("Cleanup: fix review
    fallout from the #4600-#4608 batch"), 22:51:49 the same evening, fixed the identical
    duplicate-`#[test]` shape in `crates/pex/src/decompile/cfg.rs` ("Duplicated `#[test]` from
    the #4476 rename"). `5fea8f437` landed 19 minutes later (23:10:06) reproducing the same
    editing mistake in a different file; no equivalent cleanup followed it.
- **Impact**: the disabled test pinned `adaptation_alpha`'s basic shape (0 at dt=0, monotonic,
  saturates at 1, non-positive tau snaps) — a property the new #4590 test does not independently
  re-check (it tests convergence rate at a fixed tau, not the function's general shape). A future
  change to `adaptation_alpha` could silently break monotonicity/boundedness with all other
  exposure tests still green.
- **Suggested Fix**: delete the duplicate `#[test]` + orphaned doc comment at line ~296-305;
  restore `#[test]` immediately above `fn adaptation_alpha_is_a_saturating_ramp()`. Trivial.
- **Related**: `ad96380be` (same class, same evening, fixed in a sibling file), #4590 (introduced
  this instance), TD9-2026-09-22-02 (a lower-stakes sibling of the same editing habit), Dim 8's
  CI-gate blind spot (the mechanism that let this through).

### TD9-2026-09-22-02: duplicate `use super::*;` inside `inventory.rs`'s test module
- **Severity**: LOW · **Dimension**: 9 — Test Hygiene
- **Location**: `byroredux/src/inventory.rs:1323` and `:1363`
- **Status**: NEW
- **Description**: `dcdfafa15f` (Fix #4571) inserted a new `#[cfg(test)] mod tests { use
  super::*; … }` block ahead of pre-existing test content that already opened with its own
  `use super::*;` (blame `09682c71b1`, 2026-08-15) — same module scope now imports the glob
  twice. `cargo test -p byroredux --bin byroredux` warns `unused import: \`super::*\`\` on the
  second occurrence.
- **Evidence**: `grep -n "use super::\*;" byroredux/src/inventory.rs` → two hits, lines 1323/1363.
- **Impact**: none functionally (Rust allows redundant glob imports; cosmetic warning only, and
  invisible to the CI clippy gate for the same `#[cfg(test)]`-blind-spot reason as
  TD9-2026-09-22-01). Recorded as the third same-night instance of the "insert new test content
  above existing boilerplate without checking for now-redundant preceding declarations" habit
  (`cfg.rs` fixed at 22:51, `exposure.rs` broken at 23:10, `inventory.rs` this instance) — worth a
  review-checklist note, not because this specific instance matters.
- **Suggested Fix**: delete the redundant `use super::*;` at line 1363. Trivial.
- **Related**: TD9-2026-09-22-01, `ad96380be`.

### TD1-2026-09-22-01: `context/draw.rs` crossed 2000 production LOC (2019, was 1985)
- **Severity**: LOW · **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`: lines 1721-2462,
  741 LOC — a single function well over the 200-LOC flag)
- **Status**: NEW (re-crossing; chronic site) · **Effort**: small
- **Description**: the new AgX-tonemap/auto-exposure feature (`d54382415`) plus two small fixes
  (`b20923504`/#4602, `c77fa5973`/#4604) cumulatively pushed the file over the line. `context/`
  is already split by pass/responsibility across 19 sibling files; `draw.rs` holds the top-level
  `draw_frame` orchestrator plus two free helpers. This exact function/file has been split and
  regrown at least six times before, all previously CLOSED
  (`#1052`/`#1748`/`#1857`/`#2197`/`#2255`/`#3282`, 2026-06 through 2026-08; peak 4265 LOC file /
  1844 LOC function at `#1857`).
- **Suggested Fix**: extract the exposure/tonemap staging calls `draw_frame` currently inlines
  into a `context/exposure_dispatch.rs` sibling (or fold into the existing `telemetry.rs`), on the
  established construct-vs-record-vs-teardown precedent. No barrier reordering is implicated.
  Given the six-time recurrence, also consider a structural guard (a size-budget test on
  `draw_frame` specifically, not just the file) so a seventh manual fix is not the only lever.
- **Related**: #4602, #4604, #4568 (sibling split this cycle), #1052/#1748/#1857/#2197/#2255/#3282.

### TD1-2026-09-22-02: `storage_util.rs` (2320 LOC) — tracking issue closed 2026-09-16 without the fix landing
- **Severity**: LOW · **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/sdk/src/compatibility/storage_util.rs:596` (`papyrus_storage_util_declarations`,
  251-line declarative vec), `:2106` (`adapt_storage_util_global_list`, 384-line/17-arm dispatcher)
- **Status**: `#4218` is CLOSED (2026-09-16) but unresolved — the split it describes never landed;
  filing fresh since the issue no longer tracks this as open · **Effort**: medium
- **Description**: no commit touches this file anywhere in the 29a130e19..ee6d3fb39 window; both
  functions remain verbatim at their original line ranges. The 09-21 tech-debt report itself
  listed this as "Existing: #4218 (OPEN)" — but `#4218` had already been closed five days earlier
  (2026-09-16), meaning that report's dedup check was already stale at publish time. This is not
  a regression (the split was never applied, so nothing broke); it is the same unresolved debt
  under a closed-and-therefore-untracked issue.
- **Suggested Fix**: unchanged from #4218's own proposal — extract each
  `adapt_storage_util_global_list` arm into a named helper behind a thin dispatcher; replace
  `papyrus_storage_util_declarations`'s vec literal with a `const` table. Re-file as a fresh issue
  since #4218 no longer represents open work.
- **Related**: #4218 (closed without the fix), the 09-21 tech-debt report (whose dedup check was
  stale at publish for this specific item — noted for process awareness, not itself refiled since
  it is a point-in-time snapshot).

### TD2-2026-09-22-01: hierarchy-walk guard scaffolding implemented three different ways in one commit
- **Severity**: LOW · **Dimension**: 2 — Logic Duplication
- **Location**: `byroredux/src/anim_convert.rs:37-44`, `byroredux/src/cell_loader/water.rs:736-751`,
  `byroredux/src/ragdoll.rs:548-644` (two walks) — all from `c9843d5d1` (Fix #4572)
- **Status**: NEW
- **Description**: `crates/core/src/ecs/hierarchy.rs`'s `HierarchyTraversalGuard` doc comment
  states the intended pattern: guard (step budget) *plus* a visited set, paired — the pattern
  `byroredux/src/systems/bounds.rs`'s three pre-existing guarded walks (`#3700`) already follow.
  Fix #4572, applying the same safety idiom to four MORE walks in one commit, paired guard+set
  correctly in only one of the four (`cell_loader/water.rs`); the other three
  (`anim_convert.rs::build_subtree_name_map`, `ragdoll.rs::ragdoll_writeback_system`'s two BFS
  passes) added only a bare `HashSet` and a comment citing "the HierarchyTraversalGuard rule"
  without importing or using the struct.
- **Evidence**: `grep -n "HierarchyTraversalGuard" byroredux/src/ragdoll.rs
  byroredux/src/anim_convert.rs byroredux/src/cell_loader/water.rs` — the struct is used only in
  `water.rs`; the other two match only explanatory comment text.
- **Impact**: not a correctness bug — a visited `HashSet` alone already bounds each walk to one
  visit per live entity, so none of the three can spin on a cycle; #4572's actual fix (closing the
  unbounded-loop hazard) is sound. This is scaffolding inconsistency: three different hand-rolled
  shapes of the same idiom landed together, with the one canonical helper used in only one of
  four call sites, leaving three inconsistent precedents for the next hierarchy walk to copy.
- **Suggested Fix**: apply `HierarchyTraversalGuard` consistently at all four sites, or (more
  CLAUDE.md-aligned) add a small `core::ecs` helper bundling the `HashSet` + guard pair so all
  seven now-guarded call sites (and any future one) construct it once instead of hand-assembling
  it per site. Effort: trivial-small.
- **Related**: #4572 (introduced the inconsistency while landing a correct fix), #3700 (the
  original three guarded walks).

## Deferred

- **`draw_frame` structural regrowth** — the six-time-closed pattern (see TD1-2026-09-22-01)
  warrants a process-level fix (e.g. a function-length regression test) rather than a seventh
  manual extraction; flagged for a future session, not actioned here (read-only run).
- **`docs/engine/*.md` 267 advisory-symbol backlog** — long-standing italicisation backlog per
  Dim 3/4; count grew by 1 this cycle, not independently investigated (consistent with prior
  reports treating this as a standing backlog, not a per-cycle action item).
- **Full historical `docs/audits/*.md` CRITICAL/HIGH-without-issue-trace sweep** (Dim 4's
  90-day-old-report bullet) — no report in this delta's Paths crossed the 90-day boundary; not
  re-run this cycle.

## Verification notes

- `prod_loc` self-test: ok. All baseline counts re-measured today, not quoted from 09-21.
- `_audit-validate.sh`: OK, no STALE path references; 267 advisory symbols (backlog, +1).
- CI-gate-matching `cargo clippy -j4 -- -D warnings` run per-crate (not workspace-wide, to respect
  the RAM budget) across every crate this delta touched: `byroredux-renderer`, `byroredux-menuxml`,
  `byroredux-plugin`, `byroredux-physics`, `byroredux-nif`, `byroredux-ui`, `byroredux-pex`,
  `byroredux-core` all clean; only `byroredux` (bin) is red (TD8-2026-09-22-01).
  `--all-targets` run on `byroredux-renderer` only (the crate with the most test-target churn):
  9 errors, 7 pre-existing, 2 new (TD9-2026-09-22-01 and a trivial `useless_format` in `water.rs`
  folded into TD8's write-up, not separately filed).
- `cargo machete`: clean, no unused dependencies.
- Scratch working files for every dimension: `/tmp/audit/tech-debt/dim_1.md` through `dim_9.md`
  (deleted per Phase 4 after this report was written).
