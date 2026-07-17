# SAVE-D2-04: LightSource / LightFlicker have no dedicated save/load round-trip test

**Labels**: low, ecs, bug

**Severity**: LOW
**Dimension**: Registry & (De)serialization Fidelity
**Source**: `docs/audits/AUDIT_SAVE_2026-07-16.md`

## Location
`crates/core/src/ecs/components/light.rs`; registered at `byroredux/src/save_io.rs:181-182`

## Description
Both `LightSource` and `LightFlicker` are registered and delta-columned but no test round-trips either type (unlike most of the rest of the registry). Both are flat structs (`f32`/`u32`/`[f32;3]`, no nesting/`Option`/`FixedString`/`EntityId`), so low-risk.

Verified current: no test in `crates/save/tests/*.rs` or `byroredux/src/save_io.rs`'s test module references `LightSource` or `LightFlicker`.

## Impact
A serde regression would currently surface only as a runtime visual bug, not a test failure.

## Suggested Fix
Add one assertion-bearing round trip, can piggyback on the existing `binary_registry_round_trips_including_scripttimer` test.

## Completeness Checks
- [ ] **SIBLING**: Same gap likely applies to other under-tested flat registered components — worth a sweep while adding this test
- [ ] **TESTS**: A regression test pins this specific fix
