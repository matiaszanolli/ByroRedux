# REN-D4-2026-09-20-03: #3572 doc-rot cluster: six sites still describe the retired fall_back_to_raw_hdr/rebind mechanism; rebind_hdr_views is dead code kept alive only by the svgf anchor guard

- **ID**: REN-D4-2026-09-20-03
- **Labels**: low,renderer,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Sync/Barriers
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D4-2026-09-20-03)

**Location**: `crates/renderer/src/vulkan/context/post_passes.rs` and neighbors — six sites incl. a broken intra-doc link on `taa_failed`, a dangling #4006 comment at the fence-wait site, and `rebind_hdr_views` (whose only 'consumer' is the svgf source-shape guard's anchor text)

**Description**
#3572 retired the raw-HDR fallback/rebind machinery but left its documentation and one dead function behind; the svgf anchor guard matches text inside the dead function, so the guard is kept alive by doc rot.

**Evidence**
Audit D4, 2026-09-20.

**Impact**
The next reader of the fence-wait/raw-view path reconstructs a mechanism that no longer exists; the guard's anchor is one refactor from silently vacuous.

**Suggested Fix**
Delete rebind_hdr_views with its guard-anchor, fix the six doc sites and the broken link.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
