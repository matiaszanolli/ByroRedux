# NIF-D2-NEW-02/03: bsver_values test gap + v20.0.0.4 unconditionally routes to Oblivion

**Severity**: LOW (bundled — both small `version.rs` hygiene fixes)
**Source audit**: `docs/audits/AUDIT_NIF_2026-05-10.md` (Dim 2)

## NIF-D2-NEW-02 — `bsver_values` test misses Starfield + Unknown asserts

**Location**: `crates/nif/src/version.rs:481-488`

The test asserts FO3..FO76 but never checks `Starfield.bsver() == 172` (audit hard-pin) nor `Unknown.bsver() == 0`. A typo regression would slip past CI.

**Fix**: Two extra `assert_eq!` lines.

## NIF-D2-NEW-03 — v20.0.0.4 unconditionally routes to Oblivion, ignoring early FO3 dev

**Location**: `crates/nif/src/version.rs:91-94`

```rust
if version == NifVersion::V20_0_0_4 || version == NifVersion::V20_0_0_5 {
    return Self::Oblivion;
}
```

nif.xml line 196: `<version id="V20_0_0_4__11" num="20.0.0.4" user="11" bsver="11">Oblivion, Fallout 3</version>`. The `#FO3#` verset (line 44) explicitly includes `V20_0_0_4__11`.

No retail FO3 NIF ships at v20.0.0.4 — impact is pre-release content only.

**Fix**: When `version == V20_0_0_4 && user_version == 11`, prefer `Fallout3`. For v20.0.0.5 the current rule is correct (`V20_0_0_5_OBL` is Oblivion-only per nif.xml line 197).

## Completeness Checks

- [ ] **TESTS**: Both fixes are test-pinnable; verify `bsver_values` covers Starfield + Unknown after fix
- [ ] **DOCS**: Inline comment on `detect()` clarifies the v20.0.0.4 + user_version=11 → Fallout3 split
