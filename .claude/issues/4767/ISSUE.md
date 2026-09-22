# TD1-2026-09-22-01: context/draw.rs crossed 2000 production LOC again (2019) — draw_frame chronic regrowth, 6th recurrence

**Labels**: low, tech-debt, bug

Filed via /audit-publish from docs/audits/AUDIT_TECH_DEBT_2026-09-22.md.

**Severity**: LOW · **Dimension**: 1 — File / Function / Module Complexity
**Location**: `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`: lines 1721-2462, 741 LOC)

**Status**: NEW (re-crossing; chronic site)
**Verified against**: HEAD `c3f298a24` — re-ran `prod_loc` live (2019, self-test ok). No commit in `ee6d3fb39..c3f298a24` touched this file.

## Description

The new AgX-tonemap/auto-exposure feature plus two small fixes (`#4602`, `#4604`) cumulatively pushed the file over the 2000-production-LOC line. `context/` is already split by pass/responsibility across ~19 sibling files; `draw.rs` holds the top-level `draw_frame` orchestrator plus two free helpers. This exact function/file has been split and regrown at least six times before, all previously CLOSED (#1052, #1748, #1857, #2197, #2255, #3282, 2026-06 through 2026-08; peak 4265 LOC file / 1844 LOC function at #1857).

## Impact

No functional impact — pure maintainability. Six-time-closed recurrence indicates the manual-split approach alone doesn't hold.

## Related

#4602, #4604, #4568 (sibling split this cycle), #1052, #1748, #1857, #2197, #2255, #3282.

## Suggested Fix

Extract the exposure/tonemap staging calls `draw_frame` currently inlines into a `context/exposure_dispatch.rs` sibling (or fold into `telemetry.rs`), on the established construct-vs-record-vs-teardown precedent. Given the six-time recurrence, also consider a structural guard — a size-budget test on `draw_frame` specifically, not just the file.

Source: docs/audits/AUDIT_TECH_DEBT_2026-09-22.md (TD1-2026-09-22-01)
