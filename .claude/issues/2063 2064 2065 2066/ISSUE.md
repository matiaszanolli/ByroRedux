# Batch: #2063 #2064 #2065 #2066 — Dimension-2 logic-duplication (tech-debt)

All four LOW, all OPEN, all behavior-preserving dedup. Premises verified
against current code first (several issue line numbers are stale — the
walker→`dedicated_shader` and cell-loader splits moved sites).

## #2063 — TD2-104: `EMPTY_ABSORBED` duplicated between interior + exterior loaders
- **Verdict**: current — identical `static EMPTY_ABSORBED: OnceLock<HashSet<u32>>`
  + get_or_init in `load.rs:380` and `exterior.rs:415`.
- **Action**: added `pub(crate) fn absorbed_refs_or_empty(&HashSet<u32>, usize)
  -> &HashSet<u32>` to `precombined.rs`; both loaders call it. Trimmed the now-
  redundant inline comments to a one-liner pointing at the helper.

## #2064 — TD2-105: ImportedMesh→Vertex + local-AABB copy-pasted (object_lod vs placement_lod)
- **Verdict**: current — both blocks byte-identical (same fallback defaults,
  same min/max/centre/radius loop). Format-specific streaming logic left
  untouched (correctly separate).
- **Action**: new `cell_loader/lod_support.rs` with
  `imported_mesh_to_vertices(&ImportedMesh) -> Vec<Vertex>` and
  `local_aabb_center_radius(&[[f32;3]]) -> (Vec3, f32)`; both LOD paths route
  through them. Dropped the now-unused `Vertex` import from both files.
- **TESTS**: 2 unit tests in `lod_support` (fallback defaults; AABB centre/radius).

## #2065 — TD2-106: parent-child TRS composition hand-rolled at 6 sites
- **Verdict**: current. Verified all 6 sites use the identical formula before
  touching (no drift): full-TRS at `spawn.rs:663` (collision), `spawn.rs:1058`
  (mesh), `refr.rs:499` (SCOL, `outer_*` names), `placement_lod.rs:530`
  (`local*scale` commutative ordering); position-only at `spawn.rs:511` (light),
  `spawn.rs:578` (particle emitter).
- **Action**: added `GlobalTransform::compose_trs` (loose full-TRS) +
  `compose_translation` (loose translation-only); `compose` now delegates to
  `compose_trs`. Routed all 6 sites through the new helpers — single home for
  the composition order.
- **TESTS**: `compose_trs_matches_inline_formula_and_compose` pins compose_trs /
  compose_translation to the exact old inline formula and asserts `compose`
  agrees component-for-component.

## #2066 — TD2-107: `compressed_mesh.rs` reimplements `read_vec4` inline 7×
- **Verdict**: current — 7 identical 4×`read_f32_le` `[f32;4]` clusters, no
  `use super::read_vec4`.
- **Action**: `use super::read_vec4;` + replaced all 7 (scale, _scale_copy,
  aabb_min, aabb_max, transform translation/rotation, chunk translation).
- **SIBLING**: swept all collision siblings — the two `constraints.rs` "clusters"
  the scan flagged are 6 distinct scalar constraint fields
  (cone/plane/twist angles + friction), not a vec4; constraints.rs already uses
  `read_vec4` for its real vec4s. No other unswapped site.

## Verification
- `cargo test -q` full workspace — all green (72 ok-suites); `-p byroredux-core
  -p byroredux-nif` clippy clean; my changed code is rustfmt-clean (fixed only my
  own lines, not the whole crate).
- **Out of scope / pre-existing**: `cargo clippy` on the `byroredux` binary
  surfaces `undocumented_unsafe_blocks` findings in `crates/renderer/`
  (`composite.rs`, `frame_upscaler.rs`, presentation/FSR path) — my diff touches
  zero renderer files; these are pre-existing from the session's FSR work, not
  introduced here.
