# #3909: REN-2026-09-05-D7-02: GpuMaterial.texture_index is an undocumented unsampled lane sitting in the material dedup key

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3909 --json state`).*

---

**Audit**: `docs/audits/AUDIT_RENDERER_2026-09-05_DIM6_DIM7.md` (narrowed run — dims 6 & 7) · **Severity**: LOW

## Description

`GpuMaterial.texture_index` is an **undocumented sixteenth lane that no shader samples**, and it sits **inside the material dedup key**.

## Impact

Two costs. First, an unsampled field in the dedup key splits materials that are identical in every respect the GPU actually reads — inflating the material table and its uploads with entries that render identically. Second, an undocumented unsampled lane in a `#[repr(C)]` GPU struct is exactly the kind of drift the struct-sync invariant exists to catch, and it is not covered by any note saying why it is there.

## Suggested Fix

Determine whether `texture_index` is vestigial or reserved. If vestigial, remove it from the struct and the dedup key. If reserved, document it in `include/bindings.glsl` alongside the other lanes and exclude it from the dedup key so it stops splitting identical materials.

## Completeness Checks
- [ ] **SIBLING**: The other unsampled lanes noted by this audit reviewed with the same question
- [ ] **TESTS**: If the struct changes, the `GpuMaterial` size/layout pins are updated in lockstep (see #3846 on the currently-wrong documented size)

## Related
- #3846 (`bindings.glsl` documents `GpuMaterial` as 396 B, live 432 B — same struct, adjacent documentation defect)

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
