# #5896: PAR-D4-2026-10-15-01: The Havok 2014 (v11) surface has no real-data census and no default-lane fixture

- **Labels**: medium,bug,animation,test-gap,game:fo4
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: The Havok 2014 (v11) surface has no real-data census and no default-lane fixture. Clips, skeletons, behaviors, THREECOMP48, the predicate area, `LAYOUT_2014`, the modifiers and the reference frame are each gated by at most one hand-picked file.

- **Severity**: MEDIUM (a missing gate; it already hid the 856 rejects in D5-01)
- **Dimension**: Corpus Gates
- **Location**:
  - `crates/hkx/src/animation.rs:2329-2369` (`fallout_4_skeleton_and_clip_decode`: one skeleton, one clip)
  - `crates/hkx/src/behavior.rs:1606-1652` (`fallout_4_havok_2014_graphs_decode`: 3 graphs)
  - The FO4 census lives only in examples (`examples/clip_census_ba2.rs`, `behavior_census_ba2.rs`), which no lane runs.
  - `packfile.rs` `fixtures::PackfileBuilder` writes v8 only (`0x0C = 8`, 0x30 section headers). The only v11 mention in the default lane is the contents-mismatch rejection at `packfile.rs:665`.
- **Status**: NEW. #5446 (dimension-gate fixtures) is separate and not re-filed.
- **Description**:
  - The SE clip census (`skyrim_se_spline_dimensions_census_stays_under_the_gate_ceilings`) pins 6,126 clips with an empty reject set. Nothing comparable exists for:
    - FO4: 13,990 clips, 121 skeletons, 328 behaviors, 222 characters
    - SE behaviors: 423
  - In CI, no test exercises:
    - the 0x40 section header
    - a non-zero predicate area
    - `animation_type == 3`
    - `decode_three_component_48`
    - `read_reference_frame`
    - any `LAYOUT_2014` offset
    - `read_modifier`
  - Every one of those paths could regress with the default lane still green.
  - The strict nightly `parsers` job would run the ignored FO4 tests, but it has no runner (count 0, per the skill). The ignored tests do fail under `BYROREDUX_REQUIRE_GAME_DATA`, so they are not vacuous there.
- **Evidence**: census numbers in `/tmp/audit/parsers/probe-hkx/census.txt`. Of the 3 SE `*_lod.hkx` behaviors that fail (`frostbitespiderbehavior_lod.hkx` and two more), none is pinned anywhere.
- **Suggested Fix**:
  - Add `fallout_4_hkx_census_is_exact`: exact clip, skeleton, behavior and character counts; the lossless set as a named, counted bucket; the reject set otherwise empty; a reference-frame count.
  - Add an SE behavior census.
  - Extend `PackfileBuilder` with a version-11 mode (0x40 headers, predicate size) plus unit tests for THREECOMP48 decode, type-3 gating and the reference-frame offsets.
