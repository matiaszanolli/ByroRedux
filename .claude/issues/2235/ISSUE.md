# REN-D10-01: The new fog-volume system has no debug_assert tying it to the documented RT absolute-space precision ceiling

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2235

**Dimension**: 10 (RT precision)
**Location**: `crates/renderer/src/vulkan/volumetrics.rs` (fog-volume upload / dispatch path — absolute-world-space GPU consumer)
**Status**: NEW

**Description**: The local fog-volume system is a new absolute-space GPU consumer (fog volume centers/extents are stored and consumed in absolute world coordinates), but unlike other absolute-space consumers in the renderer, it has no `debug_assert` tying its coordinate magnitudes to the documented RT float-precision ceiling (the same ceiling that motivated `render_origin`-relative rebasing elsewhere, e.g. the caustic-splat `#markarth-precision` fix).

**Impact**: A fog volume authored or placed far from the origin (or an origin-rebase bug) would silently degrade into visible precision artifacts (jitter, banding) with no debug-build assertion to catch it early — the same class of bug the `render_origin` rebasing elsewhere in this renderer was built to catch loudly.

**Suggested Fix**: add a `debug_assert!` at fog-volume upload time checking volume-center magnitude against the same precision ceiling used elsewhere (e.g. wherever `render_origin` rebasing thresholds are asserted).

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
