# PERF-GP-01: Volumetrics dispatch wasted — composite multiplies result by 0.0

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/928
**Severity**: HIGH
**Source audit**: `docs/audits/AUDIT_PERFORMANCE_2026-05-10.md` (Dim 1 / Dim 2)
**Related**: #924 (fog visibility regression — different aspect of the same gate)
**Filed**: 2026-05-10

## TL;DR

The M55 volumetrics inject + integrate compute pipeline dispatches every frame (~1.84M sun-shadow ray-query traces against the TLAS), but the composite shader multiplies the result by 0.0 (gate added in `f62d4bd` after the per-froxel banding diagnostic). **~10–20 ms/frame estimated GPU waste — single largest avoidable cost in the renderer.**

## Fix

Wrap `vol.dispatch()` in `draw.rs:1694-1738` with a const matching the composite gate. ~5 LOC. Flip both gates together when M-LIGHT v2 lands.

## Critical-path priority

This is the **#1 HIGH finding** from the audit. Fix this first before chasing other GPU perf items — it dominates the "fighting for ms" picture on Prospector at 23 FPS / 44 ms.
