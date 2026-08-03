# SF-D7-01: game-compatibility.md still states stale 99.64%/#746/#747 figures contradicting its own updated matrix row

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2365
**Labels**: documentation,low,tech-debt

---

**Severity**: LOW
**Dimension**: 7 — Real-Data Validation (Starfield audit, 2026-08-03)
**Location**: `docs/engine/game-compatibility.md:13-19`, `:194-196`, `:399`
**Status**: NEW, CONFIRMED against current code

## Description

The doc's per-game matrix row (line 38) was correctly updated to 99.99%/#2105/"mis-attributed #746-#747", but three other spots in the same file (summary prose, Tier-2 section, long-tail drift section) were missed in that reconciliation pass and still assert the pre-fix 99.64%/#746-#747 numbers as current fact. `#746`/`#747` are themselves closed issues.

## Evidence

Confirmed by direct read of `docs/engine/game-compatibility.md`:
- Line 38 (correct): "**99.99%** aggregate ... MeshesPatch's populated-`BSWeakReferenceNode` truncation tail (was 325/29 849, mis-attributed to closed #746/#747) fixed by #2105 ..."
- Line 18-19 (stale): "99.64% aggregate on Starfield (the MeshesPatch terrain-overlay truncation tail, #746/#747)"
- Line 194-196 (stale): "**99.64% aggregate clean** ... residual drift tracked under #746/#747"
- Line 399 (stale): references "#746/#747" as still-open tracked drift

## Impact

Doc-only. Risk is a future contributor citing the stale figures or re-opening closed issues. Same failure mode as the already-tracked #2264 (TD6-001) ROADMAP doc-rot finding, different file.

## Suggested Fix

Sync the three stale spots (lines 18-19, 194-196, 399) to line 38's corrected text (99.99% aggregate, #2105 fix, mis-attributed #746/#747).

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix, no behavior change
