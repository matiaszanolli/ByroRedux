# REN-3-2026-09-20-02: four sibling hash views in scene_buffer/descriptors.rs hand-roll from_raw_parts after #4445 declared byte_view 'the single place'

- **ID**: REN-3-2026-09-20-02
- **Labels**: low,renderer,tech-debt,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: GPU-Struct Layout
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-3-2026-09-20-02)

**Location**: `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:462-538`

**Description**
#4445 routed the material byte views through the bounded NoUnnit helper and its doc claims singularity; four sibling hash/buffer views in the same file still hand-roll `slice::from_raw_parts`. Three of the four are convertible; the ash-typed one legitimately is not (document the exemption).

**Evidence**
grep during the 2026-09-20 audit (D3).

**Impact**
The next bytemuck-shaped unsoundness fix has to find these by hand again; the 'single place' doc is false as written.

**Suggested Fix**
Convert the three convertible sites to byte_view; annotate the ash-typed exemption at the doc claim.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
