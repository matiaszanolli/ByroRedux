# NIF-D2-06: Max Filepath BSStreamHeader gate uses raw magic literal 103 instead of a named bsver::* constant

**Issue**: #1898 · **Severity**: LOW · **Labels**: low, nif-parser, nif, bug
**Dimension**: Version Gating · **Filed from**: docs/audits/AUDIT_NIF_2026-07-06.md (nif-deep suite)
**Location**: crates/nif/src/header.rs:158 (`if user_version_2 >= 103 { … max_filepath … }`)

## Description
Max Filepath field's nif.xml threshold #BS_GTE_103# is hardcoded at header.rs:158 as a bare 103,
while the two sibling gates above already use bsver::FALLOUT4. Value correct; naming violates the
crate's "named constants not bare literals" convention.

## Suggested Fix
Add `pub const MAX_FILEPATH: u32 = 103;` to version::bsver and reference it here.

**Related**: #1842 (separate doc-token issue on the same header path).
