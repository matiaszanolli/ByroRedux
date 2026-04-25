# FO4-D1-C2: BSSubIndexTriShape segment table wholesale-skipped via block_size — no dismemberment

**Labels:** bug, nif-parser, critical, legacy-compat
**State:** OPEN
**Source:** `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 1 C-2

## Finding

`crates/nif/src/blocks/mod.rs:226-236` dispatches `BSSubIndexTriShape` by running the base `BsTriShape::parse` and then skipping the remainder via `block_size`. Segmentation payload is never decoded: `num_primitives` u32, `num_segments` u32, per-segment `{start_index, num_primitives, num_sub_segments, per-subseg data}`, optional SSF filename, per-segment user-slot-flags.

## Impact

1. Dismemberment impossible — FO4 actor meshes need per-segment bone-slot flags.
2. Fragile against any refactor that stops plumbing `block_size`.

## Fix

Implement structured BsSubIndexTriShape struct with full segment + sub-segment + SSF + per-segment-flags decode. Replace the existing skip-validates test with a structured-decode test.

## Completeness
- SIBLING: Check every other block that uses `block_size` as a skip-to-end shortcut.
- TESTS: Structured-decode test built from a real FO4 actor mesh (`actors\deathclaw\deathclaw.nif`).
