# DELTA-01: VWD VisibleWhenDistant marker is write-only + missing positive-path spawn test

**Issue**: #1890 · **Labels**: low, tech-debt, enhancement
**From**: docs/audits/AUDIT_INCREMENTAL_2026-07-05.md (DELTA-01 + §5) · **Introduced**: a8d65d6c (#1889)

Write-only `VisibleWhenDistant` marker (components.rs; inserted in cell_loader/references/mod.rs)
has no reader — intentional/documented hook for the deferred full-model LOD cull. Tracker for two
loose ends: (1) wire a reader when the cull lands (or delete the marker if abandoned); (2) add a
positive parse→spawn test asserting a VWD base record yields the marker on the placement root.
Negligible runtime impact. Related: #1889 (closed), #1731, #1866, EXAL §5.2.
