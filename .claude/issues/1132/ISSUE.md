title:	PERF-D3-NEW-01: build_render_data is the explicit CPU bottleneck on FO4 MedTek (brd_ms=8.07 > fence_ms=5.50)
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, high, performance, renderer
comments:	0
assignees:	
projects:	
milestone:	
number:	1132
--
## Source Audit
`docs/audits/AUDIT_PERFORMANCE_2026-05-16.md` — Dimension 3 (Draw Call Overhead) — root cause spans Dim 6 (CPU Allocations)

## Severity
**HIGH** — caps achievable FPS ceiling on dense FO4 cells. M52 (mesh shaders / GPU-driven rendering) is the structural fix; this issue is the bundle of quick wins that buy 1-3 ms back without M52's surface area.

## Location
`byroredux/src/render.rs:299-1605` (the god function from #1115, but framed here as the perf-impact contribution)

## Status
**NEW** at HEAD `1608e6a2`

## Description
Bench-of-record at `1775a7e6` measures **brd_ms=8.07 > fence_ms=5.50** on FO4 MedTek (10 810 entities, 7359 draws). The frame is no longer GPU-bound on this bench; further GPU-side optimisations don't move the needle until `build_render_data` is reduced.

Three identified contributors:

1. **17 query lock acquisitions per frame**, with `GlobalTransform` taken 3 distinct times (`render.rs:341, 514, 1070, 1298`). Today these don't contend (render runs outside the scheduler), but the `#501 / M40` scheduler-parallelisation block-comment at `render.rs:495-505` acknowledges this is a ~1.5-2 ms latent stall the moment the scheduler parallelises.

2. **6 substring scans per draw, every frame** (`render.rs:660-666`):
   ```rust
   if contains_ci(tp, "effects\\fx") || contains_ci(tp, "effects/fx")
      || contains_ci(tp, "fxsoftglow") || contains_ci(tp, "fxpartglow")
      || contains_ci(tp, "fxparttiny") || contains_ci(tp, "fxlightrays")
   ```
   At MedTek scale: 7359 draws × 6 scans × ~600 byte-cmps ≈ **26M byte comparisons / frame**, all to answer a question invariant across frames (texture path doesn't change after spawn). Filed standalone as PERF-D3-NEW-02.

3. **Per-draw material hashing** — every `DrawCommand` materialises `material_hash()` (50-field hash) plus `to_gpu_material()` on dedup miss. Prospector dedup-hit rate ~97%; MedTek's rate isn't telemetered today but the hash itself runs on every draw before dedup.

## Impact
ROADMAP.md Tier 11 + commit `1775a7e6` log explicitly call this out: *"frame still CPU-bound on `build_render_data` — that one is genuine and persists across the fix"*. Bench numbers (Prospector 124.6, Whiterun 218.4, MedTek 67.1) all post-fix; FO4 is the outlier.

## Suggested Fix order (cheapest first)
1. **Static FX-skip classification at spawn** (PERF-D3-NEW-02): lift the 6-needle substring scan to a one-time `IsFxMesh` marker at cell-load. ~50 LOC. **Estimated saving: 0.5-1 ms.**
2. **Material hash precomputation**: cache `material_hash()` on the `Material` component at spawn / mutation. Estimated saving: 0.2-0.5 ms.
3. **Single query bundle**: replace 17 `world.query::<T>()` with a `RenderExtract` resource populated once per scheduler tick. Filed as P1 follow-up to M40 parallelisation; comment at `render.rs:495-505` already names this path.
4. **par_iter over draws**: embarrassingly parallel. Subordinate to (1)-(3) and gated on #1115's god-function split.

## Profiling Gap
dhat / alloc-counter regression coverage NOT wired. Real measurement: `cargo flamegraph -- --bench-frames 600 --bench-hold` on MedTek to confirm contributors split as estimated. Without that flame, ranking is by code-shape inspection only.

## Completeness Checks
- [ ] **UNSAFE**: N/A — refactor in safe Rust
- [ ] **SIBLING**: Check Prospector + Whiterun benches after each contributor fix — they're GPU-bound today but contributors (1) and (2) apply to all scenes
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: Contributor (3) reduces query count; verify no new TypeId-sort issues
- [ ] **FFI**: N/A
- [ ] **TESTS**: Need a CPU-time regression test post-fix (alloc-counter or cargo bench gate)

## Related
- #1115 (god-function size, separate dimension — same file, different framing)
- M52 (structural fix, ROADMAP Tier 11)
- PERF-D3-NEW-02 (this issue's contributor 2 broken out for standalone fix)
- `feedback_audit_findings.md` (RenderDoc/profiler measurement required before claiming concrete ms savings)
