# #1866: LC0703-01: VWD full-model cull consumer untracked

- **Severity**: MEDIUM
- **Labels**: `medium`, `legacy-compat`, `bug`
- **Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-07-03.md` (LC0703-01)
- **Dimension**: EXAL — LOD distance rendering

## Location
- `crates/plugin/src/esm/reader.rs:384` (`is_visible_when_distant`, producer)
- `byroredux/src/cell_loader/object_lod.rs:297`, `byroredux/src/cell_loader/placement_lod.rs:395` (deferred consumers)

## Description
The VWD/"Has Distant LOD" flag now parses (#1731) but has zero production consumers — the object/placement LOD spawn paths use a coarser full-detail-ring rule instead.

## Impact
A full REFR at the full-detail/LOD boundary can render alongside its LOD proxy (z-fight). Conservative ring rule prevents it in the common case; the durable problem is the untracked consumer work.

## Related
#1731 (CLOSED, parse scope); #1849 (sibling remedy shape)

## Suggested Fix
Wire `is_visible_when_distant` into object-LOD/placement-LOD spawn paths to suppress the full REFR beyond the full-detail radius when its LOD proxy is active.
