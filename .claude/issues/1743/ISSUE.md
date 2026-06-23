# SCR-D7-03: --scripts-bsa override order is first-listed wins

Filed as: matiaszanolli/ByroRedux#1743
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: LOW
- **Dimension**: Engine Attach & Trigger Wiring
- **Location**: `byroredux/src/asset_provider.rs:613-621`, 641-643
- **Labels**: low, import-pipeline, legacy-compat, bug

## Description
`extract_pex` returns the first archive hit in flag order, so override archives must be listed BEFORE vanilla — the inverse of mod-manager load order (later = higher priority). Documented in the docstring (a contract, not a defect) but an ergonomic foot-gun.

## Suggested Fix
Document override-first prominently in CLI help, or reverse iteration so last-listed wins.
