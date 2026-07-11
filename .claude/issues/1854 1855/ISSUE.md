# #1854: FNV-D1-02: NIF-cache clip-handle commit ordered after insert-driven LRU eviction

Severity: LOW · import-pipeline
Location: `byroredux/src/cell_loader/references.rs:745-757`,
`byroredux/src/cell_loader/nif_import_registry.rs:221-223`

In the end-of-load batched commit, `pending_new` entries are inserted
first (running LRU eviction), then `pending_clip_handles` are committed
via `set_clip_handle` afterward with no residency check. If a key
inserted earlier in the same loop gets evicted by a later insert in the
SAME loop, its clip handle would be set for an already-evicted key —
never released (keyframe-array leak) + dangling clip_handles entry.
Reachable only when a single `load_references` call inserts more than
BYRO_NIF_CACHE_MAX (2048) unique NIFs. Not reachable on vanilla FNV
today — latent hardening only.

Suggested fix: commit pending_clip_handles before the insert loop, OR
guard set_clip_handle on residency + release otherwise.

# #1855: FNV-D1-03: Exterior terrain/water spawn results dropped without cell-level acknowledgement

Severity: LOW · import-pipeline
Location: `byroredux/src/cell_loader/exterior.rs:309, 335`

`terrain::spawn_terrain_mesh(...)` and `water::spawn_water_plane(...)`
both return Option<usize> but are called with `let _ = ...`. Callees
already warn internally on failure, but there's no cell-correlated
"cell (gx,gy) had no terrain/water" signal at the call site.
Observability-only, no correctness effect.

Suggested fix (optional): log a warn with cell coords when the result
is None, for per-cell correlation.
