# EXT-D2-2026-10-26-01: `spawn_lod_block` lost its doc comment and `#[allow(clippy::too_many_arguments)]` to a new const (7eac91637)

Labels: low,terrain-exterior,tech-debt,doc-rot,documentation
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

## EXT-D2-2026-10-26-01: `spawn_lod_block` lost its doc comment and `#[allow(clippy::too_many_arguments)]` to a new const (7eac91637)

- **Severity**: LOW / **Dimension**: 2 (terrain-LOD, brief-named commit) / **Tier Violated**: n/a
- **Game Affected**: all LAND games (tooling / docs) / **Location**: `byroredux/src/cell_loader/terrain_lod.rs:842-851` and `:901` / **Status**: NEW
- **Description**: The skirt commit inserted `LOD_SKIRT_DROP` and `append_lod_skirts` between `spawn_lod_block`'s attribute+doc block and the
  function. The 5-line "Build + spawn one finest-band LOD block ..." doc and `#[allow(clippy::too_many_arguments)]` (line 847) now sit
  above `const LOD_SKIRT_DROP` (line 851); `fn spawn_lod_block` (line 901, 12 parameters) has neither.
- **Evidence**: lines 842-851 read `/// Build + spawn one finest-band LOD block...` / `#[allow(clippy::too_many_arguments)]` / `/// How far a skirt hangs...` /
  `const LOD_SKIRT_DROP: f32 = 640.0;`; line 901 is a bare `fn spawn_lod_block(`.
- **Impact**: rustdoc shows the spawn doc on the skirt constant and nothing on the function; clippy `too_many_arguments` fires on
  `spawn_lod_block` (static; clippy not run here - workspace clippy aborts at the first failing crate). Same defect class #6095 / open #6373
  describe, introduced after #6373 was filed (that issue's list does not include `terrain_lod.rs`).
- **Related**: #6373 (open, class), #6095 (closed), #5622 (reasonless allows), #6193.
- **Suggested Fix**: Move the doc block and the `allow` to directly above `fn spawn_lod_block`; keep `LOD_SKIRT_DROP` / `append_lod_skirts` above them.

## EXT-D6-2026-10-26-03: Displaced doc blocks and a displaced `#[allow]` from `7eac91637` and `125a21c2b`; stale premise in the skirt allowance comment

- **Severity**: LOW / **Dimension**: 6 / **Tier Violated**: n/a
- **Game Affected**: n/a
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:841-851` and `:136-139`; `byroredux/src/streaming_helpers.rs:835-843`
- **Status**: Existing class: #6373 (eight doc blocks spliced onto the wrong item); these sites are not in its table (`7eac91637` landed after its HEAD `c7463c04d`; `125a21c2b` is not listed).
- **Description**:
  1. `7eac91637` inserted `LOD_SKIRT_DROP` and `append_lod_skirts` between `spawn_lod_block`'s doc comment plus `#[allow(clippy::too_many_arguments)]` and the `fn`. The "Build + spawn one finest-band LOD block..." doc and the `allow` now sit on `LOD_SKIRT_DROP`; `spawn_lod_block` (12 parameters) has neither. rustdoc is wrong, and clippy's `too_many_arguments` would now fire on the function.
  2. `125a21c2b` left the `/// salvage is whether a cancel mid-FinishImports ...` paragraph (it documents `cancel_streaming_apply_job`'s new `salvage` parameter) glued above `salvage_one_queued_import`'s own doc; `cancel_streaming_apply_job` has none.
  3. The new skirt allowance comment (`terrain_lod.rs:136-139`) sizes against "at most 15 x 15 cells (240 edges)". The default streaming radius is 12 (25 x 25 cells = about 400 edges, 2,400 indices); the 4096/8192 allowance still covers it with margin, so no overflow, but the stated premise is the same stale ~radius-7 one #6094/#6095 retired.
- **Impact**: documentation and lint only.
- **Suggested Fix**: move the doc and `#[allow]` back onto `spawn_lod_block` (put the skirt helpers above its doc), move the salvage paragraph onto `cancel_streaming_apply_job`, correct the comment to the radius-12 count.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader copies, other sun/lighting sites, other rings)
