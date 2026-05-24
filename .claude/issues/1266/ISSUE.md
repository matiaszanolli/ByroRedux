# PERF-DIM7-14: Stale doc-comments claim dispatches_skipped is 'always zero' — #1195/#1196 shipped, comments didn't update

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/1266

## Source Audit
`docs/audits/AUDIT_PERFORMANCE_2026-05-24_DIM7.md` — Dimension 7 (TAA & GPU Skinning Cost)

## Severity
**LOW** — cosmetic / doc rot only. No behavior impact.

## Status
**NEW** at HEAD `8b5d77c1`

## Description
When #1194 (PERF-DIM7-INSTR) landed 2026-05-21 (commit `e5774b19`), the `dispatches_skipped` counter was pre-staged with **forward-looking** doc-comments asserting the value is "always zero" until a follow-up commit (#1195 / PERF-DIM7-01) lands the actual dispatch-dirty gate.

The follow-up commit (#1195 + #1196, commit `57c34c7f`) shipped 2026-05-22 — one day later — and the counter is now incremented at `crates/renderer/src/vulkan/context/draw.rs:1027` whenever an entity's bone pose is unchanged. The "always zero" comments did not update.

Three sites carry the stale forward-looking framing.

## Locations

1. **`crates/renderer/src/vulkan/skin_compute.rs:128-134`** — the primary stale block:
   ```
   /// `dispatches_skipped` (#1194 / PERF-DIM7-INSTR): gates the
   /// PERF-DIM7-01 dispatch-dirty-gate work. When that fix lands, every
   /// entity whose bone palette is unchanged this frame increments this
   /// counter instead of running the compute dispatch. Today the value
   /// is always zero; instrumentation pre-staged so the dirty-gate
   /// commit drops in cleanly. `dispatches_total - dispatches_skipped`
   /// gives the GPU dispatch count actually issued.
   ```

2. **`crates/core/src/ecs/resources.rs:422-426`** — sibling on `SkinCoverageStats.dispatches_skipped`:
   ```
   /// Entities whose compute dispatch was elided this frame because the
   /// bone palette hadn't changed since the previous dispatch (#1194 /
   /// PERF-DIM7-INSTR). Pre-#1194 always zero — PERF-DIM7-01 (dispatch
   /// dirty-gate, #1195) is the first consumer. `dispatches_total -
   /// dispatches_skipped` gives the GPU dispatch count actually issued.
   ```
   The "is the first consumer" framing reads as future tense; #1195 has shipped and is the (current) consumer.

3. **`crates/core/src/ecs/resources.rs:1229`** — test-side comment on `skin_coverage_dim7_instr_fields_default_to_zero_and_dont_break_green_bar`:
   ```
   /// Future PERF-DIM7-01 / -02 / -03 fixes will increment `dispatches_skipped`
   /// and read the GPU timer values; this test guards them against accidental
   /// removal from the struct.
   ```
   #1195 / #1196 / #1197 have all shipped — no longer "future."

## Impact
0 ms / 0 MB. Doc rot only — but a stale claim like "Today the value is always zero" actively misleads readers triaging the next perf audit (it cost the 2026-05-24 dim_7 sweep one extra grep + commit-log lookup to confirm the gate had actually landed).

## Suggested Fix
Rewrite all three doc-comments to past tense. ~9-12 LOC total. Suggested replacement for site 1:

```
/// `dispatches_skipped` (#1194 / PERF-DIM7-INSTR): incremented by the
/// dispatch-dirty gate (#1195 / PERF-DIM7-01) when an entity's bone
/// palette is unchanged from the previous frame — the compute dispatch
/// is suppressed and this counter bumps instead. `dispatches_total -
/// dispatches_skipped` gives the GPU dispatch count actually issued.
```

Sites 2 and 3 collapse to the same past-tense phrasing.

## Completeness Checks
- [ ] **UNSAFE**: N/A — doc comments only
- [ ] **SIBLING**: Verified — three sites carry the stale forward-looking framing; all listed above. Confirm no fourth site by re-grepping `grep -rnE "always zero|dirty-gate commit drops|pre-#1194 always" crates/ byroredux/` after the edit
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: `skin_coverage_dim7_instr_fields_default_to_zero_and_dont_break_green_bar` already pins the struct shape; the comment-only edit needs no test churn. Confirm test still passes after the comment update.

## Related
- #1194 (PERF-DIM7-INSTR, `e5774b19`) — landed the counter + forward-looking doc
- #1195 (PERF-DIM7-01, `57c34c7f`) — landed the consumer; should have updated the doc
- #1196 (PERF-DIM7-02, `57c34c7f`) — paired refit gate
- #1197 (PERF-DIM7-03, `946e95f9`) — descriptor-write skip
