# NIFAL-D4-2026-09-21-01: nifal.md §2 still says the .spt path keeps placement_root_billboard — dead since #3076; the #3533 fix sweep missed the spec

**Labels**: low, nifal, documentation, doc-rot

**Severity**: LOW · **Dimension**: Nodes · **Tier Violated**: — (doc rot in the dimension's ground-truth spec) · **Game Affected**: all games with TREE/spt exterior content
**Location**: `docs/engine/nifal.md:287-288`; code truth `crates/spt/src/import/mod.rs:214`/`:410`, `byroredux/src/cell_loader/references/import.rs:550-556`, `byroredux/src/cell_loader/spawn.rs:926-934` (branch "currently unreachable")
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
#3076 (`aee8783f2`, 2026-08-18) moved the SpeedTree billboard from the placement root onto the renderable mesh. Since then the spt root is a plain anchor (`billboard_mode: None`, pinned by `placeholder_uses_default_size_without_bounds`: "root is a plain anchor"), the quad carries `BILLBOARD_MODE_BS_ROTATE_ABOUT_UP` on `ImportedMesh.billboard_mode`, the cell path attaches `Billboard` per mesh (#2206), and `CachedNifImport::placement_root_billboard` is structurally always `None`. The spec paragraph still asserts the opposite. #3533's fix commit `57fdcc577` swept the code comments but missed this paragraph.

### Evidence
spt test asserts `imported.nodes[0].billboard_mode == None`; `AUDIT_SPEEDTREE_2026-08-30.md` §#3533 row ("`spawn.rs` is dead for `.spt`").

### Impact
`nifal.md` §2 is the declared ground truth for this dimension; the next Dim 4 audit starts from a false premise about the spt billboard contract.

### Related
#3533, #3076, #2206, #994

### Suggested Fix
Rewrite the sentence: since #3076 the spt billboard rides on the placeholder mesh via the #2206 per-mesh consumer; `placement_root_billboard` is a documented dead seam for a future `NiBillboardNode`-rooted producer (none exists).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
