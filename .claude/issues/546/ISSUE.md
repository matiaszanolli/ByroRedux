# NIF-01: bhkRigidBody parse fails on 12,866 Skyrim SE blocks (58% of SE NiUnknown pool)

**Severity**: CRITICAL
**Dimension**: Block Parsing × Coverage Gaps
**Game Affected**: Skyrim SE (bsver 100)
**Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-01

## Summary

`blocks/mod.rs:629` dispatches both `bhkRigidBody` and `bhkRigidBodyT` through `BhkRigidBody::parse`. On Skyrim SE (bsver 100) the parser returns `Err`; the outer loop logs a warning, seeks past the block via `block_size`, and stores a `NiUnknown`. Every Havok rigid body in `Skyrim - Meshes0.bsa` is lost — mass, inertia, collision filter, constraint refs all gone. Physics, ragdoll, and havok-driven animation on Skyrim scenes all downstream-degrade.

## Evidence

`/tmp/audit/nif/skyrimse_unk.out` — **9,772 `bhkRigidBody` + 3,094 `bhkRigidBodyT`** in the NiUnknown bucket on Skyrim SE. FNV/FO3 parse these cleanly (not in their unknown histograms); Oblivion has 6 (tracked as NIF-04).

## Location

- `crates/renderer/../blocks/collision.rs:210-` (`BhkRigidBody::parse`)
- Recovery at `crates/nif/src/lib.rs:293-316`

## Suggested fix

Bisect `BhkRigidBody::parse` with a SE-only test fixture. The three `bsver <= 34` / `bsver >= 83` / `bsver < 130` gates at lines 223 / 243 / 305 all hit the Skyrim path. Per nif.xml `bhkRigidBodyCInfo`, the cInfo size on SE is 144 bytes (bsver 100) vs 152 on FO4 (bsver 130) — likely a gated field that Skyrim has but the current `bsver >= 83` arm over- or under-consumes.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check `bhkRigidBodyT` renders the same test-case result once the base is fixed
- [ ] **TESTS**: Synthetic SE-bsver fixture at bsver=100 pinning 144-byte cInfo round-trip
- [ ] **REAL-DATA**: `cargo run --release --quiet -p byroredux-nif --example unknown_types -- <SE meshes>` must drop the `bhkRigidBody` bucket to ≤10

Fix with: /fix-issue <number>
