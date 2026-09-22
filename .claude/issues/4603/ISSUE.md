# CONC-D3-2026-09-21-01: The `lock-order-check` CI lane has been red on every sampled main run since at least 09-14, so two real lock-order cycles it caught went unnoticed

**Labels**: medium, concurrency, tech-debt, bug

Filed via /audit-publish from docs/audits/AUDIT_CONCURRENCY_2026-09-21.md.

**Severity**: MEDIUM. A defence-in-depth gap on the HIGH-floor ECS-deadlock class, graded like SAFE-D4/D5-2026-09-21-01 (#4595, #4596). · **Dimension**: 3 — ECS Lock Ordering & Deadlock
**Location**: `.github/workflows/ci.yml:186-198`, job `lock-order-check` ("ABBA lock-order detector"). It runs `cargo test --workspace` under `BYRO_LOCK_ORDER_CHECK: 1`, without `--no-fail-fast`, and has no step that tells a lock-order panic apart from any other test failure.

**Status**: NEW.
- The same lane was red at HEAD twice before: #3580 (2026-08-30) and #3819 (2026-09-03). Both were closed by fixing the cycles of the day, with no structural change to the lane. Those fixes are still in place, so this is not a regression of either. It is the structural gap that has let the lane go red a third time.
- SAFE-D5-2026-09-21-01 (#4596) covers the other lane (`vulkan-validation`). This is the lane that `/audit-concurrency` Dim 3's first step names.

**Verified against**: HEAD `f97775ca8`. CI was read read-only via `gh run list`, `gh api …/actions/runs/<id>/jobs` and `gh run view --job <id> --log-failed`.

## Description

The lane no longer changes colour when a new cycle lands, so it cannot signal one. From the report, reconstructed from `gh run view` on main:

| Run(s) | Commit, date | Why the lane was red |
|---|---|---|
| 34858811547, 34997424662, 35102663503 | 8d08da793 09-14, fd0cd577c 09-15, 5e7781197 09-16 | A **real cycle**. 29 bin tests panic at `lock_tracker.rs:476`: "`FormIdPool` while holding `FormIdComponent` … closes a cycle `FormIdPool → FormIdComponent → FormIdPool`". Never filed. |
| 35223046709, 35357015643 | 8c834e0be 09-17, e6c6405a8 09-18 14:33 | The 8 adapter-dependent `crates/ui` Ruffle tests ("Ruffle requires hardware acceleration"). Not a lock failure. |
| 35389142963 | 913fd39d8 09-18 20:01, the commit that added `walk_anim` | The **#4546 cycle** (`ActorCinematicState → Transform → AnimationPlayer → ActorCinematicState`, 5 walk_anim tests). The lane caught it the day it landed, but the colour stayed red→red. #4546 was filed from a manual run on 09-20 and fixed on 09-21 (c1f38e3da). |
| 35658431384 | f97775ca8 (HEAD) | The same 8 `crates/ui` tests, after every lock-relevant binary passed (bin 2309, core 768, physics 175, renderer 1139, …). |

**Publish-time re-check** (read-only, this pass):
- The `ABBA lock-order detector` job concluded `failure` on **all 300 main-branch CI runs from 2026-09-03 00:05 UTC (run 33697968024) through HEAD (run 35658431384)**. The lane has not been green on main at any point since #3819 closed.
- **The FormIdPool↔FormIdComponent cycle started five days before the report's first sample, and was fixed only incidentally.**
  - Bisected over main runs: run 34350867263 (60928c1ee, 09-09 12:24) fails only on the Ruffle tests, and its bin binary passed (2010 tests). Run 34353152965 (549aa9f65, 09-09 12:48) is the first with the cycle.
  - 549aa9f65 ("Fix #3299") added `byroredux/src/cell_loader/stream_snapshot.rs`. Its `global_form_id` took `world.try_resource::<FormIdPool>()` and then `world.get::<FormIdComponent>(entity)` under it, the reverse of the established `FormIdComponent`-first order.
  - d8255b2e2 ("Implement persistent reference state management…", 09-16) swapped those two lines and copies the id out before taking the pool. The cycle is absent from that commit's own run (35158845923) and from every later run sampled.
  - The cycle therefore sat on main for about seven days behind an already-red lane.
- Other red causes in the window: `tools/byro-launcher`'s `engine::tests::supervision::a_crash_carries_its_code_and_the_last_thing_the_engine_said` (run 33751180670, 09-03 11:42), and the Ruffle `crates/ui` tests (run 33982391005 on 09-05, and the sampled runs through 09-09).

## Evidence

- The report's job logs are saved at `/tmp/audit/concurrency/lo_*.log` and `lockorder_job_head.log`. This pass re-fetched runs directly from GitHub: 34858811547, 34597216311, 34763397456 and 34848442541 (the FormIdPool cycle), 33751180670 (launcher), and 33982391005 and 35658431384 (Ruffle).
- Without `--no-fail-fast`, cargo stops at the first failing test binary. While a bin-binary cycle was present (09-09 → 09-16, 09-18 → 09-21), every later binary (core, physics, scripting, save, …) and all doctests never ran under the detector. At HEAD the Ruffle failure is in `byroredux_ui`, which runs after the lock-relevant binaries, so those did run and pass.

## Impact

- Both known escapes were latent ABBA risks between systems. #4546's own issue notes that its cycle would deadlock under a schedule change.
- The `vulkan-validation` lane is also inert (SAFE-D5-2026-09-21-01, #4596), so CI currently gives no lock-order signal at all. A new cycle on a path the test suite exercises merges with no colour change.

**Trigger conditions**: any commit that introduces a new lock-order cycle on a path the test suite exercises.

## Related

- SAFE-D4-2026-09-21-01 (#4595): the same red `cargo test --workspace` (the 8 `crates/ui` tests) makes the `cargo-test` job skip its clippy step. Same root cause, separate consequence.
- SAFE-D5-2026-09-21-01 (#4596): the `vulkan-validation` lane never reaches Vulkan, so the detector's only live-world CI run is inert too.
- #3580 and #3819 (closed): the previous two red episodes of this lane. #3819's impact section already warned that a red lane "cannot catch a *new* cycle landing on top of these".
- #4546, #3266, #1410, #2137.

## Suggested Fix

- Make the lane able to go green on its own. Gate the Ruffle adapter tests (a wgpu-adapter probe or `#[ignore = "needs a Vulkan adapter"]`, the same fix #4595 needs), or pass `--exclude byroredux-ui` in this job.
- Add `--no-fail-fast`.
- Add a second gate that fails the job if and only if the output contains `lock-order cycle`, so a lock failure is distinguishable from any other red.
- The report also asks that the FormIdPool↔FormIdComponent episode be filed for the record. It is recorded above (549aa9f65 → d8255b2e2) rather than as a separate issue, since it is fixed at HEAD.

Source: docs/audits/AUDIT_CONCURRENCY_2026-09-21.md (CONC-D3-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every other CI job that runs a fail-fast `cargo test` checked for the same masking (the `cargo-test` job's clippy step is #4595; the `vulkan-validation` lane is #4596)
- [ ] **TESTS**: a main-branch run after the change shows the `ABBA lock-order detector` job green, and a deliberately introduced cycle on a throwaway branch turns it red with the cycle gate's message
