**From**: NIFAL audit `docs/audits/AUDIT_NIFAL_2026-06-13.md` (Dimension 5 — Particles)
**Severity**: LOW · **Tier Violated**: single-boundary (spec §1 "exactly one site per category — no duplicate construction sites")
**Game Affected**: all (any particle NIF)
**Location**: `byroredux/src/scene/nif_loader.rs` (~L531-554) and `byroredux/src/cell_loader/spawn.rs` (~L405-434)

## Description
The NIFAL particles slice centralised the *base-params* overlay into `apply_emitter_params` (`byroredux/src/systems/particle.rs:29`), and both load paths call it — good. But the three **adjacent** authored overlays are written as literal inline blocks duplicated verbatim at both call sites instead of behind the shared helper:

- `color_curve` → `preset.start_color` / `preset.end_color`
- `emitter_rate` → `preset.rate`
- `force_fields` → `preset.force_fields = convert_force_fields_zup_to_yup(...)`

The base-params helper exists precisely so "a field added in one place can no longer silently diverge the two load paths" (the rationale the spec uses for the Materials boundary, §3). The remaining three overlays do not get that guarantee.

## Evidence
`nif_loader.rs` color/rate/force blocks are mirrored field-for-field in `spawn.rs` — same shape, same assignments, copy-pasted (only the source struct binding differs, `emitter.*` vs `em.*`). Contrast `apply_emitter_params`, which both sites *do* call for the base params.

## Impact
Low today — each block is a trivial 1-3 line assignment from an already-centralised single-source value (`extract_first_color_curve` / `extract_emitter_rate` / `convert_force_fields_zup_to_yup`), so the divergence surface is small. But it is the "second construction site" smell the tier forbids: a future authored-overlay addition (e.g. wiring the #1333 modern-local-transform fix, or a size-over-life curve) must be hand-mirrored across two files.

## Suggested Fix
Fold the three overlays into the existing `apply_emitter_params` (or a sibling `apply_emitter_overlays`) taking the common `color_curve / emitter_rate / force_fields / emitter_params` subset, so both load paths call one helper — matching the `translate_material` template. The cell path's `Flat` variant and the loose path already carry the same four fields.

## Completeness Checks
- [ ] **SIBLING**: Confirm there is no *third* particle spawn site (e.g. NPC/effect spawners) that would also need the consolidated helper.
- [ ] **CANONICAL-BOUNDARY**: The consolidated helper is the single overlay boundary for particles — per-emitter logic stays here, not re-derived at render/sim time.
- [ ] **TESTS**: A test pins that color/rate/force overlays applied via the helper match the prior inline behaviour for both load paths (extend `apply_emitter_params_overrides_kinematics_and_size_not_color`).
