# PERF-D1-2026-09-21-02: The ground-cover detail atlas and species table are rebuilt every frame (interiors too) just to compare an unchanged signature

**Labels**: bug, low, performance, terrain-exterior

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: LOW · **Dimension**: 1 — CPU Hot Paths
**Location**:
- `byroredux/src/app_frame.rs:43-90`: `publish_groundcover_detail_atlas`, called from `collect_and_prepare_groundcover` (`:765`) every frame unless `--groundcover-off` is set. It runs on every ray-query device, because the ground-cover pipeline is created whenever `ray_query_supported` (`crates/renderer/src/vulkan/context/init.rs:717`).
- `byroredux/src/render/groundcover.rs:488-543`: `build_groundcover_detail_atlas` (+ `linear_to_srgb8` at `:545`)
- `byroredux/src/render/groundcover.rs:656-739`: `collect_groundcover_species_table` → `species_selection_table`

**Status**: NEW (`1a7a22cf0`, `fd0cd577c`)
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

- `build_groundcover_detail_atlas` allocates `16×16×species×4` bytes (`GROUNDCOVER_DETAIL_ATLAS_EDGE = 16`). It runs `linear_to_srgb8` three times per texel for every species, and that function calls `powf` on its non-linear branch.
- The caller then compares `atlas.signature` and throws the pixels away when it is unchanged, which is the steady state. The signature hashes only the `colour_gradient` bits plus the species count.
- The species table builds five Vecs (`weights`, `clean`, `quotas`, `counts`, `order`) plus a sort every frame. Its result changes only with the palette or climate.
- Nothing gates this on exteriors. The collector runs in interiors too, whenever the ground-cover pipeline exists.

## Impact

- Today the palette is always one built-in species. `resolve_palette_for_chain` passes `Vec::new()` to `GroundCoverPalette::resolve`, which pushes one climate default. That costs about 768 `powf` plus about 6 allocations per frame, *est.* 15-20 µs.
- The cap is `MAX_GROUNDCOVER_SPECIES = 32`. Once the authored tier (#4413) fills it, the cost grows to about 24.6K `powf` per frame (*est.* ~0.5 ms).

## Related

- #4607 (PERF-D1-2026-09-21-01): the same per-frame ground-cover collector family (std SipHash maps and fresh allocations in the residency ring).
- #4413 (open): the authored-model tier that would multiply this cost. The fix should land before it.

## Suggested Fix

Compute the signature from the palette in O(species) and build pixels only on a mismatch. Alternatively, key both the atlas and the species table on a palette generation counter. Cache the species table under the same key.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D1-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: `collect_groundcover_species` is checked for the same rebuild-then-compare shape
- [ ] **TESTS**: A regression test pins that an unchanged palette rebuilds neither the atlas pixels nor the species table (for example, a build counter or a signature-only path)

