# Issue #315 — R6-02: Metal reflection double-modulates by local albedo

- **Severity**: MEDIUM | **Source**: AUDIT_RENDERER_2026-04-14.md | **URL**: https://github.com/matiaszanolli/ByroRedux/issues/315

Post-#268 invariant break: `triangle.frag:503-518` folds `traceReflection().rgb` (already textured by hit surface) into `ambient`, which flows to `outRawIndirect`; composite then multiplies by local albedo a second time. Tinted metals lose 30-50% reflection energy.

Fix direction: route reflection through the direct path (add to `Lo`); metals have kD≈0 so no double-count there.
