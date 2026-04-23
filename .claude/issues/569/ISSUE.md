# #569 — SK-D2-01: No committed full-archive sweep test for Skyrim SE BSAs

**Severity:** LOW (test coverage gap)
**Labels:** low, documentation
**Source:** AUDIT_SKYRIM_2026-04-22.md
**GitHub:** https://github.com/matiaszanolli/ByroRedux/issues/569

## Location
- `crates/bsa/src/archive.rs:501-842` (test module)

## One-line
Test module has `FNV_MESHES_BSA` fixture but nothing equivalent for Skyrim SE. v105 LZ4 frame path is functionally correct but has no regression gate.

## Fix sketch
Add `#[ignore]`d sister tests following the FNV `skip_if_missing()` pattern for `Skyrim - Meshes0.bsa` (file_count + sweetroll roundtrip) and `Skyrim - Textures0.bsa` (DDS magic).

## Next
`/fix-issue 569`
