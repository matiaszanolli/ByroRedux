# REN-D7-2026-09-20-02: second #3572 doc-rot cluster: taa.rs module doc (pre-move order, 'composite → tone-mapped swapchain'), taa.comp header, retired #4309/#2760 premises, FSR-promotion error! warns about the fixed #3572 crawl

- **ID**: REN-D7-2026-09-20-02
- **Labels**: low,renderer,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: TAA
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D7-2026-09-20-02)

**Location**: `crates/renderer/src/vulkan/taa.rs:3-14`; `shaders/taa.comp:8-12` and `:99-120`; `context/init.rs` promotion `error!`

**Description**
Distinct from REN-D4-2026-09-20-03 (which covers post_passes/rebind sites): the TAA pass's own module doc, shader header, and two rationale comments still describe the pre-#3572 world, and the FSR-startup promotion logs an error! about a silhouette crawl that #3572 itself fixed.

**Evidence**
Audit D7, 2026-09-20.

**Impact**
The resolve chain's self-description is wrong at the exact place a future TAA edit starts reading; a fixed bug is still being 'warned' about.

**Suggested Fix**
One doc sweep across the four sites; downgrade or delete the promotion error!.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
