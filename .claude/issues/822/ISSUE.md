# #822 — FNV-D3-DOC: Prospector Saloon entity-count documentation drift

**Severity**: LOW (documentation only — no code regression)
**Source**: `docs/audits/AUDIT_FNV_2026-05-04_DIM3.md`
**Created**: 2026-05-04
**Bundles**: FNV-D3-DOC-01 + FNV-D3-DOC-02

## Summary

Three different Prospector entity counts across the codebase
(1200 / 784 / 809), none matching the live `entity_count` of 803 or
the parser ground-truth REFR count of 461.

## Sites

1. `ROADMAP.md:138` — "1200 entities" (older snapshot)
2. `byroredux/src/cell_loader.rs:52` doc-comment — "should produce 784"
3. `byroredux/src/cell_loader.rs:865-866` — "809 REFRs"

## Fix shape

Three single-line edits, one doc-cleanup commit. Reconcile against the
live cell-loader-log definition (803 entities / 461 REFRs) or note
each definition explicitly.

## Why bundled

Same drift, same root cause (stale numbers from earlier dispatch
generation), same fix shape. Two micro-issues would clutter the
tracker without adding value.

## How to fix

```
/fix-issue 822
```
