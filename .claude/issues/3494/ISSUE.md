# Issue #3494: PHYS-D6-2026-08-27b-03: water.rs carries a duplicated #[test] attribute and a mis-attributed rationale doc

Labels: low, physics, test-gap, bug
Filed: 2026-08-27 (published 2026-08-28)

---

**Source**: `docs/audits/AUDIT_PHYSICS_2026-08-27b.md` — PHYS-D6-2026-08-27b-03
**Severity**: LOW · **Dimension**: 6 — Water / Buoyancy (test hygiene)

## Location
`crates/physics/src/water.rs:1889-1912`

## Trigger Conditions
None at runtime — a build-time lint plus a documentation mis-attribution.

## Description
`bbfd742f` inserted the new `#3268` regression test *between* the `#3114` test's doc comment and its `#[test]` attribute. The `#3114` paragraph now ends at `water.rs:1897`; the `#[test]` at `:1898` binds to the *new* function; the `#3268` doc runs `:1899-1910`; a second `#[test]` sits at `:1911`; and `fn current_volume_without_a_water_plane_wakes_a_body_resting_in_it` starts at `:1912`. Net effect: the new test has two `#[test]` attributes, it is documented by the *other* test's rationale, and `current_volume_without_a_water_plane_does_not_wind_up_user_force` — the regression guard for a HIGH-severity "havok explosion" force wind-up — is left with no rationale doc at all.

## Evidence
```
$ cargo check -p byroredux-physics --tests
warning: duplicated attribute
    --> crates/physics/src/water.rs:1911:5
     |
1911 |     #[test]
     |     ^^^^^^^
     |
     = note: `#[warn(duplicate_macro_attributes)]` on by default
warning: `byroredux-physics` (lib test) generated 1 warning
```

Re-run at publish time on HEAD: the warning still reproduces verbatim at `water.rs:1911:5`.

## Impact
Both tests still run and pass (153/153). The cost is a permanent warning on every `cargo check --tests` — exactly the noise floor that hides the *next* warning — plus a rationale doc that explains the wrong test, on a pair of tests whose whole value is encoding why the current-volume branch is shaped the way it is.

## Related
`#3114`, `#3268` (both CLOSED); the "clear the advisory list rather than learning to scroll past it" posture in `_audit-common.md` § Path-Reference Convention.

## Suggested Fix
Delete the `#[test]` at `:1898` and move the whole `#3268` test (doc + attribute + body) below `current_volume_without_a_water_plane_does_not_wind_up_user_force`, restoring the `#3114` doc to its own function. The vanished warning is the acceptance criterion.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
