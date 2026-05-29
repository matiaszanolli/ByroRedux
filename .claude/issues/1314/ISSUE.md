# #1314 -- OB-D7-001: Stale doc reference resolve_classifier_overrides

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: LOW | **Dim 7** — NIFAL Canonical Material Translation
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OB-D7-001)

**Location**: `byroredux/src/material_translate.rs:59`; `docs/engine/material-abstraction.md:143, 147`

**Issue**: The `material_translate.rs` module doc at line 59 and two locations in `material-abstraction.md` reference `Material::resolve_classifier_overrides` — a method that no longer exists (it was renamed to `Material::resolve_pbr` during the NIFAL canonical-material-translation refactor). Broken doc/intra-link; future maintainers reading the NIFAL translate boundary encounter a dead reference to the core resolution step.

**Suggested fix**: replace `Material::resolve_classifier_overrides` with `Material::resolve_pbr` in `material_translate.rs:59` and `docs/engine/material-abstraction.md:143,147` (all three occurrences).

## Completeness Checks
- [ ] **SIBLING**: grep the whole repo for `resolve_classifier_overrides` to catch any remaining stale references
- [ ] **TESTS**: no behavior change; doc-only
- [ ] **CANONICAL-BOUNDARY**: this IS the NIFAL boundary doc — confirm the corrected symbol exists (`grep -n 'fn resolve_pbr' crates/core/src/ecs/components/material.rs`)
- [ ] **UNSAFE**: no unsafe involved
