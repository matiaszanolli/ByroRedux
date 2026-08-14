# PHYS-D5-04

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2885

---

Found by `/audit-physics` Dimension 5 (Character Controller). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `crates/physics/src/config.rs` (`ContactConfig::DEFAULT`, `default_contact_config_matches_previous_inline_values`), `byroredux/src/scene.rs:137-156`

## Trigger Conditions
Anyone re-tuning `ContactConfig` — which the module doc explicitly invites (*"Bumping the character-controller offset for a wider-clearance test becomes a single field write"*) — raising `default_contact_skin_bu` to or above `kcc_offset_bu`.

## Description
The defaults are consistent today (`kcc_offset_bu = 4.0` > `default_contact_skin_bu = 1.0`, so the 2 BU of combined skin between the player capsule and a floor collider fits inside the 4 BU KCC offset), but **nothing enforces the relation**. The existing test asserts only `kcc_offset_bu == 4.0` and `default_contact_skin_bu >= 0.0`.

`capsule_center_y_on_surface` (`scene.rs:137-144`) computes the spawn height from `kcc_offset_bu` alone and never accounts for the collider skin, so an inverted pair spawns the capsule inside the skin-inflated floor — the #2193 "blocked but permanently ungrounded" configuration.

## Impact
Latent. No live defect; a defence-in-depth gap on a struct whose doc comment advertises single-field re-tuning.

## Suggested Fix
Add an assertion to `default_contact_config_matches_previous_inline_values` (or a dedicated test) that `kcc_offset_bu > 2.0 * default_contact_skin_bu`, and state the invariant in the `kcc_offset_bu` doc comment.

## Related
- #2193 (CLOSED)
