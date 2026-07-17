# SF-D3-AUDIT-03: ComponentDatabaseFile::peek_magic is test-only, not wired into production discovery

**Severity**: LOW
**Labels**: low, tech-debt, bug
**Location**: `crates/sfmaterial/src/reader.rs:95-101`; `byroredux/src/asset_provider/material.rs:23-45`
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (SF-D3-AUDIT-03)

## Description
CDB discovery is purely path-based; a discovered path goes straight to the full `parse()`, which relies on `parse_header`'s `BadMagic` rejection rather than the cheaper dedicated `peek_magic` probe (only exercised by a test). Correctness is unaffected — a mis-named non-CDB file is still rejected, just via the heavier path.

## Suggested Fix
Gate discovery with `peek_magic` before the full extract/parse for a cheap reject, or document it as an intentional public probe for external callers.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
