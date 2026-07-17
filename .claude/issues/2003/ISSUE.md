# 2003: NIF-D1-04: NiShadeProperty.Flags read unconditionally, but nif.xml gates it to bsver <= FO3

https://github.com/matiaszanolli/ByroRedux/issues/2003

Labels: medium, nif-parser, nif, bug

**Severity**: MEDIUM · **Dimension**: Stream Position Integrity
**Location**: `crates/nif/src/blocks/properties.rs:547-556` (`NiFlagProperty::parse`), dispatch at `crates/nif/src/blocks/mod.rs:592-601`
**Status**: NEW
**Audit**: docs/audits/AUDIT_NIF_2026-07-16.md (NIF-D1-04)

## Description
nif.xml gates `NiShadeProperty.Flags` with `vercond="#NI_BS_LTE_FO3#"`; the other three types sharing this parser (`NiDitherProperty`/`NiSpecularProperty`/`NiWireframeProperty`) have no such gate. `NiFlagProperty::parse` treats all four identically, reading `flags` unconditionally.

## Evidence
```rust
impl NiFlagProperty {
    pub fn parse(stream: &mut NifStream, type_name: &'static str) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;
        let flags = stream.read_u16_le()?;   // unconditional for all 4 aliased types, including NiShadeProperty
```

## Impact
On a bsver > FO3 file with a literal `NiShadeProperty`, the parser reads 2 bytes belonging to the next field as `flags`. No confirmed corpus regression today (`NiShadeProperty` usage is scoped to a handful of Oblivion architectural pieces) — a latent gap, not an observed failure.

## Related
None found.

## Suggested Fix
Split `NiShadeProperty` into its own thin parser gated on `bsver <= FO3`, or branch inside `NiFlagProperty::parse` on `type_name == "NiShadeProperty" && bsver > FO3` to skip the read and default `flags`.

## Completeness Checks
- [ ] SIBLING: Same pattern checked in related files (other multi-type-aliased parsers in `properties.rs`)
- [ ] TESTS: A regression test pins this specific fix (bsver > FO3 `NiShadeProperty` fixture)
