# NIFAL-D9-NEW-01: Animation and Particles remain the only two declared NIFAL boundaries with no completeness/dispatch-coverage guard

URL: https://github.com/matiaszanolli/ByroRedux/issues/4167
Labels: bug, medium, nifal, test-gap

---

**Severity**: MEDIUM
**Dimension**: Completeness
**Tier Violated**: harness-coverage gap — no production tier violated (same classification precedent as the now-closed #2532)
**Game Affected**: all seven (harness-coverage gap, not a per-game data bug)
**Location**: `byroredux/src/material_translate.rs:2243-2244` (the scoping comment names this itself); no equivalent structural guard exists for `byroredux/src/anim_convert.rs::convert_nif_clip`, `byroredux/src/asset_provider/animation.rs::convert_hkx_clip`, or `byroredux/src/systems/particle.rs::apply_emitter_overlays`
**Status**: NEW (successor to closed #2532, which explicitly named Animation as "the next extension" when it fixed Lights and confirmed Collision)

**Description**: #2532 closed with Collision (pre-existing) and Lights (new) both gaining a completeness-style dispatch-coverage guard, and its own closing comment correctly re-scoped the remaining gap to Animation (`convert_nif_clip`/`convert_hkx_clip`) only. That leaves Particles (`apply_emitter_overlays`) unmentioned and still unguarded — the original #2532 body listed it as one of the ~5 boundaries in scope, but neither the issue's fix nor its closing comment addressed it, and no separate issue tracks it.

**Evidence**: `grep -rn "canonical_completeness_harness\|dispatch_coverage" -r crates/nif/src byroredux/src` finds exactly 3 guard modules: `material_translate.rs::canonical_completeness_harness` (Material), `import/collision/mod.rs::dispatch_coverage_tests` (Collision), `import/walk/lights.rs::light_dispatch_coverage_tests` (Lights). None reference `convert_nif_clip`, `convert_hkx_clip`, or `apply_emitter_overlays`. All three existing guards pass.

**Impact**: A silent field-drop or a whole-arm omission in `convert_nif_clip`/`convert_hkx_clip` or in `apply_emitter_overlays` would not be caught by any automated signal today; it depends entirely on a human audit sweep noticing, exactly the gap #2532 itself was filed to close for Collision/Lights.

**Related**: Successor/residual scope of closed #2532 (itself the successor of closed #2214).

**Suggested Fix**: For Animation, add a kitchen-sink-value harness analogous to Material's (both `convert_nif_clip` and `convert_hkx_clip` produce the same canonical `AnimationClip` target and both live in `byroredux`, so no crate-boundary obstacle applies). For Particles, `apply_emitter_overlays`'s failure mode is closer to Lights/Collision's ("a whole overlay field stops being unioned in") than Material's per-field copy, so a structural/revert-and-fail guard is the better fit. Update the Material module's scoping comment once either lands.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: New guards must live at the existing declared boundaries (`convert_nif_clip`/`convert_hkx_clip`, `apply_emitter_overlays`) — never a new parallel boundary. See `/audit-nifal`.
- [ ] **TESTS**: This finding IS a test-gap; closing it means adding the two missing completeness guards

