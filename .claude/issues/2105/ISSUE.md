# SF-D7-NEW-01: MeshesPatch truncation tail mis-attributed to closed #746/#747; real cause is an unfixed BSWeakReferenceNode garbage-skip bug

**Severity**: MEDIUM
**Labels**: medium, nif-parser, legacy-compat, bug
**Location**: `crates/nif/src/blocks/node.rs:857-931` (`BsWeakReferenceNode::parse_inner`); doc citations at `ROADMAP.md:210`, `docs/engine/game-compatibility.md:19,38,196,396`
**Source audit**: `docs/audits/AUDIT_STARFIELD_2026-07-16.md` (SF-D7-NEW-01)

## Description
ROADMAP.md and `docs/engine/game-compatibility.md` cite closed #746/#747 for the residual 325/29,849 (1.09%) MeshesPatch truncation tail. Those issues actually fixed an unrelated `bsver == 155` vs `>= 155` gate bug in `shader.rs` (closed 2026-04-28). Re-tracing the real 325 files with debug logging shows every one fails inside `BSWeakReferenceNode` parsing — confirmed via the per-block histogram (`BSWeakReferenceNode parsed=7227 unknown=325`, the only type in the archive with any unknown count). Distinct from closed #1882 (empty-weak-ref-list case only); the populated-list case (real terrain-overlay files) was never covered and reproduces an identical magic garbage skip value (`skip(2359296)`) across three unrelated files of very different sizes — a structural fixed-offset misread, not per-file corruption.

## Impact
325 vanilla `meshes\terrain\*` NIFs lose their entire `BSWeakReferenceNode` payload to `NiUnknown` substitution. Currently benign at runtime (feeds a not-yet-built LOD-streaming system either way), but the stale doc citation actively misdirects anyone trying to close out the real bug toward already-closed, unrelated shader code.

## Related
Distinct from closed #1882 (empty-weak-ref-list case only). Distinct from closed #746/#747 (unrelated shader BSVER gate).

## Suggested Fix
Update the ROADMAP/compat-doc citations to point at this issue instead of #746/#747; byte-diff a populated-list `BSWeakReferenceNode` against the `parse_inner` field sequence.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
