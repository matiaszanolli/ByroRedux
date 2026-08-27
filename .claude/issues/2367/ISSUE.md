# PERF-REGRESSION-3a02b02d..28155b79: FO4 scenes (MedTek/Dugout) ~33-34% slower at flat entity count; Prospector (FNV) ~2x faster — needs bisection

**Severity**: MEDIUM
**Dimension**: Performance / Bench-of-record (ROADMAP #2279 refresh)
**Location**: unknown — needs bisection across `3a02b02d..28155b79` (119 commits, spans Session 60-62 including procedural volumetric fog, clustered local fog volumes, material-aware path-traced GI extensions, materials-pipeline refactor `ImportedMaterial`/`MaterialTextureSet<T>`, streaming-resumability mitigations)

## Description

Refreshing the ROADMAP bench-of-record (#2279) surfaced large swings against the prior record (`3a02b02d`, 2026-07-26). Per this project's own standing methodology (documented in ROADMAP's Bench-of-record section), a same-session same-machine worktree rebuild of the prior commit was run as a control before drawing any conclusion — this is what separated PERF-REGRESSION-6c56e311 (#2161-adjacent) from machine noise previously, and does the same here.

Control (`3a02b02d` rebuilt in a worktree) vs HEAD (`28155b79`), same session/machine, TAA config, median of 3 runs x 300 frames:

| Scene | Entities (ctrl→HEAD) | TAA frame ms (ctrl→HEAD) | Verdict |
|---|---|---|---|
| Prospector (FNV) | 3626→3626 (flat) | 14.69→7.33 | **Real ~2x improvement** |
| Cornell (synthetic control) | 25→27 (flat) | 2.76→3.32 | Real but mild slowdown (~20%) |
| Whiterun (Skyrim SE) | 3406→5150 (+51%) | 9.99→15.37 | Confounded by entity growth — not conclusive |
| MedTek Research 01 (FO4) | 31495→31400 (flat) | 40.17→53.58 | **Real ~33% regression, flat content** |
| Dugout Inn (FO4) | 6978→6978 (flat) | 30.44→40.79 | **Real ~34% regression, flat content** |

The control run reproduces the original `3a02b02d` ROADMAP figures closely (e.g. Prospector 65.3→68.1 FPS, Dugout 31.9→32.9 FPS — within normal same-machine noise), which is what makes the HEAD deltas trustworthy rather than contention artifacts.

## Evidence

Full control-run report and HEAD report available in this session's bench output (`target/fsr-bench/raw.tsv` at both commits). Both regressed scenes are Fallout 4 content; the dramatically improved scene is FNV; the synthetic control is only mildly affected — this points at something FO4-specific rather than a universal engine regression, but that is a pattern observation, not a root cause.

## Impact

Two real Fallout 4 interior scenes are ~33-34% slower in frame time at byte-identical entity counts. Whiterun's entity count grew +51% over the same range (3406→5150) for reasons not yet understood — worth separately investigating since it could itself be either a content-loading behavior change or a symptom of the same underlying cause.

## Suggested Fix

Bisect `3a02b02d..28155b79` using `scripts/fsr-bench-matrix.sh` restricted to Dugout (smaller/faster-loading of the two regressed FO4 scenes, better bisection candidate than MedTek) at TAA-only to narrow the commit range efficiently. Prime suspects given the commit range's content: the procedural volumetric fog / clustered local fog volumes work and the material-aware path-traced GI extensions (both Session 62), since those are the kind of per-fragment cost additions that would hit FO4's higher-poly interiors harder — but this is a hypothesis, not yet verified.

## Completeness Checks
- [ ] **BISECT**: Narrow `3a02b02d..28155b79` to the actual introducing commit(s)
- [ ] **WHITERUN-ENTITY-COUNT**: Understand why Whiterun's loaded entity count grew 3406→5150 (+51%) over the same range
- [ ] **PROSPECTOR-IMPROVEMENT**: The ~2x Prospector improvement is also unexplained — confirm it's real engine work and not a measurement artifact before citing it as a win
- [ ] **TESTS**: N/A until root-caused — this is a measurement/bisection issue, not a code-fix issue yet
