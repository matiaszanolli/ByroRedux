# #3722 — ESM-2026-08-30-D1-02: unknown top-level GRUP labels are skipped with no telemetry

**Severity**: LOW · **Location**: `crates/plugin/src/esm/records/mod.rs` (anonymous `_ => { reader.skip_group(&group); }` catch-all)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D1-02)

The catch-all's behavior was already correct (skip by declared size, no
per-record warn spam), but it neither logged nor recorded into
`index.skipped_unconsumed_groups` — a field that existed and was already
wired for exactly one label (`PDCL`). The routing-coverage signal (Skyrim SE
98.7%, FO4 98.4%, FO76 98.1%, Starfield 96.2% — 34/46/89/95 unrouted labels
respectively) had to be computed with an external walker instead of being
available at runtime.

## Fix implemented

The anonymous catch-all now pushes the label into
`index.skipped_unconsumed_groups`, deduplicated through a
`HashSet<[u8; 4]>` local to the parse loop (`seen_unconsumed_labels`) so the
cost is **O(distinct labels)**, per the issue's own suggested fix — not one
entry per group occurrence. This sits alongside the existing named arms
(`PDCL`, and the five `warned_*`-gated ones) rather than replacing any of
them; those stay consciously-named and warn-once as before.

**SIBLING** (issue's own checklist item): checked `dispatch_misc_stub.rs`'s
match arm — its own catch-all is `_ => unreachable!(...)` (exhaustive against
the caller's already-verified label set), and every one of its 31 listed
labels (including `SECH`/`AOPF`, the pair the issue's checklist named
specifically) already routes to typed `extract_records` + `EsmIndex` map
insertion. **No current gap found** — the `dispatch_misc_stub.rs:50` comment
the issue points to documents a *past* fix (#2636/SF-D4-05), not a live one;
confirmed via the existing regression test
`sech_and_aopf_groups_dispatch_into_typed_audio_maps` (`tests.rs`), already
passing before this change. Verified the premise before treating it as a
fix target, per this session's own hygiene practice — nothing to change here.

**TESTS** (issue's own checklist item): two new tests —
`genuinely_unrouted_label_is_recorded_in_skip_telemetry` (a synthetic `ZZZZ`
label reaches `skipped_unconsumed_groups`) and
`genuinely_unrouted_label_is_recorded_once_across_multiple_groups` (two
separate top-level `ZZZZ` groups still produce exactly one telemetry entry,
pinning the dedup).

Full workspace: `cargo test --no-fail-fast` 7054 passing, 0 failing (+2 new
tests).
