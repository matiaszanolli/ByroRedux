# NIF-09: Oblivion NiUnknown pool is 429 blocks across 32 already-dispatched types (investigate)

**Severity**: MEDIUM | **Dimension**: Block Parsing × Stream Position | **Game**: Oblivion | **Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-09

## Summary
Oblivion NiUnknown pool is 429 blocks — small in absolute terms but spread across 32 distinct types that ARE in the dispatch table. These are parse-failure cascades on long-tail Oblivion content. Needs investigation (bisect the failing block instances via `trace_block`) before triaging into per-type fixes.

## Evidence
`/tmp/audit/nif/obl_unk.out`: 32 distinct type_names in the NiUnknown bucket despite all being in dispatch.

## Location
`crates/nif/src/blocks/collision.rs`, `crates/nif/src/blocks/interpolator.rs`, `crates/nif/src/blocks/particle.rs` — long-tail surface.

## Suggested fix
Phase 1 (investigative): `cargo run -p byroredux-nif --example trace_block` on one instance of each failing type. Phase 2 (implementation): per-type fix based on diff findings. This issue tracks Phase 1 only — spawn child issues for the bisect results.

## Completeness Checks
- [ ] Investigation notes filed in `.claude/issues/<N>/INVESTIGATION.md`

Fix with: /fix-issue <number>
