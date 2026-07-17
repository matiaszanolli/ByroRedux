# SF-D3-AUDIT-01: Full 1.44M-instance CDB tree parsed and retained for the whole session purely to answer a boolean presence check

**Severity**: MEDIUM
**Labels**: medium, performance, memory, bug
**Location**: `byroredux/src/asset_provider/material.rs:236-267` (`load_starfield_cdb`/`has_starfield_cdb`); `crates/sfmaterial/src/reader.rs:29-90` (`ComponentDatabaseFile::parse`)
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (SF-D3-AUDIT-01)

## Description
The only Phase-1 consumer of the parsed CDB is `has_starfield_cdb() { !self.sf_cdbs.is_empty() }`, yet `load_starfield_cdb` runs the entire parse, materializing a 1,438,780-element tree (each entry carrying a cloned `class_name` String plus a `BTreeMap` of fields) and retaining it in an `Arc` for the provider's whole lifetime. Nothing else walks it.

## Evidence
Measured 9.70s parse time (debug) for the real vanilla CDB; the retained structure is hundreds of MB to low-GB of host RAM, entirely unread.

## Impact
Multi-second startup stall plus steady-state RAM retention of a dead structure on every Starfield launch, worsening as DLC CDBs are added.

## Suggested Fix
Either probe presence header-only (parse just the header/chunk-index without walking instances), or drop `instances` after the non-empty check and re-parse when Phase 2's per-field index is actually built.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
