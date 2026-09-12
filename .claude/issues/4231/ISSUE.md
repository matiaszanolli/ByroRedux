# FO4-D1-01: fo4-csg-format.md Implementation-status section contradicts its own Reading-an-object section on BSCRC32

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4231

**Severity**: LOW
**Dimension**: 1 — M49 Precombined Geometry
**Location**: `docs/engine/fo4-csg-format.md:110-124` vs `:213-214`
**Status**: NEW

**Description**: The doc's "Reading an object" section (lines 110-124) fully documents the `BSCRC32`/`csg_name_hash` algorithm and states plainly that the cell's owning plugin is *not* a reliable substitute for resolving the `.csg` blob. The "Implementation status" section near the bottom (lines 213-214) still says the hash is "not yet reproduced" and that owning-plugin resolution is used "meanwhile" — the pre-#2369 state. `e9df743f` ("EX-15 (#2369): route FO4 precombines by their own CSG name hash") landed after `d6bf8437` (#1590) and the hash-based resolution has been fully implemented, tested against real Far Harbor rebake data, and in production since.

**Evidence**: Confirmed by reading both sections directly — the "Reading an object" section cites measured data (five DLCs re-bake ~460 `Fallout4.esm`-owned cells, decoding zero meshes if the owning-plugin substitute is used), while "Implementation status" contradicts it with stale phrasing.

**Impact**: Low — self-correcting for a careful reader, but a contributor skimming only the status section could try to "fix" already-fixed #2369 by reintroducing owning-plugin-only resolution.

**Related**: #1590, #2369

**Suggested Fix**: Update the status bullet to state the BSCRC32 hash is implemented and resolves the `.csg` blob per-object (#2369), while the `_oc.nif` filename path separately keys off the owning plugin (#1590).

## Completeness Checks
- [ ] **TESTS**: N/A (documentation-only fix)
- [ ] Doc updated to match the "Reading an object" section's already-correct description
