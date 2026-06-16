# F6 (#1585) + F7 (#1586) — FO4 cell-streaming perf

Both from docs/audits/AUDIT_PERFORMANCE_2026-06-14.md. Domain: binary (byroredux).

## #1585 F6 — Geometry.csg re-opened per precombine cell-load
- `open_geometry_csg(plugin_path)` called unconditionally per spawn_precombined_meshes
  (precombined.rs:82); reached from exterior.rs + load.rs. Re-reads chunk table,
  drops warm ChunkCache each cell -> inter-cell zlib reuse lost.
- Fix: cache Option<Arc<CsgArchive>> on MaterialProvider, mirror sf_cdb Arc pattern,
  pass Arc into spawn_precombined_meshes.

## #1586 F7 — unbounded cell-spawn drain per frame
- main.rs:1071-1084 `loop { try_recv() }` drains ALL ready payloads/frame; each runs
  consume_streaming_payload -> load_one_exterior_cell (terrain + BLAS + water +
  precombines + uploads). No per-frame budget.
- Fix: cap steady-state drain at 1-2 cells/frame, break after cap. Leave
  stream_initial_radius blocking boot path uncapped.
