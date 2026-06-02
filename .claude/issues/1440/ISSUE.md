# #1440 — LC-D5-01: Inline transform controllers dropped from embedded-animation path

_Snapshot as filed (2026-06-02) from AUDIT_LEGACY_COMPAT_2026-06-02.md. GitHub is authoritative for live state._

- **Severity**: MEDIUM
- **Dimension**: D5 (Animation Readiness)
- **Location**: `crates/nif/src/anim/entry.rs:290-498` (drop arm `:493`)
- **Status**: NEW

## Description
`import_embedded_animations` dispatches inline (non-KF) controllers attached directly to a `NiNode`. It has arms for Alpha / Vis / TextureTransform / MaterialColor / shader float+color / Flip / Light color+dimmer+intensity+radius / UV — but **no arm for `NiTransformController` / `NiKeyframeController`**. Both are parsed as typed blocks (`crates/nif/src/blocks/mod.rs:699-700`) and `extract_transform_channel` exists, but neither is wired into the embedded walker, so a transform controller on a node falls through to the `other =>` debug-log drop (`entry.rs:493`). Era-independent.

## Impact
Ambient transform animation baked inline into a loose `.nif` (Oblivion/FO3/FNV animated scenery: fans, doors, lifts, swinging signs driven by an inline `NiKeyframeController` with no `NiControllerManager`) renders **static**. The static mesh still draws — only its motion is lost. Loose `.kf` clips are unaffected.

## Suggested Fix
Add a `"NiTransformController" | "NiKeyframeController"` arm to the embedded dispatch that resolves the `NiSingleInterpController` interpolator and feeds TRS keys into `clip.channels`.

## Related
LC-D5-03 (#TBD, same root issue on the KF-sequence path).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (KF-sequence path vs embedded path; other channel converters)
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→Material/clip boundary, never pushed to shaders/renderer or re-derived at render time. See /audit-nifal.
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from [docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md](../blob/main/docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md)_
