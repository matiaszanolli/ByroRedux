## Issue 2198 [OPEN] TD1-NEW-03: npc_spawn.rs re-crossed 2000 LOC after #2052's function-level fix (legitimate new AI-behavior code)
labels: bug low tech-debt 

## Severity
LOW

## Dimension
1 (File/Function/Module Complexity) — `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` (TD1-NEW-03)

## Location
`byroredux/src/npc_spawn.rs` (2777 LOC total), `apply_ai_package_behavior` (228 LOC, new, at line 1593)

## Status
NEW

## Description
#2052 (closed) extracted `spawn_npc_entity` down from 1045 to 828 LOC — that fix holds (confirmed live at line 716, 828 LOC). The file re-crossed the 2000-LOC threshold anyway (2400→2777 since 07-16) because `apply_ai_package_behavior` (228 LOC, confirmed live at line 1593) was added in the interim, consolidating what was previously a re-resolve-per-procedure pattern into a single-resolve dispatcher for the Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol behavior tags (itself the fix for the separately-closed PERF-D7-01 / #2031). No function in the file is newly oversized; this is pure file-level LOC growth from legitimate, already-reviewed feature work.

## Evidence
`wc -l byroredux/src/npc_spawn.rs` → 2777 (confirmed live); `apply_ai_package_behavior` at line 1593, 228 LOC; `spawn_npc_entity` confirmed at line 716, 828 LOC (down from the 1045 LOC #2052 targeted).

## Impact
None beyond the file continuing to tax full-file reviews; no single function needs decomposition today.

## Related
Existing: #2052 (CLOSED, function-level, not regressed) — this is a fresh file-level crossing with no open tracking issue.

## Suggested Fix
No urgent action required today. If the file grows further with the remaining ~10 unbuilt AI procedures, extract `apply_ai_package_behavior` and its seven `active_package_is_*`-driven arms into a sibling module (e.g. `npc_spawn/ai_package.rs`).

## Completeness Checks
- [ ] **SIBLING**: If/when split, follow the same per-family module pattern already used for `systems/{sandbox,wander,travel,follow,escort,guard,patrol}.rs`
- [ ] **TESTS**: N/A for this tracking note — existing tests (`apply_ai_package_behavior_tags_sandbox_from_active_package` et al.) already cover the consolidated dispatcher; a future extraction should preserve them unchanged


---
## Issue 2199 [OPEN] TD1-NEW-04: crates/nif/src/anim/tests.rs crossed 2000 LOC (2 lines over threshold)
labels: bug low tech-debt nif 

## Severity
LOW

## Dimension
1 (File/Function/Module Complexity) — `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` (TD1-NEW-04)

## Location
`crates/nif/src/anim/tests.rs` (2002 LOC)

## Status
NEW

## Description
Marginal, mechanical crossing (2 lines over the 2000 threshold, confirmed live via `wc -l`) via ordinary test accumulation on the KF-animation import path; no organizational problem, same pattern as the already-fixed `shader_tests.rs`/`particle.rs` test-split precedent from this same window (#2053/#2056).

## Evidence
`wc -l crates/nif/src/anim/tests.rs` → 2002 (confirmed live).

## Impact
None today — purely a threshold-crossing note for future split planning.

## Related
Same pattern class as #2053 (`particle.rs` split) and #2056 (`shader_tests.rs` split), both closed in this window.

## Suggested Fix
If/when next touched, split along the existing per-phase boundaries the sibling `anim/` modules already use (`coord`, `controlled_block`, `transform`, `sequence`, `keys`, `channel`, `bspline`). Not urgent — 2 lines over threshold.

## Completeness Checks
- [ ] **SIBLING**: When split, mirror the `shader_tests/{mod,legacy,skyrim,fo4,fo76,starfield}.rs` per-era-sibling precedent
- [ ] **TESTS**: N/A — test-only file reorganization, no behavior change


---
## Issue 2200 [OPEN] TD2-NEW-01: frame_upscaler.rs hand-rolls the same 4-image barrier shape instead of a local helper
labels: bug renderer low vulkan tech-debt 

## Severity
LOW

## Dimension
2 (Logic Duplication) — `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` (TD2-NEW-01)

## Location
`crates/renderer/src/vulkan/frame_upscaler.rs:592-640` (`record_fsr_barriers_before`)

## Status
NEW

## Description
Four of the six barriers built in `record_fsr_barriers_before` are byte-identical in every field except `.image(...)`: `.src_access_mask(COLOR_ATTACHMENT_WRITE).dst_access_mask(SHADER_READ).old_layout(SHADER_READ_ONLY_OPTIMAL).new_layout(SHADER_READ_ONLY_OPTIMAL)`, applied to `inputs.scene_color`, `inputs.motion_vectors`, `inputs.reactive`, `inputs.transparency` in turn (confirmed live at lines 604-634). This is a new occurrence of the same duplication class Dim 2 already fixed once this window (#2071/TD2-112, a different barrier shape) — the existing `descriptors.rs` helpers don't cover this specific same-layout/access-pair shape, so it wasn't reachable from the prior fix.

## Evidence
`crates/renderer/src/vulkan/frame_upscaler.rs:604-634` — 4 near-identical `vk::ImageMemoryBarrier::default()...` blocks differing only in the `.image(...)` argument, confirmed live by direct read.

## Impact
Cosmetic/maintainability only — all 4 are semantically correct today; a future barrier-shape change would need to be applied at 4 sites by hand.

## Related
#2071/TD2-112 (closed) fixed a different, GENERAL→GENERAL compute barrier shape in `descriptors.rs`; this is a distinct shape from a file outside that fix's scope.

## Suggested Fix
Add a small local closure or free function `fn shader_read_barrier(image: vk::Image, range: vk::ImageSubresourceRange) -> vk::ImageMemoryBarrier` in `frame_upscaler.rs` and call it 4×.

## Completeness Checks
- [ ] **UNSAFE**: `record_fsr_barriers_before` is already `unsafe fn`; the extraction adds no new unsafe surface — confirm the existing safety comment still covers the helper's call sites after refactor
- [ ] **SIBLING**: Check `descriptors.rs` and other barrier-recording sites (`gbuffer.rs`, `svgf.rs`) for the same same-layout/access-pair shape before generalizing the helper
- [ ] **TESTS**: N/A — pure refactor, no behavior change (barrier field values unchanged)


---
## Issue 2201 [OPEN] SF-D7-2026-07-25-01: #2105's BSWeakReferenceNode 2-byte-gap gate truncates 93.9% of Starfield - Meshes02.ba2 (regression of #2105)
labels: bug nif-parser high legacy-compat 

## Summary

The `#2105` fix (closed 2026-07-21, commit `b7e0318f`) added an undocumented
2-byte skip in `BsWeakReferenceNode::parse_inner`
(`crates/nif/src/blocks/node.rs:911-930`), gated on
`stream.bsver() >= crate::version::bsver::SF_FORM_ID` (173). That fix correctly
solved a real bug on `Starfield - MeshesPatch.ba2` (325/29,849 files, all
bsver 175) — but the `>= 173` gate is too broad and now misparses a
**different, much larger population**: vanilla `Starfield - Meshes02.ba2`,
which is uniformly bsver **173** (the exact gate boundary) and does **not**
carry the extra 2-byte field.

This is a regression of closed **#2105** (SF-D7-NEW-01), not a fresh bug —
filing per that convention with full byte-level evidence below.

## Description

`#2105` gates the 2-byte skip on the same threshold (`SF_FORM_ID` = 173) used
for the per-entry `formID` field, assuming the two properties correlate 1:1.
They don't: real `Meshes02.ba2` content is bsver 173 and lacks the field,
while the `MeshesPatch.ba2` content the fix was built and tested against is
bsver 175 and has it. Applying the skip at bsver 173 misaligns every
populated `BSWeakReferenceNode` block in `Meshes02.ba2`, corrupting the read
of `unkInt1`/`num_water_refs` and — because the resulting garbage water-ref
count implies a `skip()` past EOF — dropping the block to `NiUnknown`.

## Evidence

- `BYROREDUX_STARFIELD_DATA=... cargo test -p byroredux-nif --test parse_real_nifs parse_rate_starfield_all_meshes --release -- --ignored`
  fails: `[Starfield/Starfield - Meshes02.ba2] clean rate 6.10% (461 clean /
  7091 truncated / 0 failed)`. Sibling archives unaffected: Meshes01 100%
  (31,058/31,058), MeshesPatch 99.98% (29,843/29,849, matching the documented
  6-file residual), LODMeshes 100% (19,535/19,535), FaceMeshes 100%
  (1,282/1,282).
- `nif_stats --unknown-only` against `Starfield - Meshes02.ba2` confirms:
  `parsed 461 unknown 7091 type BSWeakReferenceNode` — the only type with any
  unknown count in the archive.
- `trace_block` byte-level decode of three independently-sampled truncated
  Meshes02 files (`lc179world.1.-2.1.nif`, `cydoniacity.1.1.3.nif`,
  `rl036world.1.-1.-1.nif`) all show `user_version_2 (bsver): 173`, and all
  fail at the same shape: the naive field walk (base NiNode -> 1 weak-ref
  entry with `formID`+transform+0 materials -> `unkInt1` -> `num_water_refs`)
  reads a huge garbage `num_water_refs` value 2 bytes into what the block's
  own declared `size` says should already be past the end of the block (one
  sample: declared `size=176`, but fields line up cleanly only if the
  `#2105` 2-byte skip is *not* applied — removing it lands
  `consumed == 176 == size` exactly).
- By contrast, `trace_block` on a `Starfield - MeshesPatch.ba2` file that
  parses cleanly today (`lc133world.1.-1.0.nif`) shows `bsver: 175` and
  consumes its declared block size exactly (8,970/8,970) *with* the 2-byte
  skip applied — confirming the skip is correct for bsver-175 content and
  wrong for bsver-173 content.
- `Starfield - Meshes01.ba2` (100% clean, unaffected) has **no**
  `meshes\terrain\*` content at all (checked via `d5_listba2`), which is why
  the base-game archive with the same era's bsver never exercises this code
  path.
- The regression test #2105 shipped
  (`bs_weak_reference_node_parses_populated_lists_with_undocumented_gap`,
  `crates/nif/src/blocks/dispatch_tests/nodes.rs:246-304`) hardcodes
  `user_version_2: 175` in its synthetic fixture and asserts only the
  2-byte-gap-present shape parses cleanly — there is no sibling fixture for
  the bsver-173/gap-absent shape, so the test suite could not have caught
  this before it shipped.
- `ROADMAP.md:245` states, under a `2026-07-21 sweep` byline (the same date
  #2105 landed): `Meshes02 **100%** (7 552)` — directly falsified by this
  run. The figure was legitimately 100% when first measured (commit
  `dd203a00`, 2026-04-28) and was not re-verified against real data after the
  #2105 change landed.

## Impact

7,091 of 7,552 (93.9%) NIFs in a vanilla Starfield mesh archive now lose
their entire `BSWeakReferenceNode` payload to `NiUnknown`. Current
player-visible/runtime impact is effectively zero — this payload (weak-refs,
water-refs) is not yet consumed by anything (feeds the unbuilt M35+
LOD-streaming/packin system per the code's own doc comment), and the content
in question (`meshes\terrain\*`) is exterior/LOD geometry, not the interior
Cydonia cell this project's cell-loading currently renders.

The real risk is (a) the project's own compat-matrix and prior audit now
cite a false 100%-clean figure for a whole archive, actively misleading
anyone reasoning about Starfield NIF coverage, and (b) even the 461 files
`nif_stats` calls "clean" likely still suffer the same 2-byte misalignment
silently (their water-ref-list is probably empty, so the resulting garbage
`num_water_refs` read happens not to overflow before the outer
block-size-table realignment silently recovers stream position) — meaning
this data would arrive corrupted, not just truncated, the moment a future
consumer reads it.

## Suggested Fix

Narrow the 2-byte-gap gate to the bsver range actually observed to carry the
field (empirically `>= 175`, not `>= SF_FORM_ID = 173`) rather than reusing
the `formID`-presence gate, since the two properties do not correlate 1:1 in
real content. Add a second synthetic regression fixture at
`user_version_2: 173` (mirroring Meshes02's real shape: 1 weak-ref entry, 0
materials, 0 water-refs, no 2-byte gap) so the test suite pins both
populations. Until fixed, treat the ROADMAP's Meshes02 100% figure as stale
and re-run `parse_rate_starfield_all_meshes -- --ignored` after any future
change to `BsWeakReferenceNode`.

## Related

- Regression of closed #2105 (SF-D7-NEW-01, 2026-07-16 audit, landed
  `b7e0318f` 2026-07-21).
- Sibling of the already-tracked residual-6 MeshesPatch truncation (also
  `BSWeakReferenceNode`, bsver 175, but a distinct and still-unexplained
  cause per that finding's own text — unaffected by this bug).
- Source: `docs/audits/AUDIT_STARFIELD_2026-07-25.md`, Dimension 7,
  finding SF-D7-2026-07-25-01.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other bsver-gated undocumented fields in `crates/nif/src/blocks/`)
- [ ] **TESTS**: A regression test pins this specific fix — add a bsver-173/gap-absent fixture alongside the existing bsver-175/gap-present one in `crates/nif/src/blocks/dispatch_tests/nodes.rs`
- [ ] **CANONICAL-BOUNDARY**: N/A — this payload is not yet consumed by NIFAL/`translate_material`; no per-game branching risk here


---
