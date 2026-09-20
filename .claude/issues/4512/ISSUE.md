# REN-D9-2026-09-20-03: vkCmdCopyBufferToImage mip regions exceed the staging buffer by exactly 8 bytes (one BC1 block) on Skyrim deep-mip texture uploads

- **ID**: REN-D9-2026-09-20-03
- **Labels**: high,renderer,memory,bug,game:skyrim
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: HIGH · **Dimension**: Memory/Lifecycle
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D9-2026-09-20-03)

**Location**: Staging-region builder vs `total_data_size` budget on the DDS upload path (`crates/renderer/src/vulkan/texture.rs`); observed as pRegions[8] 349536 > 349528 and pRegions[10] 5592416 > 5592408

**Description**
On the validated Skyrim `WhiterunBanneredMare` run (NOT on FNV), validation reported 10 runtime errors of the form `vkCmdCopyBufferToImage(): pRegions[8] is trying to copy 349536 bytes to/from the VkBuffer … which exceeds the VkBuffer total size of 349528 bytes`. Every instance is exactly +8 B = one BC1 block row tail — the classic div_ceil(w,4) vs truncated-multiply disagreement on final mips between the per-mip region list and the staging allocation. FNV clean suggests a Skyrim-specific texture dimension class.

**Evidence**
Live sync-validation output (10 occurrences, deep-mip ≥11-mip textures), captured by the 2026-09-20 renderer audit D9 run.

**Impact**
Vulkan spec violation on the default texture-upload path; a strict driver may drop the copy or fault — silent missing mips on Skyrim deep-mip chains at best.

**Suggested Fix**
Diff total_data_size against the sum of per-mip region.size for BC1/BC3 odd sub-block dimensions; unify the div_ceil block math on both sides and add `debug_assert!(regions.iter().map(|r| r.size).sum::<u64>() <= staging_size)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
