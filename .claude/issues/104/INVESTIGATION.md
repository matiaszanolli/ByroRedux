# Investigation: #104 — Oblivion v20.0.0.5 has no block_sizes

## Current State
- Header parser correctly skips block_sizes for v20.0.0.5 (empty Vec)
- Block size safety net (lib.rs:70-83) only fires when block_sizes is present
- All 15 planned Oblivion block types now have dedicated parsers (N23.3 complete)
- Havok collision blocks on Oblivion already return a hard error (mod.rs:207-215)

## Practical Impact
For Oblivion NIFs WITHOUT Havok collision blocks, parsing should work
correctly since all common block types have byte-perfect parsers.
For NIFs WITH Havok blocks, parsing fails — but this was already documented
as a known limitation (N23.6 deferred Oblivion Havok to M28).

## Fix
The issue title says "parse errors unrecoverable." The practical fix is:
1. Improve error messages to indicate this is an Oblivion-specific limitation
2. Add a compile-time-verified list of Oblivion-safe block types
3. On unknown block type for Oblivion, log the specific type name and position

This is not an architectural overhaul — it's error handling improvement.

## Scope
1-2 files: lib.rs (error handling), blocks/mod.rs (error message)
