# SF2D2-04: .mesh suffix/geometries\ head composed unconditionally, contradicting the field's documented path-or-stem semantics

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2361
**Labels**: bug,nif-parser,low,legacy-compat

---

**Severity**: LOW
**Dimension**: 2 — BSGeometry Mesh Extraction (Starfield audit, 2026-08-03)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:70`
**Status**: NEW, CONFIRMED against current code

## Description

The importer always composes `geometries\{mesh_name}.mesh` with no inspection of `mesh_name` (`let canonical = format!("geometries\\{mesh_name}.mesh");`), but nifly (the cited wire-format authority) and this codebase's own block-level doc both document the field as holding *either* a bare stem *or* a full path. A `mesh_name` already carrying the prefix/suffix double-composes into a guaranteed miss.

## Evidence

Confirmed by reading `bs_geometry.rs:70` directly — the `format!` unconditionally prepends `geometries\` and appends `.mesh` with no case/separator-insensitive head/tail check.

## Impact

Zero on vanilla (every real `.mesh` name sampled is a bare 20-hex stem); affects authoring-tool output / mods using readable paths, where the mesh silently vanishes (compounded by SF2D2-03, #2357's silence on resolve misses).

## Suggested Fix

Skip the prepend/append when the name already carries them, reusing the case/separator-insensitive head test already written in `normalize_mesh_path`.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other `format!`-composed archive path in the importer has the same double-composition risk
- [ ] **TESTS**: A regression test pins a `mesh_name` that already carries `geometries\`/`.mesh` resolving correctly instead of double-composing
