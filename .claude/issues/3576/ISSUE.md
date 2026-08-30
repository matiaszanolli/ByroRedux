# #3576 — REN-2026-08-30-D1-02: the Dimension 1 checklist in `audit-renderer/SKILL.md` carries two stale claims — a deleted entry-point symbol and a "no recovery path exists" gap that was closed on 2026-08-16 — and the staleness already produced a false "re...

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3576 --json state`.

---

- **Severity**: LOW
- **Dimension**: AS Correctness (audit-tooling doc rot)
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (line 74 entry-point list; line 85 Dimension-1 LRU/shrink checklist). Ground truth: `crates/renderer/src/vulkan/context/resources.rs` (`restore_missing_static_blas_for_draws`, line 267), `byroredux/src/app_frame.rs` (line 235)
- **Status**: NEW
- **Description**: Two independent inaccuracies in the same checklist paragraph the auditor is instructed to treat as authoritative:
  1. **Line 74** lists `crates/renderer/src/vulkan/context/resources.rs` (`build_blas_for_mesh`) as a Dimension-1 entry point. That symbol does not exist. It was deleted under #2914 together with the single-shot `build_blas`; `docs/engine/memory-budget.md` documents the deletion in its own words ("the single-shot `build_blas` / `build_blas_for_mesh` pair had **no caller anywhere in the workspace** … Both functions were deleted under #2914"). The only surviving mentions in the tree are two prose references (`crates/facegen/src/eval.rs:119`, `resources.rs:203`) that both describe it in the past tense. **Here the SKILL is wrong and the code+`memory-budget.md` are right.**
  2. **Line 85** instructs the auditor to recast, not re-report, "#1793: a permanently-missing rigid BLAS has no recovery path (**no per-frame build primitive exists**)". The parenthetical is false. `VulkanContext::restore_missing_static_blas_for_draws` (`resources.rs:267`) is exactly that per-frame build primitive: it collects every rigid, TLAS-eligible draw handle, LRU-stamps the whole set via `mark_static_blas_used`, retains only `!accel.has_blas(handle)`, resolves each survivor's retained source (dedicated RT buffers at offset 0, or a byte-offset subrange of the global geometry buffers for global-only LOD meshes), and re-batches them through `build_blas_batched` — and it is called every frame from `byroredux/src/app_frame.rs:235`, before `draw_frame`. `build_tlas_instances`' own `missing_rigid_blas` arm has been rewritten to match: *"The app-frame prepass normally restores an evicted rigid BLAS from retained mesh buffers before entering `draw_frame`. Reaching this arm therefore means that recovery failed or the source mesh was ineligible."*
- **Evidence**:
  - `grep -rn "build_blas_for_mesh" crates/ byroredux/` → 2 hits, both prose, zero definitions or calls.
  - `git log -S "restore_missing_static_blas_for_draws" -- crates/renderer/src/vulkan/context/resources.rs` → `8e7582ed`, dated **2026-08-16** — eleven days *before* the last full sweep at `969d81c8` (2026-08-27).
  - `docs/audits/AUDIT_RENDERER_2026-08-27.md`, "Known-open, deliberately NOT re-reported": *"Per `SKILL.md` Dimension 1, the two documented-not-fixed AS gaps from `#1793` (no recovery path for a permanently-missing rigid BLAS; …) were re-verified as unchanged and are not re-reported."* That line is a verification claim the code did not support at the time it was written; the stale checklist is what produced it.
  - #1793 is **not** in the 159-issue OPEN set, consistent with the gap having been closed.
  - The *second* #1793 gap in the same sentence (a synchronous multi-cell `--grid` burst false-evicting via the shared `frame_counter`) **is** still accurate — `blas_static.rs:228-238` still carries the "Deferred pending a `--grid` + low-VRAM-budget repro" note and still bumps `self.frame_counter` per `build_blas_batched` call. `mark_static_blas_used` partially mitigates it for the upcoming rigid draw set but does not remove the counter-semantics hazard. Only the first half of the sentence needs correcting.
- **Impact**: The checklist's "Recast, don't re-report" instruction converts a stale premise into a *positive false statement in the audit record* — the strongest form of the ~1-in-6 stale-finding problem, because it manufactures a "verified intact" line rather than merely a dropped finding. Any future Dimension-1 run will reproduce the same false verification until the text is corrected.
- **Suggested Fix**: In `SKILL.md`: (a) replace `build_blas_for_mesh` in the line-74 entry-point list with `restore_missing_static_blas_for_draws` (the live pre-TLAS recovery primitive) and `build_blas_batched`; (b) in line 85, drop the "permanently-missing rigid BLAS has no recovery path" clause and replace it with a regression guard — *"verify the per-frame `restore_missing_static_blas_for_draws` prepass (`resources.rs`, called from `app_frame.rs`) still runs before `draw_frame` and still calls `mark_static_blas_used` **before** `handles.retain(|h| !accel.has_blas(h))`"* — which is already pinned by the source-position test at `resources.rs:473-497`; (c) keep the `--grid` / shared-`frame_counter` half of #1793 as-is, it is still accurate.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D1-02

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
