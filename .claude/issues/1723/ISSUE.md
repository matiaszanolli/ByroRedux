# FNV-D4-LOW-02: SCHR header doc comment lists 16 bytes but claims "20-byte"

**Issue**: #1723
**Severity**: LOW
**Labels**: low, import-pipeline, legacy-compat, documentation
**Dimension**: 4 — ESM Record Parser
**Location**: `crates/plugin/src/esm/records/script.rs:5-6`
**Source audit**: AUDIT_FNV_2026-06-23 (FNV-D4-LOW-02)

## Description
The module doc says `SCHR — 20-byte header: numRefs u32, compiled_size u32, var_count u32, script_type u16, flags u16`, but those five fields sum to 16 bytes. The actual 20-byte layout (correctly read at `script.rs:122-144`, and correctly described by the inline comment at lines 122-124) has a leading unused/pad `u32` before `numRefs` that the module doc omits.

## Impact
Documentation only — the parser reads the right 20 bytes. A maintainer reconciling the module doc against the code could misjudge field offsets.

## Suggested Fix
Add the leading `pad u32` to the module-doc field list so the byte count matches the "20-byte" claim.
