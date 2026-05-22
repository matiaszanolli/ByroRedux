# #1132 Investigation

**Resolution**: partial-completion close. Two of four listed contributors shipped; the remaining two are architecturally gated on M40 (scheduler parallelisation) and #1115 (god-function split), with the deferral rationale documented in-code.

## Fix-order status (from issue body)

| # | Contributor | Status | Reference |
|---|---|---|---|
| 1 | Static FX-skip classification at spawn | **DONE** | #1136 CLOSED COMPLETED; `IsFxMesh` marker at [`components.rs`](../../byroredux/src/components.rs), consumed at [`static_meshes.rs:124-127, 252-254`](../../byroredux/src/render/static_meshes.rs#L124-L127) |
| 2 | Material hash precomputation | **DONE** | #781 / PERF-N4; `intern_by_hash` closure-based path at [`static_meshes.rs:636-640`](../../byroredux/src/render/static_meshes.rs#L636-L640) — `to_gpu_material()` only fires on dedup miss (~3% of calls on Prospector) |
| 3 | Single `RenderExtract` query bundle | **DEFERRED** | Rationale documented at [`static_meshes.rs:70-80`](../../byroredux/src/render/static_meshes.rs#L70-L80): implementing before M40 (parallel scheduler) lands would lock in a design without the constraints of the actual parallel scheduler to inform it; adds ~0.5 ms/frame for zero benefit today |
| 4 | `par_iter` over draws | **GATED** | Depends on Item 3 + #1115 god-function split |

## Bench evidence of progress

Issue baseline (`1775a7e6`, 2026-05-16): **brd_ms = 8.07 > fence_ms = 5.50** on FO4 MedTek (10810 entities, 7359 draws).

Current ROADMAP-pinned (`d0b52bd5`, 2026-05-21): **brd_ms = 7.10** on FO4 MedTek (10912 entities, 7364 draws @ 67.9 FPS / 14.72 ms).

Delta: **−0.97 ms** — sits inside the issue's projected combined savings band for items 1 + 2 (0.5-1 ms + 0.2-0.5 ms = 0.7-1.5 ms).

## Why close (not just leave open)

1. **All actionable items are done**. Items 3 + 4 are explicitly deferred with rationale captured *in-code* — the comment at [`static_meshes.rs:70-80`](../../byroredux/src/render/static_meshes.rs#L70-L80) IS the work-record. Leaving an issue open as a duplicate tracker for an in-code TODO is the exact pattern flagged by today's `audit-tech-debt` Dim 10 (Audit-Finding Rot) work.

2. **The CPU bottleneck framing is still accurate** (brd_ms=7.10 > likely-fence_ms), but that's the M40-gated portion. The issue's "Fix order" lists 4 items and they are all either done or deferred-with-rationale. There is no actionable next step that doesn't require M40 parallelisation infrastructure.

3. **Profiling gap remains** (issue's own "Profiling Gap" section). No flame-graph or dhat measurement was wired up; "concrete ms savings" claims in the issue are by code-shape inspection. The empirical 0.97 ms improvement is the only ground-truth we have.

## SIBLING checks (per issue's completeness list)

- **Prospector / Whiterun benches post-fix**: ROADMAP shows Prospector 120.7 FPS / 8.28 ms, Whiterun 211.0 FPS / 4.74 ms — both well-bounded; items 1+2 had room to help them but they were already GPU-bound, so the improvements landed where they were needed (FO4 MedTek CPU-bound case).
- **LOCK_ORDER**: not yet relevant since item 3 (the query-bundle reduction) is deferred. When M40 parallelisation lands, the RenderExtract design will need TypeId-sort verification at that point.
- **TESTS**: no CPU-time regression test landed (issue's TESTS check). dhat / alloc-counter infra still ungated — same gap surfaced in today's tech-debt audit's TD9-200/201 BLOCKED note. The flame-graph harness is a precondition for any further perf work in this area.

## Pattern observation

Same hygiene flow as today's three earlier tracker-closes (#1199, #1229, #1185, #1156) — listed work was done over the last 1-2 weeks; the in-code rationale captures the deferred portion; the GitHub tracker just needs a closing comment to reflect the partial-done + deferred state.
