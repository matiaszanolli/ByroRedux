# DIM5-01: walkers.rs module-doc XCLL comment still says the Starfield gate is == 108, but the live gate uses >= 108 since #1579

**Severity**: LOW
**Labels**: low, import-pipeline, legacy-compat, documentation
**Location**: `crates/plugin/src/esm/cell/walkers.rs:38-46,159-160` (doc) vs `:576-582` (live code)
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (DIM5-01)

## Description
#1579 hardened the Starfield XCLL dispatch gate from exact `== 108` to `>= 108` so a future-DLC cell with trailing pad bytes still takes the dedicated Starfield decode arm. The fix is correct and test-pinned, but the module-level doc comment (predating #1579) still states `== 108` in two places, directly contradicting the code three lines below it.

## Impact
No functional bug today, but a future engineer skimming the doc-first comment could "fix" the code back to exact equality, silently reintroducing the #1579 regression on any future-DLC or modded cell with trailing padding.

## Suggested Fix
Update both doc-comment occurrences from `== 108` to `>= 108`, matching the live predicate and the inline `#1579` comment already adjacent to it.

## Completeness Checks
No rows apply — this is a documentation-only fix.
