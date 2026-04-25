# FNV-D1: bhkBreakableConstraint trailer fields zeroed on FO3+ + wrapped CInfo size table not version-aware

## Finding: FNV-D1 (bundle of FNV-D1-01 + FNV-D1-02)

- **Severity**: MEDIUM (D1-01) + LOW dormant (D1-02)
- **Source**: `docs/audits/AUDIT_FNV_2026-04-24.md`

## FNV-D1-01: bhkBreakableConstraint silently zeroes threshold + remove_when_broken on FO3+ (MEDIUM)

**Location**: [crates/nif/src/blocks/collision.rs:1721-1733](crates/nif/src/blocks/collision.rs#L1721-L1733)

`BhkBreakableConstraint::parse` only reads `threshold` and `remove_when_broken` on the Oblivion path. On FO3+ (every FNV instance) it returns `threshold: 0.0, remove_when_broken: false` unconditionally. Per nif.xml line 7027, both fields are unconditional and trail the wrapped CInfo on every Bethesda version.

The Oblivion `wrapped_payload_size` table at lines 1678-1692 is correct for FNV in the common case (motor type 0). `block_size` recovery keeps the stream aligned (parse-rate gate stays green) but two semantically meaningful fields are silently lost on every FNV/FO3 instance.

Same pattern applies to `BhkConstraint::parse` at `collision.rs:1346-1431` which short-stubs on FO3+ and loses the entire CInfo payload despite the FNV layout being fully derivable.

**Fix**: drop the `is_oblivion` gate; use `wrapped_payload_size` on FO3+ for all wrapped types where the payload size is derivable, then read the trailer fields directly. Keep the recovery fallback for Malleable type and motor type ≠ 0.

## FNV-D1-02: Wrapped CInfo size table is Oblivion-only but presented as version-agnostic (LOW, dormant)

**Location**: [collision.rs:1681](crates/nif/src/blocks/collision.rs#L1681) (Hinge=80) and [collision.rs:1358](crates/nif/src/blocks/collision.rs#L1358) (parallel table, Hinge=80).

nif.xml has version-conditional fields the table ignores:

| Constraint | Oblivion (`until 20.0.0.5`) | FNV (`since 20.2.0.7`) | Table value | Drift |
|---|---|---|---|---|
| `bhkHingeConstraintCInfo` | 5 × Vec4 = 80 B | 8 × Vec4 = 128 B | 80 | -48 |
| `bhkLimitedHingeConstraintCInfo` | 7 × Vec4 + 3 × f32 = 124 B | 8 × Vec4 + 3 × f32 = 140 B | 124 | -16 |
| `bhkRagdollConstraintCInfo` | 6 × Vec4 + 24 trailing floats = 120 B | 8 × Vec4 + Motor A + Motor B = 152 B | 120 | -32 |

Currently dormant — the table is only consulted on the Oblivion branch — but **becomes a real bug the moment FNV-D1-01 lifts the `is_oblivion` gate**. Both findings should land together.

**Fix**: make `wrapped_payload_size` and the parallel table version-aware (key off `stream.version()`). Sizes derive mechanically from nif.xml.

## Related

- #557 (closed): NIF-12 Havok tail types — added the dispatch.
- #546 (closed): bhkRigidBody CInfo2010 body on Skyrim LE/SE.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: After D1-01 lands, verify no other constraint type relies on the Oblivion-only short-stub path.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic FNV NIF with bhkBreakableConstraint with non-default threshold + remove_when_broken; assert post-parse fields match. Roundtrip test for each constraint kind in the wrapped-payload table on both Oblivion and FNV versions.

_Filed from audit `docs/audits/AUDIT_FNV_2026-04-24.md`._
