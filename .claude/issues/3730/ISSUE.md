# #3730 (INFO) — ESM-2026-08-30-D8-02: FileHeader::record_count is not a completeness gate

**Severity**: INFO · **Location**: `crates/plugin/src/esm/reader.rs::FileHeader::record_count`
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D8-02)

A recorded fact, not a defect — filed so `record_count` is never promoted
into a parse-completeness assertion. Its meaning differs by game, measured
with the full file walked to EOF and zero walker errors in every case:

- Oblivion/FO3/FNV/Skyrim: `HEDR.count == records + groups` exactly.
- Starfield: `HEDR.count == records + 1` (records only, groups excluded).
- FO4/FO76: neither formula, both short by exactly 80,196 against
  `records + groups` — an unexplained but identical delta across two
  unrelated files.

This also disposes of a "FO4/FO76 walk drops ~80k records" hypothesis: the
file is walked to EOF with zero errors and two independent walkers agree on
the true counts. It's `HEDR.count` semantics, not a walk defect.

## Fix implemented

Added the doc comment the issue's own Suggested Action asked for, directly
on the `FileHeader::record_count` field, carrying the full per-game
breakdown and an explicit "do not build a completeness assertion on this
field" warning.

**SIBLING** (issue's own checklist item): grepped every `record_count`
reference in the workspace. The only production consumer is exactly what
the issue states — a single `log::info!` in `parse_esm_with_load_order`
(`records/mod.rs:206-210`). Every other reference is a test assertion, a
doc comment, or a debug/example binary (`sf_smoke.rs`,
`esm_dim8_coverage.rs`, `dump_voli_subs.rs`) — none treats the field as a
completeness gate today. Nothing to change beyond the doc comment.

**TESTS**: n/a per the issue's own checklist (informational, doc-only fix).

Full workspace: `cargo test --no-fail-fast` 7056 passing, 0 failing
(unchanged — no source behavior touched).
