# REG-2026-09-29-01: #4606 was closed on its doc half; the GPU-idle throughput defect is still in the code and has no open issue (regression of #4606)

**Labels**: medium,renderer,performance,sync,bug

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
