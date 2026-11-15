# #6953: TD1-2026-11-15-07: Two never-swept tools carry a deep-nesting function and a 296-line print match

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6953
- **Labels**: low,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (nesting >5, fn >200)
- **Effort**: trivial (`draw_gpu`) + small (`print_response`)
- **Labels**: `tech-debt`
- **Status**: NEW
- **Sites**:
  - `tools/byro-launcher/src/settings_screen.rs:125-170` `draw_gpu` (47 code lines, brace-depth 9 as measured; about 8 constructs deep: frame closure, `match &state.gpu`, `Ok`, `match caps.verdict()`, `Ok(())`, `if !already`, `if let`, `horizontal` closure, `if clicked`).
  - `tools/byro-dbg/src/display.rs:5-` `print_response` (296 code lines, depth 5): one `match` over every `DebugResponse` variant with the printing inline.
- **Description**:
  - `draw_gpu` is the launcher's only function with this depth. The GPU-verdict logic (floor warning, recommendation, blocker colouring) is mixed with egui layout.
  - `print_response` is a transport enum printer that grows by one arm of ~20 lines per new debug response.
- **Suggested Fix**:
  - `draw_gpu`: extract `fn recommended_preset_row(ui, state, caps)` and `fn blocker_label(ui, blocker)`. The two `Err(blocker)` arms are already the same call.
  - `print_response`: one `fn print_<variant>(..)` per variant over 10 lines (`print_stats`, `print_entity_list`, ...), the `match` becoming a one-line dispatch table. `byro-dbg` is a pure CLI formatter, so the response-formatting tests are the check.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
