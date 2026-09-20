# REN-D7-2026-09-20-03: bloom.rs module doc still describes pre-#2796 architecture ('composite adds … before tone-mapping', 'composite samples up_mips[0]')

- **ID**: REN-D7-2026-09-20-03
- **Labels**: low,renderer,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Bloom
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D7-2026-09-20-03)

**Location**: `crates/renderer/src/vulkan/bloom.rs:1-21`

**Description**
The module doc predates the in-place bloom_apply architecture (#2796) — outside the sites #4312's doc sweep covered.

**Evidence**
Audit D7, 2026-09-20.

**Impact**
Doc rot at the head of the file a bloom edit reads first.

**Suggested Fix**
Rewrite the module doc to the composite → bloom_apply-in-place → TAA/upscale reality.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
