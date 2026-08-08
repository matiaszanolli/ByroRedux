# FNV-D8-01: ROADMAP's 145.1 FPS Prospector headline is no longer reproducible with its own repro command after FSR became the engine default

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2560
**Finding ID**: FNV-D8-01

**Severity**: LOW
**Dimension**: Real-Data Validation & Bench-of-Record
**Location**: `ROADMAP.md:432,1117`
**Status**: NEW, related to but distinct from open #2367 (a different, real, still-unexplained ~2× speedup investigated across a different, later commit window)

## Description
The ROADMAP compat-matrix headline Prospector figure ("145.1 FPS", captured `8a668eff` 2026-07-18) is no longer reproducible with its own cited bare repro command, because `5c7acfe2` (2026-07-24) made FSR 3.1 Quality the engine-default upscaler — six days *after* the baseline was captured under native TAA.

## Evidence
Confirmed directly: `ROADMAP.md:70` carries the "145.1 FPS" Prospector figure. Running the literal ROADMAP repro command today measures 254.0 FPS (FSR default), not ~145 FPS, which nearly registered as a false "+75% FPS" regression-that-wasn't before a `git log` cross-check identified the confound. Forcing `--upscaler taa` reproduces the intended baseline within noise (135.9 vs 136.4 FPS, per this audit's own fresh bench run).

## Impact
Documentation-currency issue, not an engine regression. Risk is a future bench comparison misreading the FSR-vs-TAA confound as a real performance change.

## Suggested Fix
Annotate `ROADMAP.md:432,1117` noting the pre-FSR-default capture date and pointing to the already-correct config-labeled "TAA (native)"/"FSR Quality" columns.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
