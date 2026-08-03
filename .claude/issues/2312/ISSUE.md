# TD1-084: setup_cornell_scene grew to 296 LOC

**Severity**: LOW
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `byroredux/src/cornell.rs` (`setup_cornell_scene`, 244→296 LOC)
**Labels**: low, renderer, tech-debt, bug
**Source**: `docs/audits/AUDIT_TECH-DEBT_2026-08-03.md`

## Description
Grew via #2248/#2249's real fog-volume and fire-refraction probe setup
(confirmed genuine coverage, not stub). Test harness code, not load-bearing
production path.

## Suggested Fix
Low priority; extract per-probe setup (fog volume, fire-refraction material)
into helpers if it grows further.

## Age / Effort
This window. Effort: trivial, deferrable.
