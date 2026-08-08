# SKY-D7-02: MaterialInfo default docs cite a BSLSP parser stub default that the Skyrim parser arm contradicts, at line numbers stale since the #1279 parser split

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2590
**Finding ID**: SKY-D7-02

**Severity**: LOW
**Dimension**: NIFAL Canonical Material Translation (Skyrim slice)
**Location**: `import/material/mod.rs:588-598,1029-1031`; `lighting_shader_pbr_tests.rs:205-209`
**Status**: NEW

## Description
Three sites anchor the neutral-default doc to specific `shader.rs` line numbers that, since the `#1279` three-arm parser split, land in unrelated code (the `starfield_tail` doc, not the stub). The docs also assert a single "parser stub default" exists when there are two disagreeing ones (`material_reference_stub` = `1.0/5.0`, `parse_skyrim` = `0.0/0.0`).

## Impact
A reader following these anchors lands in unrelated code and concludes the default contract is upheld — the documentation half of why SKY-D7-01 (this session) went unnoticed through #1241 → #2284.

## Related
SKY-D7-01 (this session).

## Suggested Fix
Anchor to the function name, not a line number; state plainly which parser arms honour the neutral default.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
