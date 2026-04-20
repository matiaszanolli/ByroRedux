# Issue #485

FNV-AN-L1: KeyType::Quadratic rotation falls back to SLERP (RotationKey stores no forward/backward quats)

---

## Severity: Low

**Location**: `crates/core/src/animation/interpolation.rs:239`

## Problem

Comment: `"RotationKey doesn't store forward/backward quats today, so fall back to SLERP."`

nif.xml's `QuaternionKey` carries control quats for Quadratic interpolation (forward and backward control rotations). The current `RotationKey` struct in `crates/core/src/animation/types.rs` stores only the value quat.

## Impact

Small — Bethesda KFs almost exclusively use `Linear` + `Tbc` for rotation channels. No known vanilla FNV clip uses Quadratic rotation keys.

Translation + scale Quadratic already use their control keys correctly.

## Fix

1. Extend `RotationKey` with optional `forward_control: Option<Quat>` + `backward_control: Option<Quat>`.
2. Update `crates/nif/src/anim.rs` NiQuaternionKey parser to populate them when `interpolation_type == Quadratic`.
3. Implement Bezier or squad interpolation in `sample_rotation` using the control quats.

## Completeness Checks

- [ ] **TESTS**: Synthetic clip with Quadratic rotation keys + control quats, assert curved interpolation vs SLERP baseline
- [ ] **SIBLING**: Confirm translation/scale Quadratic paths are actually using control keys (spot-check against existing tests)
- [ ] **NIF**: nif.xml reference in the parser comment

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-AN-L1)
