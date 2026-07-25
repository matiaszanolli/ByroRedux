**Severity**: LOW · **Dimension**: 1 — discovered while verifying the ABBA-detector CI guard (#1410)
**Source**: `docs/audits/AUDIT_ECS_2026-07-25.md` (ECS-2507-04)
**Status**: NEW

## Description
The CI table in `docs/contributing.md` states "Three jobs run on every push /
PR" and lists `cargo-test`, `lock-order-check`, `vulkan-validation`. `ci.yml`
currently defines five jobs: `shader-artifacts` (line 23, added in `ca7a4e0e`),
`cargo-test` (45), `lock-order-check` (75), `nif-heap-allocation-bounds`
(105) and `vulkan-validation` (131). The closing sentence "CI passes if: all
unit tests pass, no clippy warnings, no ABBA cycles detected, and no Vulkan
`ERROR`-severity validation messages fire" also omits the shader-artifact
parity check and the NIF heap-allocation bounds.

## Evidence
`grep -n "^  [a-z-]*:$" .github/workflows/ci.yml` →
`shader-artifacts`, `cargo-test`, `lock-order-check`,
`nif-heap-allocation-bounds`, `vulkan-validation`.

## Impact
Documentation only. A contributor reading `contributing.md` will not know
that a shader-SPIR-V drift or a NIF allocation-budget regression is a hard CI
failure. The ECS-relevant guard (`lock-order-check`) is correctly documented
and correctly blocking.

## Suggested Fix
Update the table to five rows and extend the pass criteria sentence.
One-line doc edit.

## Related
#1410 (ABBA detector in CI — verified closed and blocking).

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix, no test to pin
