**Severity**: HIGH · **Dimension**: 2 — Memory Corruption / UB (SSBO indexing / AS build input)
**Location**: `crates/renderer/src/mesh.rs:387-404` (`accumulate_global_geometry`)
**Source**: `docs/audits/AUDIT_SAFETY_2026-06-14.md` (SAFE-D2-NEW-02; carryover of unpublished 2026-06-11 SAFE-D6-NEW-01)

## Description
Commit `01251733` added a `#markarth-fragments` diagnostic that detects a mesh whose maximum local index is `>= vertices.len()` — but it only `log::error!`s and then **uploads the mesh anyway**: the `pending_vertices.extend_from_slice` / `pending_indices.extend_from_slice` calls at `mesh.rs:403-404` run unconditionally right after the check, with no `bail!`, clamp, or skip. The code is byte-for-byte unchanged since the June 11 audit flagged it; no issue was ever opened.

## Evidence
- `mesh.rs:388-401` — `if max_idx as usize >= vertices.len() { log::error!(...) }` (no early return); `:403-404` append to the global pool regardless.
- `device.rs` never enables `robustBufferAccess` (no `robust_buffer_access` hits anywhere in `crates/renderer/src/`), so an out-of-range vertex fetch is UB, not a clamped read.
- Static BLAS builds declare `max_vertex(vertex_count.saturating_sub(1))` (`blas_static.rs`); an index above `maxVertex` is an invalid AS build input per the Vulkan spec.

## Impact
A self-inconsistent (index, vertex) pair — from a NIF decode remap bug (the class the diagnostic was added to bisect), a corrupt file, or a mispointed CSG offset (see SAFE-D2-NEW-03) — produces (a) raster reads into *other meshes'* vertices in the shared global pool (the "exploding spike" artifact), (b) for a pool-tail mesh, an OOB GPU vertex fetch with robustness off (UB, potential DEVICE_LOST), and (c) an invalid BLAS build input. GPU-level UB on the AS/SSBO-indexing axis ⇒ HIGH (impact-based; per the severity table, wrong SSBO index / AS geometry is the CRITICAL family, here gated behind a malformed-decode trigger so held at HIGH).

## Related
SAFE-D2-NEW-03 (a producer that can emit exactly this); #1392 (CLOSED — the analogous `instance_custom_index` guard was hardened from debug-only to a release runtime check, the template for this fix); 2026-06-11 audit finding SAFE-D6-NEW-01 (never published).

## Suggested Fix
Turn the guard into a hard gate — `return` (skip the mesh, keep the log) or clamp offending indices to `vertices.len() - 1` before appending. The diagnostic value is preserved either way; the upload of known-inconsistent geometry is not.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related geometry-accumulation / index-upload paths (skinned vs static pools)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (an overshoot mesh is skipped/clamped, not appended)
