title:	NIFAL-D7-NEW-01: hkx crate's convert_hkx_clip is a second AnimationClip production boundary, undeclared in nifal.md
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	animation, documentation, low
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2305
--
**Severity**: LOW
**Dimension**: Animation · **Tier Violated**: single-boundary (spec gap)
**Location**: `byroredux/src/asset_provider/animation.rs:165-276` (`convert_hkx_clip`); undeclared in `docs/engine/nifal.md`'s Animation section
**Status**: NEW

## Description

The `hkx` crate's `convert_hkx_clip` is a second, legitimate production
boundary constructing the canonical `AnimationClip` (from Havok 2010
packfile data, not NIF — it cannot route through `convert_nif_clip`, the
spec's declared single boundary). It reuses the canonical type correctly (no
parallel struct) but is undeclared in `nifal.md`'s Animation section, which
names only `anim_convert.rs::convert_nif_clip`. It also synthesizes two
text-key events (`ExitCartEnd`/`IdleFurnitureExit`) not present in the source
clip — deliberate, well-commented, but an uncited fabrication in the spec's
framing.

## Evidence

`grep -n "convert_hkx_clip\|hkx" docs/engine/nifal.md` returns no hits;
`byroredux/src/asset_provider/animation.rs:165` defines `fn convert_hkx_clip(...)`
as a second canonical-`AnimationClip` producer.

## Impact

Doc-only — no behavior impact. But the spec currently implies exactly one
production boundary for `AnimationClip`, which understates the real surface
a future auditor or contributor needs to check for single-boundary
compliance.

## Suggested Fix

Add a paragraph to `nifal.md`'s Animation section naming
`convert_hkx_clip` as the second declared boundary (Havok-sourced, not
NIF-sourced), and note the two synthesized text-key events as an intentional,
documented exception to no-fabrication.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only fix — no behavior change to pin)

