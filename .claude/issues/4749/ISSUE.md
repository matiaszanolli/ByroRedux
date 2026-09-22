# #4749: SPT-2026-09-22-D6-01: #4229 overlay-divergence PBR recompute reopens #1819 foliage substring-collision class for the SpeedTree placeholder

**Labels**: high, speedtree, nifal, bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4749

## Description

`translate_material` (`byroredux/src/material_translate.rs`) resolves
`Material.metalness` / `Material.roughness` as
`recomputed_pbr.or(source.metalness_override).unwrap_or(NAN)` (roughness
identically). `recomputed_pbr` is computed — silently overriding whatever
`source.metalness_override`/`roughness_override` already hold — whenever
`overlay_changed_base_color && !source.bgsm_pbr_scalars_authored &&
source.metalness_override.is_some()` (lines 562-583).

Both the `ResolvedPaths::source_base_color` field doc (lines 159-175) and the
`translate_material` call-site comment (lines 553-561) justify the
`metalness_override.is_some()` half of that guard by asserting it "mirrors
`classify_legacy_pbr`'s own... gate... it's `Some` exactly when the
classifier's result was stored at import time." That's true for every
`crates/nif` mesh extractor, but **false** for `crates/spt`'s placeholder
importer. `placeholder_billboard_mesh` (`crates/spt/src/import/mod.rs:395-397`)
sets `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)`
directly — not via the keyword classifier:

```rust
// crates/spt/src/import/mod.rs:393-397
// Foliage is matte/non-metallic. Resolve this here instead of
// letting texture-name keywords misclassify leaves as wood/glass.
metalness_override: Some(0.0),
roughness_override: Some(0.85),
```

These are the deliberate anti-collision overrides **#1819** added specifically
because `classify_pbr_keyword`'s unbounded substring matching mis-tags
foliage: `"wood"` inside `ShrubBoxwoodLeaves*.dds`, and `"ic"+"e"` across the
`generIC`/`Elderberry` word seam in `ShrubGenericElderberryLeaves*.dds` (both
vanilla Oblivion tree species, per #1819's own evidence). The #4229 guard
excludes only the BGSM-authored case; it cannot distinguish a
keyword-classified `Some` from an explicitly-set-for-a-different-reason
`Some`. So a SpeedTree placeholder whose overlay-resolved base-color path
diverges from its own un-overlaid path has its `Some(0.0)`/`Some(0.85)`
protection discarded and replaced with a fresh `classify_pbr_keyword` run
against the new path — reopening the exact wood/glass collision #1819 closed,
now gated behind an overlay swap rather than unconditional.

## Evidence

```rust
// material_translate.rs:713-722
metalness: recomputed_pbr.as_ref().map(|p| p.metalness).or(source.metalness_override).unwrap_or(f32::NAN),
roughness: recomputed_pbr.as_ref().map(|p| p.roughness).or(source.roughness_override).unwrap_or(f32::NAN),
```

The #4229 regression test `overlay_swap_recomputes_pbr_from_the_effective_path`
(`material_translate.rs`, `mod overlay_pbr_divergence_tests`) proves the
mechanism *replaces* a `Some(0.9)/Some(0.55)` override with a freshly
classified value whenever the overlay path diverges and
`bgsm_pbr_scalars_authored` is `false` — exactly `placeholder_billboard_mesh`'s
configuration (`ImportedMaterial::default()` gives
`bgsm_pbr_scalars_authored: false`, `crates/nif/src/import/types.rs:862`, and
the placeholder never sets it otherwise). The three existing tests in that
module (no-divergence pass-through, divergence-recomputes,
BGSM-authored-protected) leave an untested gap for exactly this case: a
*non-keyword* `Some` override under a diverging overlay. Verified directly
against `crates/spt/src/import/mod.rs:382-398` and
`byroredux/src/material_translate.rs:562-583,713-722` at HEAD `c3f298a24`;
still present, unchanged in substance from the audit report (only line
numbers drifted ~190 lines from an unrelated intervening commit, `9d6fc4801`
Fix #4411).

## Impact

Reachability is narrow but real, not purely theoretical, and severity is
scored on impact, not likelihood (project severity policy). The trigger
requires `build_refr_texture_overlay`
(`byroredux/src/cell_loader/refr.rs:419-511`) to actually populate
`alt_texture_ref`/`land_texture_ref`/`texture_slot_swaps` for a TREE REFR
from an XATO/XTNM/XTXR/XMSP sub-record. Those are FO4-vocabulary fields;
TREE/`.spt` exists only on Oblivion/FO3/FNV, and the parser's own documented
provenance caveat (`crates/plugin/src/esm/cell/walkers.rs:1026-1040`,
FO3-D3-001/#1887) confirms none of those three games natively author a
*successful* overlay this way — FNV's `XATO` tag means something else there
(an Activation-Prompt string misread as a garbage FormID that near-certainly
misses `texture_sets`, i.e. an inert empty overlay, not a real swap). So
**vanilla content on all three `.spt`-bearing games cannot reach this today**.
What can: hand-authored mod content adding one of those sub-records directly
to a TREE REFR (no record-type gate blocks it in
`build_refr_texture_overlay`), or any future engine change that starts
authoring per-REFR texture overrides more broadly for these games. When it
fires, the effect matches #1819's own documented impact: visual-only, but the
Elderberry-class collision (the `"ice"/"glass"` arm, roughness 0.1) crosses
the RT-reflection threshold (`roughness < 0.6` in `triangle.frag`), rendering
the affected leaf billboard mirror-smooth. Per the severity matrix's
unconditional rule ("wrong/divergent `Material` out of NIFAL
`translate_material` → at least HIGH") and #1819's own precedent for the
identical defect class, this is scored HIGH rather than downgraded for the
narrow trigger.

## Related

- #1819 (CLOSED) — the original substring-collision bug this reopens a path
  to; its own fix is intact, not regressed.
- #4229 (CLOSED) — introduced this gap.
- #1887 (CLOSED) — the FO3-D3-001 provenance caveat that currently blocks
  vanilla reachability of this defect on all three `.spt`-bearing games.

## Suggested Fix

Narrow the `recomputed_pbr` guard so it fires only for materials the keyword
classifier actually produced. Cheapest correct fix: add an explicit
`pbr_classified_at_import: bool` to `ImportedMaterial` (default `false`), set
`true` only by `crates/nif`'s `classify_legacy_pbr` call sites, and gate
`recomputed_pbr` on that flag instead of `!bgsm_pbr_scalars_authored`. As a
stopgap, `crates/spt/src/import/mod.rs` could set
`bgsm_pbr_scalars_authored: true` on the placeholder material (the existing
BGSM-authored-scalars guard already treats that as "authoritative, do not
reclassify" — exactly SpeedTree's situation) — functionally correct today but
a misleading field name for a producer that ships no BGSM.

Source: docs/audits/AUDIT_SPEEDTREE_2026-09-22.md (SPT-2026-09-22-D6-01)
