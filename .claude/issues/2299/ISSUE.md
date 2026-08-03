# NIFAL-D4-03: nifal.md passthrough table still calls BSFurnitureMarker unwalked — stale since M41.5

Source: `docs/audits/AUDIT_NIFAL_2026-08-03.md`

**Severity**: LOW
**Dimension**: Nodes · **Tier Violated**: (doc)
**Location**: `docs/engine/nifal.md:309` (Passthroughs table, `BSFurnitureMarker` / `BSInvMarker` row)
**Status**: NEW

## Description

`docs/engine/nifal.md`'s passthrough table still calls `BSFurnitureMarker`
"parsed, not walked into `Imported*`" — stale since `#2010`/M41.5 Phase B;
furniture markers are consumed by `byroredux/src/systems/sandbox.rs` via
`furniture_component()`. `BSInvMarker` (the row's other half) genuinely is
still passthrough-only, so the table's single combined row is half-stale in
one direction.

## Evidence

`byroredux/src/systems/sandbox.rs:69,345` reference `furniture_component`'s
resolution logic as the live consumer; the doc table still lists both markers
together as unwalked.

## Impact

Doc-only — no behavior impact, but a future auditor or contributor reading
`nifal.md` would incorrectly conclude furniture markers are a gap needing
work, when the gap is `BSInvMarker` only.

## Suggested Fix

Split the combined row in two: mark `BSFurnitureMarker` as consumed (cite
`furniture_component` / M41.5), keep `BSInvMarker` as passthrough-only with
its existing "inventory-icon system" future-consumer note.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only fix — no behavior change to pin)

## Filed as

GitHub issue #2299, labels: low, nif-parser, documentation.
