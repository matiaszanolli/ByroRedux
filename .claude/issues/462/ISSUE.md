# Issue #462

FO3-5-03: Havok constraint stubs spew 45 warn! lines per skeleton NIF load

---

## Severity: Low (log hygiene)

**Location**: `crates/nif/src/blocks/collision.rs:1186-1336`, `crates/nif/src/lib.rs:204-211`

## Problem

Havok constraint stubs (per #117) read a 16-B base and rely on the outer `block_sizes` loop to skip the remaining payload. Intentional design, but every recovery emits a `warn!` line at `lib.rs:204-211`.

## Evidence

- `skeleton_male.nif`: 16× `bhkMalleableConstraint` + 3× `bhkRagdollConstraint`, all `expected 181/169/193 consumed 16`.
- `deathclaw_skeleton.nif`: 29 more constraint warnings.

One full FO3 cell load with actor spawns spews hundreds of constraint warn lines, drowning real parser drift signals.

## Fix

In `lib.rs:204-211`, downgrade to `trace!` when the block type name is a known-stub. Add:
```rust
fn is_havok_constraint_stub(type_name: &str) -> bool {
    matches!(type_name,
        "bhkMalleableConstraint"
        | "bhkRagdollConstraint"
        | "bhkLimitedHingeConstraint"
        | "bhkHingeConstraint"
        | "bhkBallAndSocketConstraint"
        | "bhkPrismaticConstraint"
        | "bhkStiffSpringConstraint"
        | "bhkGenericConstraint"
    )
}
```

Real parser drift stays visible. Orthogonally, finish the constraint payload parsers so the base+trailer adds up to the full `CInfo` size (separate track).

## Completeness Checks

- [ ] **TESTS**: Skeleton probe NIF parse emits zero `warn!` lines, still has `trace!` coverage
- [ ] **SIBLING**: Audit other intentionally-stubbed block types for the same log noise pattern
- [ ] **LINK**: Cross-reference #117 in the fix commit

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-5-03)
