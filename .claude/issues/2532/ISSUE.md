# NIFAL-D9-04: The new canonical-tier completeness harness covers 1 of ~5 declared translate boundaries

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2532
**Finding ID**: NIFAL-D9-04

**Severity**: MEDIUM
**Dimension**: Completeness · **Tier Violated**: (harness gap — no production tier violated; same classification `#2213`/`#2214` used pre-fix)
**Game Affected**: all seven (harness-coverage gap, not a per-game data bug)
**Location**: `byroredux/src/material_translate.rs:571-574` (the scoping comment); no equivalent kitchen-sink module exists for `crates/nif/src/import/collision/shape.rs::resolve_shape_inner`, `byroredux/src/anim_convert.rs::convert_nif_clip` (+ `byroredux/src/asset_provider/animation.rs::convert_hkx_clip`), `byroredux/src/systems/particle.rs::apply_emitter_overlays`, or `crates/nif/src/import/walk/mod.rs`'s `LightKind` resolution
**Status**: NEW (successor to `#2214`'s residual scope; `#2214` itself is now closed as Material-scoped, verified genuinely functional by an independent revert-and-fail test)

## Description
The six translate-boundary bugs a prior sweep found and cited as evidence the completeness harness was needed were NIFAL-D6-01, D6-02, D3-01, D4-02, D6-03, D6-04 — four in Collision, one in Lights, one in Nodes. **Zero were in Material.** The kitchen-sink harness added by `#2214` (`byroredux/src/material_translate.rs:547-798`, `mod canonical_completeness_harness`) is scoped to Material only; its own doc comment says "collision/animation have no `translate_*` boundary yet to extend it to" — but per `docs/engine/nifal.md` itself, both categories *do* have declared, named, "converged"/"audited" boundaries. What's missing for those categories is not the boundary but a kitchen-sink canonical-output completeness test of the kind `#2214` just wrote for Material; the harness's scoping comment understates what already exists.

## Evidence
Confirmed directly: `grep -rln "kitchen_sink" crates/nif/src byroredux/src` returns only `byroredux/src/material_translate.rs`. The four fixed Collision bugs and the Lights bug were all caught by manual code tracing in prior sweeps — no automated harness existed for those categories then, and none exists now. Collision's own dimension independently caught and fixed two further boundary bugs this delta (`#2285`/NIFAL-D6-07, `#2298` triple-duplicated destrip logic) — again by manual trace, supporting evidence for this finding.

## Impact
The completeness *signal* is real for one of ~9 NIFAL categories. "Dimension 9 passes" cannot be read as "the translation layer's output is regression-tested" beyond Material — the other categories still depend entirely on manual audit sweeps catching drift.

## Related
Successor/residual scope of closed `#2214` (NIFAL-D9-02).

## Suggested Fix
Extend the `canonical_completeness_harness` pattern in priority order: Collision (`resolve_shape_inner` — highest historical bug count, 4/6), Lights (`LightKind` resolution — 1/6), Animation (`convert_nif_clip`/`convert_hkx_clip`). Also correct the scoping comment at `material_translate.rs:571-574` regardless of extension timing — it currently reads as though Collision/Animation have no boundary at all, contradicting `nifal.md`'s own "converged"/"audited" verdicts.

## Completeness Checks
- [ ] **TESTS**: Extend the kitchen-sink harness pattern to at least the Collision boundary (`resolve_shape_inner`) as the first extension
- [ ] **CANONICAL-BOUNDARY**: New harness modules mirror the Material harness's revert-and-fail self-verification approach
