# TD1-011: esm/records/mod.rs — parse_esm_with_load_order is a 949-line, 110-arm record-type dispatch

**GitHub Issue**: #2060
**Labels**: low,import-pipeline,legacy-compat,tech-debt,bug

**Severity**: LOW
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/plugin/src/esm/records/mod.rs:126-1074` (`parse_esm_with_load_order`)

## Description
Large-but-inherent dispatch table over a wire-format tag space, analogous to `blocks/mod.rs::parse_block_inner`. Idiomatic shape, still exceeds the 200-line guidance.

## Evidence
Confirmed live: `crates/plugin/src/esm/records/mod.rs` is 1094 LOC total; `pub fn parse_esm_with_load_order(data: &[u8], remap: Option<FormIdRemap>) -> Result<EsmIndex> {` starts at line 126, matching the report's claimed location — 949-line span, arm-per-line ratio loose per the report.

## Suggested Fix
Group into per-domain dispatch tables mirroring the `records/{actor,world,misc/*}.rs` split. Low urgency — arm-per-line ratio is loose (~8.6).

**Effort**: medium, low urgency

## Completeness Checks
- [ ] **SIBLING**: Same "large dispatch table over a fixed wire-format tag space" shape as `blocks/mod.rs::parse_block_inner` (TD1-012, deliberately not flagged for action) — this one is deemed actionable because arm-per-line ratio is looser
- [ ] **TESTS**: A regression test pins that the per-domain dispatch split produces identical `EsmIndex` output for the existing ESM fixture corpus
