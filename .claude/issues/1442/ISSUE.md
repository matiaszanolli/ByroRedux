# #1442 — LC-D5-03: KF-sequence dispatch matches only NiTransformController not aliased NiKeyframeController

_Snapshot as filed (2026-06-02) from AUDIT_LEGACY_COMPAT_2026-06-02.md. GitHub is authoritative for live state._

- **Severity**: LOW
- **Dimension**: D5 (Animation Readiness)
- **Location**: `crates/nif/src/anim/sequence.rs:40`
- **Status**: NEW

## Description
`import_sequence` dispatches controlled blocks on the resolved controller-type *string*. The transform arm matches only `"NiTransformController"` (`:40`), while the block **parser** deliberately aliases both `"NiTransformController" | "NiKeyframeController"` (`crates/nif/src/blocks/mod.rs:699-700`, comment: "NiKeyframeController is the pre-Skyrim per-bone animation driver"). The import dispatch should carry the same alias for consistency; otherwise a controlled block whose type string resolves to the classic name falls to the `_ =>` drop (`sequence.rs:123`).

## Impact
**Premise caveat**: this only bites if real target-era KF controlled blocks carry the `"NiKeyframeController"` type string rather than the `"NiTransformController"` Bethesda exporters typically write. That was **not confirmed against sample FNV/Oblivion KF data** in this sweep, so the finding is defense-in-depth / parity, not a confirmed content regression.

## Suggested Fix
One-line alias — `"NiTransformController" | "NiKeyframeController" =>` at `sequence.rs:40`. (Confirm against a sample FNV/Oblivion .kf first.)

## Related
LC-D5-01 (#TBD).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (KF-sequence path vs embedded path; other channel converters)
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→Material/clip boundary, never pushed to shaders/renderer or re-derived at render time. See /audit-nifal.
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from [docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md](../blob/main/docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md)_
