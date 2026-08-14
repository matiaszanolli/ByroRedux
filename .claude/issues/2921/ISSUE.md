# REN-D8-02: should_force_history_reset's doc block is attached to advance_completed_frames

- **Issue**: [#2921](https://github.com/matiaszanolli/ByroRedux/issues/2921)
- **Finding ID**: `REN-D8-02`
- **Labels**: `low,renderer,documentation`
- **Source report**: [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](../../../docs/audits/AUDIT_RENDERER_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2921 --json state`.

---

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/svgf.rs` — the `///` run ending
  "…this extraction is the regression guard the audit asked for." immediately
  above `advance_completed_frames`; `should_force_history_reset` follows it
  undocumented
- **Status**: NEW
- **Description**: A blank line between the two doc blocks was lost, so the
  paragraph written for `should_force_history_reset` ("Should the temporal pass
  force a full history reset on this frame? … Pinned as a pure helper (#648 /
  RP-2)…") is concatenated onto the doc for `advance_completed_frames`.
  `advance_completed_frames`'s rustdoc therefore *opens* by describing a different
  function's contract, and `should_force_history_reset` — the helper `#648`
  extracted specifically so the reset policy would be discoverable and
  test-pinned — has no doc comment at all.
- **Evidence**: `svgf.rs`, contiguous `///` lines with no separator:
  ```rust
  /// extraction is the regression guard the audit asked for.
  /// Advance the per-FIF history age for whichever slots were dispatched, and
  /// clear their latches.
  ```
  followed by `pub(super) fn advance_completed_frames(…)`, then a bare
  `pub(super) fn should_force_history_reset(frames_since_creation: u32) -> bool`.
  The damage is load-bearing rather than cosmetic: `SvgfPipeline::upload_params`
  says "See `should_force_history_reset`'s doc for the cross-link", and
  `CompositePipeline`-side readers following that pointer land on a function that
  no longer carries it.
- **Impact**: Documentation only — no behavioural change. `cargo doc` renders the
  reset-policy rationale under the wrong symbol and leaves the #648 regression
  guard undocumented, which is exactly the discoverability that extraction bought.
- **Related**: #648 / RP-2, #2146, #917 / REN-D10-NEW-03.
- **Suggested Fix**: Insert the missing blank line so the first block reattaches
  to `should_force_history_reset` (move it below `advance_completed_frames`, or
  move the function above its doc).

---

## Completeness Checks
- [ ] **SIBLING**: The same doc table / anchor class is swept, not just the one row cited
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](docs/audits/AUDIT_RENDERER_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
