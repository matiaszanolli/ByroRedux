# #3634 — REN-2026-08-30-D23-04: `UI_PIPELINE_DYNAMIC_STATES`' contract comment points at a call site, a field, and a const that #3426 removed

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3634 --json state`.

---

- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:840-849` and `:960-972` (`UI_PIPELINE_DYNAMIC_STATES`, `create_ui_pipeline`)
- **Status**: NEW
- **Description**: The `#663` contract block instructs the next editor that the overlay call
  site "lives in `vulkan/context/draw.rs` (post-`cmd_bind_pipeline(pipeline_ui)`)" and that
  a `_UI_PIPELINE_DYNAMIC_STATES_LEN` const assert "at the call site" fires when the list
  grows. After #3426 none of those three exist: the call site is
  `presentation.rs::record_overlay`, there is no `pipeline_ui` symbol anywhere in the tree,
  and the live compile-time guard is named `_UI_OVERLAY_DEFENSIVE_STATE_INVARIANT`. The
  guard itself is correct and in the right place — only the pointer to it is wrong, and the
  pointer is the entire mechanism by which the contract is discovered.
- **Evidence**:
  - `grep -rn "pipeline_ui" crates byroredux` → four hits, all inside comments
    (`presentation.rs:869` is a test asserting its *absence* from `geometry_pass.rs`;
    `pipeline.rs:844`, `:963`, `:965` are this stale block).
  - `grep -rn "_UI_PIPELINE_DYNAMIC_STATES_LEN" $(git ls-files '*.rs')` → one hit,
    `pipeline.rs:969`, inside the same comment.
  - `presentation.rs::record_overlay` contains the real
    `const _UI_OVERLAY_DEFENSIVE_STATE_INVARIANT: () = { assert!(UI_PIPELINE_DYNAMIC_STATES.len() == 2, …) }`
    plus the matching `cmd_set_viewport` / `cmd_set_scissor` pair.
  - Secondary, same block: `create_ui_pipeline`'s doc now says "`extent` is therefore the
    output extent", but the body is `let _ = extent;` — the parameter has been inert since
    viewport/scissor went dynamic (#578) and is now a misleading signal that the overlay
    pipeline is extent-bound (it is not, which is why a resize can rebuild it safely).
- **Impact**: Doc-only, but it is a doc whose stated job is to route a future editor to the
  one place that must change in lockstep. As written it routes them to a file that no longer
  contains the overlay draw.
- **Needs RenderDoc**: no
- **Suggested Fix**: Repoint the two comment blocks at
  `presentation.rs::record_overlay` / `_UI_OVERLAY_DEFENSIVE_STATE_INVARIANT`, and either
  drop the `extent` parameter from `create_ui_pipeline` or note in the doc that it is
  retained only for signature symmetry.

---
- **Cross-dimension corroboration**: Found independently four times — also as *D11-04*, *D8-02* and *D5-07*: the `#663` UI dynamic-state contract, `pipeline.rs`'s UI-pipeline contract, and the `UI_PIPELINE_DYNAMIC_STATES` comment all still point at the retired `pipeline_ui` field and its former geometry-pass call site.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D23-04

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
