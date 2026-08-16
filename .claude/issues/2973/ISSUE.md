# UI-D7-01: UiInputState.modifiers and cursor_position are only updated while the menu holds focus, so they are stale across every focus transition

**Issue**: #2973
**Severity**: LOW
**Dimension**: Engine Wiring & Input Routing
**Labels**: `low,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_UI_2026-08-16.md`
**Filed**: 2026-08-16 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_UI_2026-08-16.md` (Dimension 7 — Engine Wiring & Input Routing). Profile: both.

**Location**: `byroredux/src/main.rs`:494-509 · `byroredux/src/ui_input.rs`:17-21, 57-60, 89

## Description

`route_scaleform_window_event` early-returns (`if !ui_manager.has_input_focus() { return false; }`) **before** `dispatch_window_event`, and `dispatch_window_event` is the only writer of `App::ui_input_state`.

`WindowEvent::ModifiersChanged` and `WindowEvent::CursorMoved` therefore update the cached modifier set and cursor position **only while a menu already has focus**. Nothing resets either on focus loss.

## Evidence

```rust
// byroredux/src/ui_input.rs:89 — the only writer of state.modifiers …
WindowEvent::ModifiersChanged(modifiers) => state.modifiers = modifiers.state(),
```

```rust
// byroredux/src/main.rs:507 — … behind a gate that skips it when unfocused
if !ui_manager.has_input_focus() {
    return false;
}
```

(`route_scaleform_window_event` is still in `main.rs` post-#2731; its sole caller is `byroredux/src/app_events.rs`:281.)

## Impact

A modifier pressed or released while the menu is unfocused is never observed.

**Concrete failure**: menu focused with Ctrl held → focus released → user releases Ctrl → menu refocused → the next `a` keypress is translated as `UiTextControlCode::SelectAll` instead of typing a character, and stays wrong until winit emits another `ModifiersChanged`.

The cursor-position cache has the same shape: the first `MouseDown` after a focus grant uses the last position seen *during a previous focused period*.

LOW because Scaleform text fields are not yet wired to anything gameplay-facing — but this is the routing layer that work will sit on.

## Suggested Fix

Feed `ModifiersChanged` and `CursorMoved` into `ui_input_state` unconditionally (before the focus gate), **or** clear `UiInputState` on every focus transition so a stale modifier cannot survive one.

The unconditional feed is preferable: clearing loses the true current modifier state, which winit will not resend until it next changes.

## Related

- Distinct from the `release_world_input` contract, which the audit verified holds (report §4)

## Completeness Checks
- [ ] **SIBLING**: Every other cached-state field written only inside the focus gate checked for the same staleness
- [ ] **FOCUS-TRANSITION**: State is correct immediately after a focus grant, not only after the next winit event
- [ ] **NO-LEAK**: Moving the write before the focus gate does not route *actionable* input to an unfocused menu — only cache updates move
- [ ] **TESTS**: A regression test drives modifier-change-while-unfocused → refocus → keypress and asserts the character path, not `SelectAll`

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2973 --json state` when live state is needed.*