- **Merged from the gates agent (its PAR-D4-2026-10-15-40, same gap, independent census)**:
  - **Severity**: MEDIUM
  - **Dimension**: Corpus Gates
  - **Location**:
    - `crates/hkx/src/animation.rs:2331-2373` (`fallout_4_skeleton_and_clip_decode`): one skeleton and one clip, `1hm\walkforward.hkx`.
    - `crates/hkx/src/behavior.rs:1607-1650` (`fallout_4_havok_2014_graphs_decode`): three named graphs.
    - `crates/hkx/src/animation.rs:72-81`: the `MAX_TRANSFORM_SAMPLES` rationale, which cites the SE census only.
    - `crates/hkx/src/animation.rs:83-89`: the #5847 ceilings, documented as SE-census-derived.
    - `crates/hkx/examples/clip_census_ba2.rs` and `behavior_census_ba2.rs`: whole-archive FO4 censuses with **0 assertions**, which no lane runs.
    - `.github/workflows/real-data-gates.yml:208`: the `parsers` loop, which does run the two FO4 tests.
  - **Status**: NEW. The SE census (`skyrim_se_spline_dimensions_census_stays_under_the_gate_ceilings`) is exact. FO4 support (a7bbba273, 60082cc55, 49529db7c, all 2026-10-14) added a second packfile generation with no equivalent. Distinct from #5446, which covers the unit fixtures.
  - **Trigger Input**: n/a (gate gap). Regression class: any change to `Packfile::parse` v11 (the predicate area or the 0x40-byte section headers), spline type 3, THREECOMP48, or `hkaDefaultAnimatedReferenceFrame` that breaks clips other than `1hm\walkforward.hkx` or graphs other than the three named ones.
  - **Description**:
    - FO4 humanoids now run graph-driven locomotion and idles from this reader, but the strict lane checks 2 clips and 3 graphs out of 15,320 files.
    - The census code already exists, but only as print-only examples. That is the "census exists but cannot fail" pattern #5326 / #5671 removed for SE.
    - The dimension ceilings were never re-measured against FO4. The doc comment calls `MAX_TRANSFORM_SAMPLES` = 1,000,000 "8× the measured maximum" (SE: 124,821). FO4 vanilla reaches 584,155, so the real headroom is 1.71×.
    - A modded voice/lip clip about 1.7× longer than Magnolia's line would be rejected, and no gate would show how close vanilla sits.
    - The two FO4 test helpers (and `behavior.rs:1502-1516`'s SE helper) also do not make a set `BYROREDUX_FO4_DATA` / `_SKYRIMSE_DATA` binding (#3850). A set-but-wrong override silently skips outside the strict lane, whereas `animation.rs:1425-1431` asserts `is_dir()`.
  - **Evidence**: probe `fo4hkx`, real `decode_spline_animation` / `decode_behavior_graph`, release, every BA2 in the FO4 Data dir.

    | Scope | `.hkx` files | Clips decoded | Non-clip (`MissingClass`) | Rejects | Dropped-track clips | `behaviors\` graphs decoded |
    |---|---|---|---|---|---|---|
    | `Fallout4 - Animations.ba2` | 15,320 | 13,514 | 1,806 | 0 | 0 | 328 |
    | All BA2s (including DLC, CC, SS2, UFO4P) | 18,863 | 16,187 | 2,676 | 0 | 0 | 461 (0 errors) |

    - Maximum frames: 6,149. Maximum tracks: 120.
    - Maximum samples, vanilla: **584,155** (`sound\voice\fallout4.esm\npcfmagnolia\0019ddf3_1.hkx`, 95 × 6,149). Mods: 559,360 (SS2).
    - No non-finite sample appeared in any decoded clip.
    - SE side (probe `sebeh`): 420 SE `behaviors\` graphs decode. 3 `*_lod.hkx` graphs (frostbitespider, goat ×2) fail `MissingClass("hkbBehaviorGraphStringData")`. 343 of 345 character files decode. None of this is gated either; only the swinging blade and the Nordic lever are pinned.
  - **Impact**: today's vanilla state is fully green, but nothing holds it there. A v11 regression would surface only as FO4 actors sliding or idling wrong, and the cap documentation overstates its margin by about 4.7×.
  - **Related**: #5326, #5671, #5316, #5847, #5446, #3850.
  - **Suggested Fix**:
    - Promote `clip_census_ba2` to an `#[ignore]` test over `Fallout4 - Animations.ba2`, pinned exactly at 15,320 / 13,514 / 1,806, rejects as the empty set, 0 partial, max frames 6,149, and `max_samples <= MAX_TRANSFORM_SAMPLES`.
    - Add a graph census pinned at 328 for FO4 and 420 plus the named 3-file reject set for SE.
    - Correct the `MAX_TRANSFORM_SAMPLES` doc to cite the FO4 maximum, or re-derive the cap from it.
    - Make the three helpers' env override binding.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
