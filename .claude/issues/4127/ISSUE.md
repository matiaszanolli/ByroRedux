### COORD-02: `docs/engine/coordinate-system.md`'s XCLL call-path claim is stale

- **Severity**: LOW
- **Dimension**: Legacy compatibility — Coordinate-system correctness (documentation)
- **Location**: `docs/engine/coordinate-system.md:192-194` vs. `byroredux/src/cell_loader/load.rs:218-233` and `byroredux/src/cell_loader/euler.rs:4-6`
- **Status**: NEW
- **Description**: The dimension's own reference doc states: "Non-REFR callers (XCLL directional lighting in `scene.rs`, `#380`) call the canonical `euler_zup_to_quat_yup` directly, bypassing the dispatcher." This is no longer true on two counts: (1) XCLL lighting no longer lives in `scene.rs` — it moved to `cell_loader/load.rs`; (2) it no longer calls `euler_zup_to_quat_yup` at all. Fix `#3313`/`#3314`/`#3315`/`#3316` (`b78749aff`, 2026-08-26) replaced the `#380` quaternion-based routing with a dedicated `xcll_direction_yup(azimuth, elevation)` spherical-to-vector helper, specifically because routing through the shared REFR helper discarded azimuth (an X-axis-only quaternion rotation cannot move the model vector's azimuth component). `cell_loader/euler.rs`'s own module doc already states the opposite of the shared doc: "XCLL directional lighting deliberately does not use these helpers."
- **Evidence**: `xcll_direction_yup`'s trigonometric derivation (`cos_elev·cos_az, -sin_elev, -cos_elev·sin_az`) is a correct direct construction of the `(x,z,-y)`-swapped direction vector — the current code is correct, only the doc is wrong. `docs/engine/coordinate-system.md` was last touched `a5a360f96` (2026-08-26, ~3h before `b78749aff` landed the same day) and was never reconciled afterward.
- **Impact**: Low direct impact (the code itself is correct), but this is the dimension's cited authoritative reference; a future contributor "fixing" `xcll_direction_yup` to route through `euler_zup_to_quat_yup` on the doc's authority would reintroduce the discarded-azimuth bug `#3313` fixed.
- **Related**: Fix `#3313`/`#3314`/`#3315`/`#3316`. No open issue tracks the doc drift itself.
- **Suggested Fix**: Update the "Impact on Euler angles" section of `coordinate-system.md` to point at `cell_loader::load::xcll_direction_yup` and describe the azimuth/elevation-to-direction derivation instead of claiming XCLL uses `euler_zup_to_quat_yup`.

## Completeness Checks
- [ ] **SIBLING**: Check whether `docs/engine/per-game-translation-survey.md`'s SCOL era-gate description (flagged in the same report, not filed as its own issue) needs the same kind of doc-drift pass
