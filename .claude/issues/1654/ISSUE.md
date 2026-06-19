# D3-1: SCPT SCHR flags read as u32 where field is u16 (always 0 on FO3)

**Issue**: #1654
**Source audit**: docs/audits/AUDIT_FO3_2026-06-18.md (HEAD `2aac5351`) — re-confirmed
still-open; first reported AUDIT_FO3_2026-06-16.md (D3-1), never previously filed.
**Severity**: LOW · **Labels**: low, import-pipeline, legacy-compat, bug
**Dimension**: 3 — ESM Records
**Location**: `crates/plugin/src/esm/records/script.rs:136-137`

## Description
`if sub.data.len() >= 20 { record.flags = r.u32().unwrap_or(0); }` — vanilla FO3/FNV/TES5
SCHR is 20 bytes with a u16 flag at offset 18 (only 2 bytes remain), so the strict u32
read fails and flags is always 0. The in-module test at `script.rs:259` only passes
because its fixture is a non-vanilla 22-byte SCHR — it masks the bug.

## Impact
Low — `ScriptRecord.flags` is parsed-but-unused.

## Suggested Fix
Read `r.u16()`; fix the comment; use a 20-byte vanilla fixture in the test.
