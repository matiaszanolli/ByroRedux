# TD8-101: log declared as a dependency in 7 crates that never call it

**GitHub Issue**: #2075
**Labels**: low,tech-debt,bug

**Severity**: LOW
**Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
**Location**: `crates/{bgsm,sfmaterial,debug-ui,spt,facegen,pex,papyrus}/Cargo.toml`

## Description
Each of these 7 crates lists `log = { workspace = true }` with zero `log::`/`warn!`/`info!`/etc. call sites anywhere in `src/`. Reads like an untrimmed crate-template dependency.

## Evidence
Confirmed live: `grep -n "^log" crates/{bgsm,sfmaterial,debug-ui,spt,facegen,pex}/Cargo.toml` all match `log = { workspace = true }` (papyrus lists it as `log = { workspace = true }` alongside `logos = "0.15"`); `grep -rln "log::" crates/<crate>/src/` returns 0 files for all 7 crates.

## Impact
None at runtime (tiny facade crate) — pure housekeeping.

## Suggested Fix
Remove the dependency from the 7 `Cargo.toml`s. If any plan to add logging soon (facegen/sfmaterial/bgsm parsers are plausible candidates), leave a one-line comment instead of silently deleting.

**Effort**: trivial

## Completeness Checks
- [ ] **SIBLING**: All 7 crates share this exact gap — verify no other crate in the workspace has the same untrimmed dependency
- [ ] **TESTS**: N/A (Cargo.toml dependency removal — `cargo check` across the workspace is the regression check)
