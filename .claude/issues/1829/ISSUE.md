# SF2-02: Stage A internal-geom find_map accepts the first Internal slot even when its body is empty

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1829
**Labels**: bug, nif-parser, medium, legacy-compat

**Severity**: MEDIUM
**Dimension**: BSGeometry mesh (Starfield audit Dim 2)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:32-35`
**Related**: Sibling of SF2-01 (Stage B / external-mesh emptiness variant, HIGH). Both stem from the same missing emptiness check, on the two different branches of `extract_bs_geometry`. Not a regression of #1209 — #1209 fixed slot-*kind* selection on this same Stage A branch (`first()` → `find_map()`); this finding is about slot-*emptiness*, which #1209 did not address.

## Description

The Stage A `find_map` (internal-geom branch, fixed for slot-kind selection by #1209) returns the first slot whose `kind` is `Internal { mesh_data }` regardless of whether `mesh_data.vertices` / `triangles` is empty. If an inline `Internal` slot is itself a `scale<=0` sentinel (the format permits inline sentinel slots) and a later `Internal` slot is populated, the post-loop guard at `bs_geometry.rs:80-82` drops the mesh. Lower severity than SF2-01 because vanilla Starfield ships external `.mesh` (inline is authoring-tool / port only per `crates/nif/src/blocks/bs_geometry.rs:208-212`), so the trigger is rarer.

## Evidence

`crates/nif/src/import/mesh/bs_geometry.rs:32-35`:
```rust
shape.meshes.iter().find_map(|m| match &m.kind {
    BSGeometryMeshKind::Internal { mesh_data } => Some(mesh_data.as_ref()),
    BSGeometryMeshKind::External { .. } => None,
})?
```
No emptiness check. Sentinel-empty body applies to inline slots too (`crates/nif/src/blocks/bs_geometry.rs:391-408`, reached via `BSGeometryMeshData::parse`).

## Impact

Inline-geometry Starfield / ported meshes with a sentinel-first slot order silently drop. Rare in vanilla, realistic in modded/ported content.

## Suggested Fix

Add the emptiness check to the `find_map` closure:
```rust
BSGeometryMeshKind::Internal { mesh_data }
    if !mesh_data.vertices.is_empty() && !mesh_data.triangles.is_empty() =>
    Some(mesh_data.as_ref()),
```
Fold into the SF2-01 test module.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the Stage B external-mesh branch (SF2-01) — both branches need the emptiness check
- [ ] **TESTS**: A regression test pins this specific fix (inline sentinel-before-populated slot ordering)

