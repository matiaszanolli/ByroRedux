# PHYS-D5-05

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2886

---

Found by `/audit-physics` Dimension 5 (Character Controller). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `byroredux/src/systems/character.rs:691-692`, `:711-712`, `:724-726`; `crates/physics/src/components.rs:195-198`

## Trigger Conditions
Any future re-tune of `CharacterController::HUMAN`.

## Description
The three `integrate_vertical` tests use `let g = -1373.4; // CharacterController::HUMAN.gravity` and `let jv = 380.0;`. **The live preset is `gravity: -1220.8` and `jump_velocity: 506.6667`** (`crates/physics/src/components.rs:134-135`) — the values were retuned (2x jump height / 1.5x hang time) and the tests were not.

The comment asserts a link to the preset that **does not exist in code**, so a future preset change cannot break these tests. (`terminal_velocity: -2000.0` is still current.)

Related nit in the same family: `character_controller_human_dimensions` (`crates/physics/src/components.rs:195-198`) asserts `terminal_velocity < gravity` with the message *"terminal velocity must be more negative than 1-frame gravity"* — it compares a BU/s **velocity** against a BU/s^2 **acceleration**. It happens to hold, but it does not test what it says.

## Evidence
```
crates/physics/src/components.rs:134:        jump_velocity: 506.6667,
crates/physics/src/components.rs:135:        gravity: -1220.8,
byroredux/src/systems/character.rs:691:        let g = -1373.4; // CharacterController::HUMAN.gravity
byroredux/src/systems/character.rs:725:        let jv = 380.0;
```

## Impact
Documentation/coverage rot only; the pure-function behaviour under test is correct.

## Suggested Fix
Reference `CharacterController::HUMAN.gravity` / `.jump_velocity` directly instead of literals, and either drop or restate the unit-confused assertion as `terminal_velocity < gravity * MAX_DT`.
