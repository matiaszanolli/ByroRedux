# EXAL-02: Worldspace climate resolution ignores the WRLD parent chain (WNAM/PNAM) — child worldspaces silently get the procedural Mojave sky

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2450
**Finding ID**: EXAL-02 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 5 — EXAL
**Location**: `byroredux/src/cell_loader/exterior.rs:799-817`; `crates/plugin/src/esm/cell/wrld.rs:215-217`; `crates/plugin/src/esm/cell/mod.rs:821-826`
**Status**: NEW (concrete sub-finding under #2373/#2369)

## Description
Climate resolution is a single flat lookup (`worldspace_climates.get(&worldspace_key)`) populated only when a WRLD authors its own `CNAM`. `parent_worldspace`(WNAM)/`parent_flags`(PNAM) are parsed with zero consumers repo-wide. A child worldspace inheriting climate from its parent resolves to `None` → `apply_worldspace_weather` installs the procedural-fallback Mojave desert sky.

## Evidence
Confirmed directly: `parent_worldspace`/`parent_flags` are parsed and stored in `crates/plugin/src/esm/cell/mod.rs:821-825`; grep for both names outside `crates/plugin/src` returns zero hits in `byroredux/src/`.

## Impact
Any child worldspace relying on PNAM inheritance (Skyrim's DLC/holdout worlds, FO4 sub-worlds, Oblivion-plane worlds) renders the wrong sky/fog/sun. Silent — the fallback is an intentional canonical default, so nothing logs the inheritance miss; presents as a weather bug, not an inheritance gap.

## Related
#2373 (OPEN), #2369 (OPEN, EX-14/15 — adjacent parent-worlds scope).

## Suggested Fix
Chase `parent_worldspace` when no own CNAM exists, gated on the PNAM inherit bit, inside `env_translate.rs`; log at `warn` when the chain terminates unresolved.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Parent-chain chase lives inside `env_translate.rs`, not a second climate-resolution site
- [ ] **TESTS**: A regression test confirms a childless-CNAM worldspace with a valid WNAM chain resolves to its parent's climate, not the procedural fallback
