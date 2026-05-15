# Issue #1060: Thread StagingPool through scene-load and per-frame geometry rebuild

**State:** OPEN (but already fixed)

Both TODOs were removed and the StagingPool was threaded through in
commit `e6192cc5` (Fix #1055). The pool now lives on
`MeshRegistry.geometry_staging_pool` with lazy-init on first call,
reused on every frame-loop rebuild.

- `byroredux/src/scene.rs:475` — `build_geometry_ssbo` no longer takes `None`
- `byroredux/src/main.rs:1148` — `rebuild_geometry_ssbo` no longer takes `None`
- `crates/renderer/src/mesh.rs:589` — pool stored on MeshRegistry, lazy-initialized

grep confirms zero remaining TODO markers.
