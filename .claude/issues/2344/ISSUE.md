# NIF-OBL-D1-01: NiBlendInterpolator drops Single Interpolator + Single Time at v10.1.0.108-109

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2344
**Severity**: MEDIUM
**Dimension**: NIF Version Handling (v20.0.0.5 + v10.x NetImmerse Tail)
**Location**: `crates/nif/src/blocks/interpolator.rs:877-1013` (`NiBlendInterpolator::parse` / `parse_legacy`)
**Source audit**: `docs/audits/AUDIT_OBLIVION_2026-08-03.md` (finding NIF-OBL-D1-01)
**Labels**: medium, nif-parser, legacy-compat, bug

### Description
`NiBlendInterpolator::parse` (line 877) routes `version <= V10_1_0_109` to
`parse_legacy(stream, /* int_priority */ true)` and `V10_1_0_110..=V10_1_0_111`
to `parse_legacy(stream, /* int_priority */ false)`. Inside `parse_legacy`
(line 946), only the `int_priority == false` branch (lines 994-1001) reads the
`Single Interpolator` (Ref, 4B) + `Single Time` (f32, 4B) pair. nif.xml gates
both fields on `since="10.1.0.108" until="10.1.0.111"` — an 8-byte window that
overlaps the *first* branch (`int_priority == true`, taken for `<= 10.1.0.109`)
at exactly v10.1.0.108 and v10.1.0.109.

### Impact
An 8-byte under-read per `NiBlendInterpolator` on any file at exactly
v10.1.0.108 or v10.1.0.109 — cascades through the sizeless format (same shape
as the #1301/#1310/#1337/#1508 truncation family). Blast radius on vanilla
Oblivion content is zero (confirmed via a fresh version census — no file in
that band exists in `Oblivion - Meshes.bsa`). Exposure is limited to
third-party Gamebryo/Oblivion mod content.

### Suggested Fix
Gate the `Single Interpolator`/`Single Time` pair on
`version >= V10_1_0_108 && version <= V10_1_0_111` independently of the
`int_priority` branch selector; add a `V10_1_0_108` constant to `version.rs`;
add a byte-exact regression test at v10.1.0.108.

### Related
`#1508` (the original three-band `NiBlendInterpolator` + `ControlledBlock` fix).
