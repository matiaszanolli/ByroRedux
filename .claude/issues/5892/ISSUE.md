# #5892: PAR-D2-2026-10-15-01: The graph runtime's trigger re-arm adds `period = duration / playback_speed` without checking that time advances

- **Labels**: high,bug,animation,scripting
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: The graph runtime's trigger re-arm adds `period = duration / playback_speed` without checking that time advances. A huge or tiny file-authored `playbackSpeed` loops `step()` forever on the main thread.

- **Severity**: HIGH (main-thread hang reachable from a mod file). Owner overlap: `/audit-scripting` Dim 5 owns HKX playback. Filed here because the decoder passes the field through unvalidated and this audit's brief asked about runtime loops.
- **Dimension**: Error Semantics
- **Location**:
  - `crates/scripting/src/animation_graph.rs:795-797` (`speed` is accepted when finite and > 0; `period = registered.duration / speed`)
  - `:948-951` (`rearm`: `at: fired.at + period`)
  - `:985-1011` (`step`: `while let Some(due) = self.pop_due(limit)`)
  - Called every frame from `object_graph_step_system` (`:1417-1427`)
  - Decode side: `crates/hkx/src/behavior.rs:1051` (`playback_speed: file.f32(...)`, unchecked)
- **Status**: NEW
- **Description**:
  - A looping clip's trigger is re-armed at `fired.at + period`.
  - If `period` is 0 (underflow, e.g. `duration / 3e38`) or below half an ULP of `fired.at`, the new entry has the same `at`. It is then due again within the same `limit`.
  - When the trigger's event changes no machine state, `send_event` returns `None` without touching `scheduled` (`:884-890`). Two cases do this: an event no transition answers, or an event id `-1`, which `transition_of` refuses at `:846`. The loop then never exits.
  - Each pass also pushes onto the raised-event list (`note_raised`), so memory grows while the frame hangs.
  - Even when `at` does advance, a period of 1e-6 costs dt/period ≈ 16 K `send_event` + `terms()` evaluations per frame.
  - Absorption also arrives with time alone. With `speed = 1e3` on a 1 s clip, `at + 1e-3 == at` once graph time passes about 2^14 s (4.5 h).
- **Evidence**: static. `pop_due` selects `at <= limit` (`:955-965`). `rearm` is unconditional for looping clips. `send_event`'s early `return None` leaves the rearmed entry in place.
- **Trigger Input**: an `hkbClipGenerator` with mode Looping, `playbackSpeed` ≥ ~1e30 (or a duration/speed ratio below the f32 ULP of the graph clock), and a trigger whose event no transition answers.
- **Impact**: the engine freezes on the first frame the trigger is due, whether in a placed object or an FO4 actor graph.
- **Suggested Fix**:
  - Clamp the playback speed at decode or at the runtime boundary to a sourced range, and reject non-positive or absurd speeds as "not looping".
  - In `rearm`, require `fired.at + period > fired.at`. Otherwise drop the re-arm and warn once.
  - Cap the iterations of the `step()` loop per frame, as `run_events` already does with `MAX_CHAIN` at `:924`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
