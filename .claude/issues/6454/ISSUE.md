# EXT-D6-2026-10-27-04: Two more doc blocks spliced onto the wrong item since 3e6f6b66a, missed by #6373's new scan

Labels: low, terrain-exterior, documentation, doc-rot
Source: docs/audits/AUDIT_EXTERIOR_2026-10-27.md

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-27.md` finding `EXT-D6-2026-10-27-04` (Dimension 6) · Severity LOW (doc-rot) · Status NEW (same class as closed #6373).

**Location (current)**: `byroredux/src/streaming_helpers.rs:559-604` (`drain_lod_reclaim_targets` doc); `byroredux/src/cell_loader/terrain_lod.rs:1979-1987` (the `#6409` doc above `synth_blocks_decode_vnml_with_the_terrain_decoder`).

## Description
Two more doc blocks were spliced onto the wrong item since `3e6f6b66a`, and #6373's new scan misses both:

1. `ab9d46d18` / `22d6e1a80` inserted `GROUND_VERTICES_PER_CELL` .. `exterior_pool_reserve` between the two halves of `drain_lod_reclaim_targets`'s doc. "Drain every rootless distant-LOD owner ... (#1536)." now documents `const GROUND_VERTICES_PER_CELL`, and the function keeps only its "Mirrors the `collect_victim_gpu_handles` ..." half.
2. `398a6b525` inserted `synth_blocks_decode_vnml_with_the_terrain_decoder` under the "#6409 — the streaming step keys both the rebuild and the attempted-miss cache on the pair ..." doc. That doc now heads the VNML test, and `the_ring_rebuilds_and_gates_on_the_hole_and_rim_pair` has none.

## Evidence
`source_scan::spliced_doc_blocks` (`546f5e2d4`) trips only on a doc that leads with a sibling's name, and its commit says so. Neither splice opens with one. Current text in `streaming_helpers.rs`: the "Drain every rootless distant-LOD owner out of a worldspace-streaming state ..." paragraph sits directly above `/// Per-pool cost of the ground ...` / `const GROUND_VERTICES_PER_CELL`, and `drain_lod_reclaim_targets` is preceded only by "Mirrors the `collect_victim_gpu_handles` (#1341) extraction ...".

## Impact
Rustdoc for a constant and a test describes unrelated items; the function that owns the #1536 reclaim-set contract loses half its doc. No behavioural effect.

## Related
#6373 (closed), #6410 (open; a different doc-loss site in `terrain_lod.rs`), SKY-D1-01 (same blind spot, AUDIT_SKYRIM_2026-10-27).

## Suggested Fix
Re-attach both doc halves to their items. Optionally widen `spliced_doc_blocks` to flag a doc whose first sentence names a symbol that is not the next item.

## Completeness Checks
- [ ] **SIBLING**: Re-run the splice scan over `streaming_helpers.rs` and `cell_loader/terrain_lod*.rs` after the fix
- [ ] **TESTS**: If the scan is widened, a fixture reproduces both splice shapes

