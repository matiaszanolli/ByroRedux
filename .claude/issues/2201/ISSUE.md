# SF-D7-2026-07-25-01: #2105's BSWeakReferenceNode 2-byte-gap gate truncates 93.9% of Starfield - Meshes02.ba2 (regression of #2105)

- **Issue**: #2201
- **Severity**: HIGH
- **Dimension**: 7 (Real-Data Validation), regression of #2105 (SF-D7-NEW-01, 2026-07-16 audit)
- **Location**: `crates/nif/src/blocks/node.rs:911-930` (`BsWeakReferenceNode::parse_inner`, the `#2105` 2-byte skip gated on `stream.bsver() >= crate::version::bsver::SF_FORM_ID`); regression test gap at `crates/nif/src/blocks/dispatch_tests/nodes.rs:246-304` (`bs_weak_reference_node_parses_populated_lists_with_undocumented_gap`, hardcodes `user_version_2: 175`)
- **Source Report**: `docs/audits/AUDIT_STARFIELD_2026-07-25.md`
- **Labels applied**: `high`, `nif-parser`, `legacy-compat`, `bug`

## Description

#2105 fixed a real bug where populated `BSWeakReferenceNode` weak-ref lists on
real `Starfield - MeshesPatch.ba2` content (325/29,849 files, all bsver 175)
were mis-parsed because an undocumented 2-byte field sits between the
weak-ref array and `unkInt1`. The fix gates the 2-byte skip on
`bsver >= SF_FORM_ID` (173) — the same threshold that gates the per-entry
`formID` field. That threshold is too broad: real `Starfield - Meshes02.ba2`
content is uniformly bsver **173** (exactly the gate boundary) and does
**not** carry the extra 2-byte field, so the new skip misaligns every
populated `BSWeakReferenceNode` block in that archive, corrupting the read of
`unkInt1`/`num_water_refs` and — because the resulting garbage water-ref
count implies a `skip()` past EOF — dropping the block to `NiUnknown`.

## Evidence

- `BYROREDUX_STARFIELD_DATA=... cargo test -p byroredux-nif --test parse_real_nifs parse_rate_starfield_all_meshes --release -- --ignored` fails:
  `[Starfield/Starfield - Meshes02.ba2] clean rate 6.10% (461 clean / 7091 truncated / 0 failed)`.
  Sibling archives are unaffected: Meshes01 100% (31,058/31,058), MeshesPatch
  99.98% (29,843/29,849, matching the documented 6-file residual), LODMeshes
  100% (19,535/19,535), FaceMeshes 100% (1,282/1,282).
- `nif_stats --unknown-only` against `Starfield - Meshes02.ba2` confirms:
  `parsed 461 unknown 7091 type BSWeakReferenceNode` — the only type with any
  unknown count in the archive.
- `trace_block` byte-level decode of three independently-sampled truncated
  Meshes02 files (`lc179world.1.-2.1.nif`, `cydoniacity.1.1.3.nif`,
  `rl036world.1.-1.-1.nif`) all show `user_version_2 (bsver): 173` and all
  fail at the exact same shape: the naive field walk (base NiNode -> 1
  weak-ref entry with `formID`+transform+0 materials -> `unkInt1` ->
  `num_water_refs`) reads a huge garbage `num_water_refs` value 2 bytes into
  what the block's own declared `size` says should already be past the end
  of the block (one sample: declared `size=176`, but the fields as currently
  parsed only line up cleanly if the 2-byte `#2105` skip is *not* applied —
  removing it would land `consumed == 176 == size` exactly).
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
  (`bs_weak_reference_node_parses_populated_lists_with_undocumented_gap`)
  hardcodes `user_version_2: 175` in its synthetic fixture and asserts the
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
Cydonia cell this project's cell-loading currently renders. The real risk is
(a) the project's own compat-matrix and prior audit now cite a false
100%-clean figure for a whole archive, actively misleading anyone reasoning
about Starfield NIF coverage, and (b) even the 461 files `nif_stats` calls
"clean" likely still suffer the same 2-byte misalignment silently — meaning
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

## Fix with

`/fix-issue 2201`
