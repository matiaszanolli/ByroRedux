# #282: C2-02 — SVGF reads previous frame-slot G-buffer without fence synchronization

**Severity**: MEDIUM | **Domain**: renderer | **Type**: bug

## Finding
SVGF descriptor set binds `mesh_id_views[prev]` (`svgf.rs:460`). Fence wait only
covers `in_flight[frame]`, not `in_flight[prev]`. Frame N-1's render pass may still
be writing when frame N's SVGF reads the other slot's G-buffer. RAW hazard.

## Fix
Wait on both in-flight fences at the top of draw_frame.
