# FO4-D1-01: stale exterior-absorption-dormant comment

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2593
**Finding ID**: FO4-D1-01

**Severity**: LOW
**Dimension**: 1 (ESM/Plugin)
**Location**: `crates/plugin/src/esm/cell/wrld.rs:493-503`
**Status**: NEW

## Description
A comment block describes FO4 exterior worldspace absorption as "dormant" /
not yet wired up. That has been stale since #2063/#2376 landed — exterior
absorption is live wiring today, not a documented gap.

## Evidence
`crates/plugin/src/esm/cell/wrld.rs:493-503` still carries the pre-#2063
"dormant" framing even though the exterior absorption path it describes has
been active since #2063 (and refined in #2376).

## Impact
Doc-only. No functional effect — but the comment actively misleads anyone
reading the file into thinking exterior absorption isn't wired.

## Suggested Fix
Update the comment to describe current (live) behavior, cross-referencing
#2063/#2376 instead of describing the pre-fix state.

## Completeness Checks
- [ ] **TESTS**: N/A — doc-only change
