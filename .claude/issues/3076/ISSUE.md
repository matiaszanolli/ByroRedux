# SPT-D3-2026-08-16-01: .spt placeholder billboards never face the camera

**Issue**: #3076
**Severity**: HIGH
**Labels**: `high,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SPEEDTREE_2026-08-16.md` (Dimension 3 — placeholder billboard).

**Location**: `crates/spt/src/import/mod.rs`:322-329

## Description

`.spt` placeholder billboards **never face the camera** — the `Billboard` marker lands on the non-renderable placement root rather than on the mesh that is actually drawn.

## Evidence

```rust
// crates/spt/src/import/mod.rs:322-329 (re-verified 2026-08-17)
// Billboard mode for the .spt placeholder rides on the sibling
// `ImportedNode` root (`CachedNifImport::placement_root_billboard`,
// #994) — this is a different, per-mesh mechanism (#2206) for the
// general NIF `walk_node_flat` path that .spt scenes don't go
// through.
billboard_mode: None,
```

The comment states the mechanism explicitly: the billboard rides the placement root, and `.spt` scenes do not go through the per-mesh path that would apply it to the drawn geometry.

## Impact

Every `.spt` placeholder in FNV/FO3/Oblivion exteriors renders as a static quad at a fixed orientation instead of a camera-facing billboard. From most angles a tree placeholder is edge-on and effectively invisible.

The whole point of the placeholder-billboard fallback (Session 33 Phase 1) is to stand in for unparsed SpeedTree geometry; a billboard that does not billboard does not do that.

## Suggested Fix

Apply the billboard mode to the renderable mesh node rather than the placement root, or route `.spt` scenes through the per-mesh `#2206` mechanism the comment names.

## Related

- #994 (`placement_root_billboard`), #2206 (the per-mesh mechanism `.spt` bypasses)
- #3078 (SPT-D1-01 — the other reason `.spt` placeholders go missing)

## Completeness Checks
- [ ] **RENDERABLE-NODE**: The billboard flag lands on the node that is actually drawn
- [ ] **SIBLING**: Both `.spt` import paths (`scene/nif_loader.rs`, `synth_child.rs`) covered
- [ ] **COMMENT-TRUTH**: The `:322-329` comment matches the new behaviour
- [ ] **TESTS**: A regression test asserts the imported `.spt` placeholder carries a billboard mode

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3076 --json state` when live state is needed.*
