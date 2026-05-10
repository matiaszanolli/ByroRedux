# #898 — REN-D15-02: shader comment claims "roughly same brightness" but cumulative 0.6 × 0.4 = 0.24× drops interior fill ~46% at NdotL=0.5

**Severity**: INFO / LOW (comment-vs-math honesty; tuning is user-validated)
**Dimension**: Sky / Weather / Exterior Lighting (interior-fill cross-cut)
**Location**: `byroredux/src/render.rs:159` (CPU `INTERIOR_FILL_SCALE = 0.6`) + `crates/renderer/shaders/triangle.frag:2053` (`INTERIOR_FILL_AMBIENT_FACTOR = 0.4`)
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-05-07_DIM15.md` § REN-D15-02
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/898

## Summary

Two-stage interior-fill multiplier: CPU `0.6` × shader `0.4` = `0.24×` net on `directional × albedo`. Shader comment claims "roughly same brightness" as legacy half-Lambert; arithmetic shows ~46% dim-down at NdotL=0.5.

## Fix sketch (preferred)

Update the shader comment at `triangle.frag:2045-2052` to reflect actual semantic — the 0.4 was tuned by visual judgment, the cumulative chain is 0.24×, the dim-down is intentional vs the corrugated-metal stripe pathology.

No behaviour change. Docstring honesty only.

## Alternative

If user wants closer parity: bump shader-side factor to ~0.7 (net 0.42 vs old 0.45). Visual judgment.

## Status

NEW. CONFIRMED via direct line-walk during Dim 15 focused audit. The 2026-05-07 full audit incorrectly called the prompt's "0.6× ambient" claim "stale" — it is correct; the shader 0.4 is on top of, not in place of.
