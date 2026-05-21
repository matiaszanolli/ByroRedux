# #1200 — REN-DIM15-02: stale triangle.frag:1321 reference in audit-renderer.md

**Source**: docs/audits/AUDIT_RENDERER_2026-05-19_DIM15.md (Dim 15, LOW — doc drift)
**Severity**: low
**Labels**: low, documentation
**State**: OPEN (filed 2026-05-19)

## Cause

`.claude/commands/audit-renderer.md` Dim-15 item 9 and Dim-20 item 5 cite `triangle.frag:1321` as the location of the `radius=-1` interior-fill gate. Actual location: line 2228 (`bool isInteriorFill = radius < 0.0;`). Shader has grown; line numbers drift.

## Fix

Replace `triangle.frag:1321` references with the symbol `isInteriorFill` in both checklist items. Sweep audit-renderer.md for other `triangle.frag:NNNN` patterns and convert to symbol references.

## Risk

N/A — documentation-only change.

## Estimated impact

0 runtime. Avoids future audits flagging the gate as missing when it's just been moved.
