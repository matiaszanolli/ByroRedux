# FNV-D1-LOW-03: NifImportRegistry::insert doc claims default cache mode is unlimited (it is 2048)

**Issue**: #1724
**Severity**: LOW
**Labels**: low, import-pipeline, documentation
**Dimension**: 1 — Cell Loading
**Location**: `byroredux/src/cell_loader/nif_import_registry.rs:291-293` (`insert` doc)
**Source audit**: AUDIT_FNV_2026-06-23 (FNV-D1-LOW-03)

## Description
The `insert` doc says "Empty Vec on the no-eviction path (the default `BYRO_NIF_CACHE_MAX=0` mode)", but the constructor (`new()`, lines 170-173) defaults to `unwrap_or(2048)` — `BYRO_NIF_CACHE_MAX=0` is the opt-in unlimited mode (lines 174-178 warn it disables the cap), not the default. With the real default of 2048, eviction can fire (and the returned clip-handle Vec is the mechanism that must be released).

## Impact
Documentation only — the LRU code and `#[must_use]` contract are correct. The doc inverts which mode is the default.

## Suggested Fix
Reword to "the no-eviction path (`BYRO_NIF_CACHE_MAX=0`, opt-in unlimited)"; the default 2048 cap does evict.
