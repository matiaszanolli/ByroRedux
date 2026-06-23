# SCR-D1-02: No Skyrim-BE / Starfield-guards round-trip test on an untrusted parser

Filed as: matiaszanolli/ByroRedux#1728
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: MEDIUM
- **Dimension**: PEX Reader & Opcode Decode · Untrusted-Input: Yes (coverage of)
- **Location**: `crates/pex/src/lib.rs:115-273` (`build_sample` is FO4-LE only)
- **Labels**: medium, legacy-compat, bug

## Description
The only round-trip writer test exercises the FO4 little-endian dialect. The big-endian Skyrim path and the Starfield guards path have no round-trip regression. A field-order/endian regression in those arms passes CI silently (corpus smoke needs game data + manual run).

## Suggested Fix
Add a BE-Skyrim and a Starfield-with-guards round-trip to the writer test.
