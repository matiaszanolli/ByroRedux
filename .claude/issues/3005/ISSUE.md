# RT-2026-08-16-06: draw-batch merge regression on fnv and fo3 — batches and GPU calls past the x1.1 contract

**Issue**: #3005
**Severity**: MEDIUM
**Dimension**: Telemetry baseline
**Labels**: `medium,performance,bug`
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RUNTIME_2026-08-16.md` (runtime telemetry baseline diff).

**Location**: `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv` · `.claude/audit-baselines/runtime/fo3-MegatonPlayerHouse.tsv` · `byroredux/src/render/mod.rs`

## Description

The runtime telemetry sweep measured `fnv` and `fo3` past the `≤ baseline ×1.1` contract on the **draw-batch merge and GPU-call halves**, while the `DrawCommand` count itself stayed within tolerance. That combination points at the merge step rather than at more geometry being submitted.

## Evidence

Measured by `/audit-runtime` against the checked-in baselines on 2026-08-16; both scenes exceed the ×1.1 gate on `batches` and `gpu_calls` while `cmds` remains inside it. Full per-metric table in the source report (§ RT-2026-08-16-06).

Baselines re-confirmed present 2026-08-17: `fnv-FreesideAtomicWrangler.tsv`, `fo3-MegatonPlayerHouse.tsv`.

## Impact

More batches and more GPU calls for the same draw-command count means the batching pass is merging less effectively than the baseline recorded — CPU-side submission cost rises with no rendering benefit. On the user's hardware (Ryzen 7950X), a CPU-side regression is the kind that shows up as a frame-time floor rather than a GPU stall.

## Suggested Fix

Bisect `byroredux/src/render/mod.rs`'s batch-merge path against the baseline commit to find what changed the merge key or its ordering. Then either fix the regression or, if the new behaviour is correct, re-baseline **with the justification recorded** — a silent re-baseline would erase the signal.

## Related

- #3006 (RT-2026-08-16-07 — the FO4 scene's different regression shape)

## Completeness Checks
- [ ] **CAUSE-NOT-BASELINE**: The regression is explained before any re-baseline
- [ ] **SIBLING**: The other three baseline scenes checked for the same merge-side drift
- [ ] **RE-BASELINE-JUSTIFIED**: If re-baselined, the reason is recorded in the TSV's companion notes
- [ ] **TESTS**: The telemetry gate fails on this metric until resolved

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3005 --json state` when live state is needed.*
