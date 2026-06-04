# Investigation — #1320 Test Hygiene

## TH6-NEW-01 Status: ALREADY FIXED
**Test**: `crates/plugin/tests/parse_real_esm.rs::dump_prospector_saloon_refrs` (lines 1545–1648)

Assertions have already been added (lines 1639–1647):
- `assert!(!rows.is_empty())` — at least one REFR resolved
- `assert!(rows.iter().any(|(_, mesh)| mesh != "<no base>"))` — at least one base-form linkage

The test now has meaningful regression guarantees. This issue is **CLOSED**.

---

## TH6-NEW-02 Status: OPEN
**Test**: `crates/nif/tests/translation_completeness.rs::cross_game_translation_completeness` (lines 258–328)

### Current State
- Test infrastructure is fully wired (MaterialStats collection, per-game iteration, printed comparison table)
- **Only assertion**: structural consistency (buffer-length invariant) — lines 315–327
- **Comment** (lines 309–314): explicitly defers fill-rate floor assertions as "future task"

### What's Missing
Per-game minimum fill-rate thresholds for key canonical Material fields:
- `with_texture_path` (texture_path populated)
- `with_material_path` (material_path populated for BGSM/FO4+)
- `with_material_kind` (material_kind != 0)
- `with_metalness_override` (metalness_override is Some)
- `with_roughness_override` (roughness_override is Some)
- `with_normal_map` (normal_map populated)
- `with_tangents` (tangents vector non-empty)

### Reference
ROADMAP compat matrix and prior per-game audit findings suggest baseline targets.

### Fix Scope
Add per-game assertions after the printed table (lines 302–307) using fill-rate closures matching the MaterialStats print formatting.

---

## TH6-NEW-03 Status: OPEN
**Test**: `byroredux/tests/golden_frames.rs::cube_demo_golden_frame` (lines 67–106)
**Baseline**: `byroredux/tests/golden/cube_demo_60f.png` (220 KB, created 2026-05-09 12:05)

### Current State
The baseline was captured before major shader changes:
- Disney-BSDF (fix #1163)
- Water caustics (fix #1459)
- Multiple RT lighting pipeline refinements

The baseline is now incompatible with HEAD's shader output.

### Options
1. **Regenerate**: Run `BYROREDUX_REGEN_GOLDEN=1 cargo test --release -p byroredux -- --ignored cube_demo_golden_frame` (requires Vulkan device)
2. **Retire**: Delete the baseline and replace the golden test with a pixel-statistics check (no golden file dependency)

Given the project's iterative nature and the likelihood of future shader changes, **retire the golden test and replace with pixel-stats** is lower-maintenance. However, since the infrastructure is already wired and the test is quick (60 frames @ ~16ms each = ~1s), **regenerate the baseline** is pragmatic for now.

---

## Completeness Plan
- [ ] Verify TH6-NEW-01 assertions are sound
- [ ] Research ROADMAP fill-rate baselines for TH6-NEW-02
- [ ] Add per-game thresholds to translation_completeness
- [ ] Regenerate golden baseline for TH6-NEW-03
- [ ] Run `cargo test --ignored` to verify all three pass
