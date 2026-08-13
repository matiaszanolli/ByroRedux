# REN-D20-01: egui-winit's raw-input event queue grows without bound while the overlay is hidden

- **Severity**: MEDIUM
- **Dimension**: 20 — Debug/Telemetry
- **Location**: `crates/debug-ui/src/lib.rs` (`DebugUiState::run`, `DebugUiState::on_window_event`); forwarding site `byroredux/src/main.rs` (`window_event`)
- **Status**: NEW
- **Description**: `DebugUiState::on_window_event` is invoked for every `winit::WindowEvent`, unconditionally — the binary's `window_event` calls it before any visibility check (the `egui_consumed` binding is computed first, then used only to decide whether to skip the camera layer). `egui_winit::State::on_window_event` appends translated events onto its private `egui_input.events` `Vec`, which is drained only by `take_egui_input`. `DebugUiState::run` is the sole caller of `take_egui_input`, and it short-circuits before reaching it: `if !self.visible && snapshot.interaction_prompt.is_none() { return PanelOutputs::default(); } let raw_input = self.egui_winit.take_egui_input(window);`. `visible` is `false` at boot, and `interaction_prompt` is `None` except when the player is aimed at an activatable reference. So in the default configuration — overlay closed, nothing under the crosshair — nothing ever drains the queue.
- **Evidence**: `byroredux/src/main.rs`'s `window_event` calls `state.on_window_event(win, &event).consumed` for all events with no `visible` gate. In `egui-winit-0.33.3`, `on_window_event` pushes into `self.egui_input.events` on the cursor-moved, mouse-wheel, pointer-button, key, touch and cut/copy/paste arms, and `take_egui_input` ends in `self.egui_input.take()` — the only drain. `take_egui_input` appears exactly once in `crates/debug-ui/src/lib.rs`, after the early return.
- **Impact**: One `egui::Event` retained per forwarded mouse-move / key / wheel / touch event, for the lifetime of the process, in host RAM. A fly-camera session produces `CursorMoved` continuously, so the queue grows monotonically for as long as the operator never opens the overlay — which is the expected steady state, since the overlay is opt-in behind F3. Second-order: the first F3 press hands egui the entire accumulated backlog in a single `RawInput`, so that frame replays every queued pointer/key/paste event at once (one-shot hitch plus nonsense interaction state). Recovered only by opening the overlay.
- **Related**: #2166 (per-system tracker armed on first overlay open — same "hidden overlay is the steady state" assumption); #2247 (`merge_egui_pending_output`, the mirror-image "skipped egui frame drops state" bug on the renderer side).
- **Suggested Fix**: On the short-circuit branch of `run`, still drain and discard: `let _ = self.egui_winit.take_egui_input(window);` before returning. That keeps egui-winit's viewport/modifier bookkeeping current so the first visible frame is correct. Gating the `on_window_event` forwarding on `visible` instead is the worse fix — egui would then miss modifier/focus state across the toggle boundary.

## Completeness Checks
- [ ] **SIBLING**: Check for the same "state accumulates only while a feature is inactive" shape in #2166 (per-system tracker) and #2247 (`merge_egui_pending_output`)
- [ ] **TESTS**: A regression test drives `DebugUiState::run` with `visible == false` across a burst of forwarded `WindowEvent`s and asserts the internal egui-winit event queue stays bounded

---
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-12b.md` (finding `REN-D20-01`)
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2831
