# SK-D1-03 / #795 — BSTriShape inline tangents discarded; Skyrim+ has no per-vertex tangents

**Severity**: HIGH (currently dormant; reactivation blocker for #786 on Skyrim content)
**Domain**: nif-parser × renderer (Shader Correctness)
**Status**: NEW — tracking comment in code said "follow-up" but no GH issue existed

## Locations
- `crates/nif/src/blocks/tri_shape.rs:579-583` (inline parser `stream.skip(4)`)
- `crates/nif/src/import/mesh.rs:792-803` (importer ships `tangents: Vec::new()`)

## One-line summary
M-NORMALS (#783) only wired authored tangents for the NiTriShape pre-Skyrim path (`NiBinaryExtraData`). Skyrim BSTriShape stores tangents inline in the packed vertex buffer and the parser discards them — 18,862 Skyrim+ meshes feed empty tangents to the GPU.

## Fix shape
1. Decode the 4 tangent bytes at `tri_shape.rs:579-583` instead of skipping
2. Wire `shape.tangents` into `extract_bs_tri_shape` at `mesh.rs:802`
3. Add `synthesize_tangents` fallback for meshes without `VF_TANGENTS`
4. **Must land in same change as SK-D1-04 / #796** — sibling site for the SSE skin-partition path

Verify Bethesda `tan_u`/`tan_v` swap convention against authoritative reference per `feedback_no_guessing.md`.

## Audit source
`docs/audits/AUDIT_SKYRIM_2026-05-03.md` finding SK-D1-03.
