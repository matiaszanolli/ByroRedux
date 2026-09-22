# ESM-2026-09-21-D7-01: two references to the deleted crates/plugin/src/legacy/ module survived #4384

**Issue**: #4647
**Filed**: 2026-09-22 (audit-publish, AUDIT_ESM_2026-09-21.md)

**Severity**: LOW
**Dimension**: EsmIndex → ECS Handoff & Redux-Native Tier
**Location**: `crates/plugin/src/esm/reader.rs:385-386`; `byroredux/src/sf_smoke.rs:317`

## Description
Two references to the deleted `crates/plugin/src/legacy/` module (`#4384`, deleted in `e3131f5ef`) remain: the `FormIdRemap` doc points at `legacy/mod.rs` for `FormIdPair`; `sf_smoke`'s low-resolve-rate hint names a `legacy/starfield.rs` from-scratch parser.

## Evidence
`ls crates/plugin/src` shows no `legacy/` directory. `FormIdPair` lives at `crates/core/src/form_id.rs:123`.

## Impact
Documentation/diagnostic rot only.

## Suggested Fix
Point the doc at `byroredux_core::form_id::FormIdPair`; update the `sf_smoke` hint to name `crates/plugin/src/esm/records/dispatch_*.rs`.

## Related
#4384, #1322 (closed)

## Source
docs/audits/AUDIT_ESM_2026-09-21.md (ESM-2026-09-21-D7-01)
