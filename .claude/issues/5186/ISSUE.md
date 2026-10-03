# #5186: EXT-D6-2026-10-02-02: The `.btt` recon in object_lod.rs misreads the header, and its claim that `.lst` is "not needed" is false: the billboard size and atlas UV rect live only in the `.lst`

**Labels**: low,terrain-exterior,documentation,doc-rot,game:skyrim
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW
- **Dimension**: Distant LOD and trees
- **Location**: `byroredux/src/cell_loader/object_lod.rs:765-773` (the doc on `tree_lod_supported`); the same claim is in the 8e512b02e commit message and the #4913 comment.
- **Status**: NEW
- **Tier Violated**: no-fabrication
- **Game Affected**: Skyrim
- **Description**:
  - The comment reads the header as "three u32s (`15, 9, 21` — version, counts?)". It concludes that "the `.lst` lists are generation-time species data and not needed to consume the baked `.btt`".
  - The census shows something different. A `.btt` is a u32 **group count**, then per group a u32 **tree-type index**, a u32 count, and that many 32-byte records. Each record is: f32 x, y, z (world, Z-up); f32 rotation (radians); f32 scale; u32 REFR FormID; two zero u32s.
  - For `tamriel.4.4.-12.btt`, 15/9/21 means 15 groups, then type 9 with 21 trees.
  - The type index keys the `.lst`: u32 count, then 32-byte entries of u32 index, f32 width, f32 height, f32 u_min, v_min, u_max, v_max, u32.
  - The `.lst` is therefore the only source of each billboard's world size and its rect in `<ws>treelod.dds`. A consumer cannot render a `.btt` without it.
  - `exal.md:347-352` already says this ("the 9 `.lst` tree species lists that key them", "a `.btt`+`.lst` … consumer"). The code comment contradicts the spec.
- **Evidence**: A Python parse of `Skyrim - Meshes1.bsa` (LZ4 v105):
  - 380 of 386 `.btt` parse to exactly their length under this layout. Every type index is below its worldspace's `.lst` entry count (tamriel 34, dlc2solstheimworld 36, sovngarde 15, …).
  - The 6 exceptions are `dlc2solstheimworld.4.{12,16}.{4,8,12}.btt`, which carry trailing bytes past the declared groups (e.g. 2,796 of 3,244 bytes). That tail is unexplained.
  - `tamriel.lst` entry 0: index 0, 566.4 × 1521.5 BU, UV (0.809, 0.002)–(0.895, 0.250).
- **Impact**: A future consumer that follows the in-code recon would skip the `.lst` and have no billboard dimensions or atlas coordinates.
  - Side note: each `.btt` record carries a REFR FormID, so the tree tier *does* have per-object ids. #3307's premise that "baked quads carry no per-object ids" holds for `.bto` but not for `.btt`. That matters when VWD culling is designed.
- **Suggested Fix**: Replace the recon paragraph with the verified layout of both files, record the 6 Solstheim files that carry trailing data as open, and drop the "not needed" sentence. Note the per-tree REFR FormID for #3307.

---

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Every other copy of the stale text (specs, code comments, skill files) updated in the same change
- [ ] **TESTS**: If a source-scan or doc-sync test can pin the corrected statement, add it
