# Investigation — #1297 (title/body mismatch)

## Mismatch — both findings already accounted for
#1297's **title** is `C-1` (skin-pool `overflow_attempt_count` cumulative-per-call
telemetry) — that finding is **already fixed and CLOSED as #1296** (D12-C1). Its
**body** is `DIM12-A-01` (latent OOB compute write) — the OPEN, actionable bug, also
filed as **#1298**. So I fixed the body (DIM12-A-01); the title's C-1 needs nothing.
Third title-swap of the session (cf. #1304, #1309).

## Body finding — DIM12-A-01 — CONFIRMED
`context/draw.rs` first-sight skin loop: `needs_slot = !skin_slots.contains_key(eid)`.
An existing slot was reused verbatim — no comparison of the slot's allocated
`vertex_count` against the live `mesh.vertex_count`. The compute dispatch pushes
`push.vertex_count = mesh.vertex_count` and the shader writes
`outputVertexData[vid …]` for `vid in 0..push.vertex_count`, bounded only by the push
constant, NOT the slot's allocated `output_size`. If an entity's `mesh_handle` is
remapped to a larger-vertex mesh, the write runs OOB. `SkinSlot::vertex_count()`
existed with **zero callers** (confirmed via grep). Not reachable today (#907: no
in-engine path remaps entity→mesh between frames), but the BLAS refit side already
guards the identical remap via `validate_refit_counts` while the compute dispatch
runs *before* it → asymmetric protection. Severity medium (latent OOB).

## Fix
- `skin_compute.rs`: new pure predicate `skin_slot_capacity_stale(slot_vc, mesh_vc)
  -> bool` (= `slot_vc != mesh_vc`), placed next to `should_evict_skin_slot`. Uses
  `!=` for symmetry with `validate_refit_counts` (which also rejects on `!=`, not just
  growth — keeps the slot exactly matching the mesh). 3 unit tests (match / grew /
  shrank), mirroring the `validate_refit_counts` test idiom — GPU-free.
- `context/draw.rs` first-sight loop: when a slot exists, if
  `skin_slot_capacity_stale(slot.vertex_count(), mesh_vertex_count)`, log + `remove`
  the slot + `destroy_slot` + `drop_skinned_blas` + set `needs_slot = true` so
  `create_slot` re-allocs to the new size. This **activates the previously-dead
  `vertex_count()` accessor** and makes the `SkinSlot` "sized for vertex_count"
  invariant load-bearing.

### Why immediate `destroy_slot` is safe here
`destroy_slot` is synchronous and requires no in-flight command buffer references the
slot's buffer (#1003). The first-sight loop runs after `draw_frame`'s
`wait_for_fences(in_flight[frame], in_flight[prev])` (draw.rs:234) — BOTH in-flight
frames' command buffers are retired — so no GPU work references the buffer. The idle-
eviction loop (draw.rs:1285) already relies on this same post-fence-wait safety for an
immediate `destroy_slot`. (The `pending_skin_unload_victims` deferred queue is only for
`unload_cell`, which runs *outside* `draw_frame` with no fence wait.)

## Completeness checks
- **UNSAFE**: no new unsafe (reuses `destroy_slot`/`drop_skinned_blas`); the immediate-
  destroy safety is documented via the fence-wait reasoning.
- **SIBLING**: BLAS refit side already guarded by `validate_refit_counts`; this adds the
  symmetric compute-side guard. Raster path reads the shared global vertex SSBO (no
  per-entity slot capacity to overflow) → N/A.
- **DROP**: reuses the established `destroy_slot` (frees descriptor sets + destroys
  output buffer) + `drop_skinned_blas`; no new Vulkan-object lifecycle. `Drop` unchanged.
- **TESTS**: 3 unit tests on `skin_slot_capacity_stale` (full renderer lib suite 289 → 292).

## Closes
#1297 (body DIM12-A-01) + the duplicate #1298. Title C-1 = already-closed #1296.
