# NIF-D2-NEW-01: NifVariant::Fallout3.bsver() returns 21, not 34 — contradicts hard-pin

**Severity**: MEDIUM
**Source audit**: `docs/audits/AUDIT_NIF_2026-05-10.md` (Dim 2)

## Location

`crates/nif/src/version.rs:125` — `Self::Fallout3 => 21`

## Why it's a bug

Audit hard-pin: `bsver()` should return `FO3=34, FNV=34, SK=83, SK_SE=100, FO4=130, FO76=155, SF=172`.

nif.xml line 208: `<version id="V20_2_0_7_FO3" num="20.2.0.7" user="11" bsver="34" ext="rdt">Fallout 3, Fallout NV</version>`

`Fallout3` variant only ever applies to FO3 mod/dev builds (retail FO3 NIFs at bsver=34 detect as `FalloutNV` since the two are binary-identical at that BSVER). Whatever single value `Fallout3.bsver()` returns mis-represents most of the FO3 dev-bsver fan-out (14, 16, 21, 24-28, 30-33, 34).

## Impact

Today nothing outside the unit test queries `NifVariant::bsver()` (every parse site uses `stream.bsver()` directly), so behavior is uncorrupted. But the value is misleading and traps future contributors who try the variant-based path.

## Fix

Either:
- (a) Return 34 to match the hard-pin, document `Fallout3` variant = "dev/mod FO3" with retail bsver matching FNV.
- (b) Rename helper to `canonical_dev_bsver()` and mark non-authoritative.

Option (a) matches the audit spec.

## Completeness Checks

- [ ] **SIBLING**: Verify all 9 variant arms; check `Unknown.bsver()` returns 0
- [ ] **TESTS**: Update `bsver_values` test (also see NIF-D2-NEW-02 — Starfield + Unknown asserts)
