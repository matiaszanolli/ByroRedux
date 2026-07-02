# SAVE-05: A second load before the drain silently discards the first queued snapshot

**Labels**: low, tech-debt, bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1848
**Source**: docs/audits/AUDIT_SAVE_2026-07-02.md

**Severity**: LOW
**Dimension**: Frame-Boundary Capture & Off-Frame Apply
**Location**: `byroredux/src/save_io.rs:145` (`PendingSaveLoadSlot(pub Option<Snapshot>)`), `byroredux/src/save_io.rs:546-552` (`LoadCommand` overwrites `pending.0`)

## Description
`PendingSaveLoadSlot` is a single `Option`. If two `load` commands are issued in the same frame (before `step_save_loads` drains), the second overwrites the first with no warning. Idempotency of a single load is correct (the drain `.take()`s and teardown is unconditional), but the drop-the-earlier-request behaviour is silent.

## Evidence
`LoadCommand::execute` does `pending.0 = Some(snapshot)` unconditionally; no check for an already-populated slot.

## Impact
Cosmetic / astonishment only — the *last* `load` wins, which is arguably the intent, but the discarded request is invisible. No data loss (the on-disk saves are untouched).

## Suggested Fix
Log at INFO when overwriting a non-empty pending slot ("load slot N superseded by slot M before drain").

## Completeness Checks
- [ ] **TESTS**: A regression test issues two `load` commands in the same frame and asserts the drain applies the second, with the supersede logged
