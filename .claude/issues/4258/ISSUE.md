# OB-D1-02: BSFaceGenNiNode has a dedicated block parser but no arm in the scene walker's as_ni_node, silently dropping the subtree

**Issue**: #4258 — https://github.com/matiaszanolli/ByroRedux/issues/4258
**Labels**: low,nif-parser,nif,game:oblivion,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 1 — NIF Version Handling
**Location**: `crates/nif/src/import/walk/mod.rs:102-143 (as_ni_node); parser at crates/nif/src/blocks/node.rs:1053-1059`
**Status**: NEW

## Description
`BSFaceGenNiNode` has a dedicated block parser (`crates/nif/src/blocks/node.rs:1053-1059`) but `as_ni_node` — the scene walker's dispatch used to recognize node-family blocks for hierarchical traversal (`crates/nif/src/import/walk/mod.rs:102-143`) — has no arm for it, so any `BSFaceGenNiNode` subtree is silently dropped from the imported scene.

## Evidence
Live corpus measurement during the Oblivion audit: 0 occurrences of `BSFaceGenNiNode` across the full 9,612-file vanilla+DLC Oblivion corpus — `BSFaceGenNiNode` is a FO3-era-and-later block type. Zero impact for Oblivion specifically.

## Impact
Zero impact on Oblivion content (0/9,612 files). Real impact is deferred to FO3+ head-mesh content (FaceGen), where this gap would silently drop the FaceGen node subtree rather than rendering it. Flagged during this audit for investigation by a NIFAL/character-focused audit on Skyrim/FO4 corpora, not fixed here.

## Related
None filed. Cross-referenced for the character/NIFAL audit domain.

## Suggested Fix
Add a `BSFaceGenNiNode` arm to `as_ni_node` (or to whichever node-family match the walker uses) so the subtree is traversed like other `NiNode`-family blocks, verified against FO3/Skyrim/FO4 FaceGen head-mesh content.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_OBLIVION_2026-09-11.md — findings verified against live code during this publish run.*
