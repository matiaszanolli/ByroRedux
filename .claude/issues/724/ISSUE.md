# NIF-LOW-BUNDLE-02: Version-handler dead-code + cosmetic cleanup

URL: https://github.com/matiaszanolli/ByroRedux/issues/724
Labels: enhancement, nif-parser, low

---

## Severity: LOW (bundled)

## Bundled findings
4 low-impact cosmetic / dead-code findings from AUDIT_NIF_2026-04-26.md. None bite shipped content; filing as a tracking issue for cleanup.

### NIF-D2-01: Header endian-byte gate uses 20.0.0.4 instead of nif.xml's 20.0.0.3
- **Location**: `crates/nif/src/header.rs:68`
- **Fix**: Change `if version >= NifVersion(0x14000004)` → `NifVersion(0x14000003)` per nif.xml line 1968 `since="20.0.0.3"`.
- **Game**: Theoretical — no Bethesda title shipped 20.0.0.3.

### NIF-D2-02: `has_shader_emissive_color()` includes Fallout3 against `#BS_GT_FO3#`
- **Location**: `crates/nif/src/version.rs:182-187`
- **Fix**: Either drop `Self::Fallout3` from the matches arm, or remove the dead helper entirely (zero callers — preferred per `feedback_audit_findings.md`). nif.xml line 6250 gates emissive color on `vercond="#BS_GT_FO3#"` (BSVER **>** 34, strict).

### NIF-D2-06: Variant `bsver()` hardcoded values are foot-gunny
- **Location**: `crates/nif/src/version.rs:97, 123`
- **Description**: `(11, uv2 < 34) => Fallout3` then `Fallout3 => 21` for `bsver()`. A FO3 NIF with in-file BSVER = 11 (early-FO3 dev) reports `variant().bsver() == 21`. Currently zero callers (verified via grep), but a foot-gun for future audit fixes.
- **Fix**: Either rename to `canonical_bsver()` to discourage misuse, OR change to `pub fn bsver(self) -> Option<u32>` returning `None` so callers are forced to use `stream.bsver()`.

### NIF-D1-06: BSLightingShaderProperty `Has Texture Arrays` reads as raw byte (cosmetic)
- **Location**: `crates/nif/src/blocks/shader.rs:782`
- **Description**: nif.xml says `Has Texture Arrays: byte`, code uses `read_u8()? != 0` — correct, but inconsistent with sibling `read_byte_bool()` reads in the same function. No functional impact.
- **Fix**: Either use `read_u8()` directly and store as `u8`, or stay with `read_byte_bool()` for consistency.

## Impact
None today. Defense-in-depth against future audit passes; cleanup of dead code.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D2-01, NIF-D2-02, NIF-D2-06, NIF-D1-06)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Sweep `version.rs` for other hardcoded `bsver()` returns; check other dead helpers
- [ ] **TESTS**: Update existing version regression tests if signatures change
