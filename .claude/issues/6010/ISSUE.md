# SCR-D5-2026-10-17-09: After #5892 a one-shot trigger ping-pong still runs 1024 transitions every frame, indefinitely

Issue: #6010
Labels: low, scripting, bug, performance

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-09: After #5892 a one-shot trigger ping-pong still runs 1024 transitions every frame, indefinitely

- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `crates/scripting/src/animation_graph.rs:288, 1038-1057, 974-998`
- **Status**: NEW (residual of #5892)
- **Untrusted-Input**: Yes
- **Description**: `MAX_TRIGGERS_PER_STEP` bounds one `step`, and `skip_missed` only skips *periodic* triggers ("a one-shot stays due"). Two states whose zero-length / `relative_to_end` triggers raise the event that transitions to each other re-schedule at `now` on every transition, so each frame handles 1024 `send_event`s (three `terms()` evaluations, a trigger purge and a `schedule_from` apiece), forever, for one graph. A mod-supplied behavior graph (cell-placed object, Untrusted-Input) can hold the main thread at a constant multi-millisecond cost without ever tripping a guard; the runaway warning is only emitted from the periodic path. Cost per frame is unmeasured here (no engine run).
- **Impact**: sustained frame-time burn from one malformed placed-object graph.
- **Related**: #5892.
- **Suggested Fix**: when `MAX_TRIGGERS_PER_STEP` is hit with non-periodic triggers still due, drop the due one-shots (and report once), or disable the graph's trigger schedule after N saturated frames.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
