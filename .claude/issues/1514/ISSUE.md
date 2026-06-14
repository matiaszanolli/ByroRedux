**From**: NIFAL audit `docs/audits/AUDIT_NIFAL_2026-06-13.md` (Dimension 1 — Material)
**Severity**: LOW (doc-rot) · **Tier Violated**: none (documentation only)
**Game Affected**: none
**Location**: `docs/engine/material-abstraction.md:143,147` (also the `roughness_override = 0.10` framing at `:133,:150`)

## Description
The function was renamed `resolve_classifier_overrides` → `resolve_pbr`. The code reference in `material_translate.rs` is already correct, but `material-abstraction.md` step 2 ("`resolve_classifier_overrides` collapses the `Option`s…") and step 3 ("…right after `resolve_classifier_overrides`") still cite the dead symbol. The same region also uses the pre-canonical `roughness_override = 0.10` framing — the canonical field is now `Material.roughness`, forced by `classify_glass_into_material`.

The code-side rename was closed as #1309 (OB-D7-001), but #1309's body was repurposed to an unrelated wireframe-pipeline topic and these two doc lines were never actually corrected; no open issue tracks them. Prior audits flagged it as D1-01 / OB-D7-001 — re-confirmed STILL present in the live tree this sweep.

## Evidence
```
$ grep -rn resolve_classifier_overrides crates byroredux --include='*.rs'
ABSENT from .rs (doc-rot confirmed)
$ grep -n 'fn resolve_pbr' crates/core/src/ecs/components/material.rs
638:    pub fn resolve_pbr(&mut self) {
$ grep -n resolve_classifier_overrides docs/engine/material-abstraction.md
143: ... resolve_classifier_overrides collapses the Options ...
147: ... right after resolve_classifier_overrides). Rule:
```

## Impact
A reader following `material-abstraction.md` greps for a symbol that does not exist and may believe the Option-collapse step is unimplemented. Pure doc-rot; zero runtime effect.

## Suggested Fix
`s/resolve_classifier_overrides/resolve_pbr/` at lines 143, 147; update the `roughness_override = 0.10` framing at 133/150 to `Material.roughness` (forced glass-smooth). Note in the commit that #1309 closed only the code-side reference, not these doc lines.

## Completeness Checks
- [ ] **SIBLING**: Sweep `material-abstraction.md` (and `nifal.md`) for any other pre-canonical `*_override` Option framing that survived the #1346 plain-f32 migration.
- [ ] **TESTS**: n/a (doc-only) — verify the cited symbol/field names match the live `material.rs` after edit.
