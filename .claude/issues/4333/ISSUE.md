# #4333 SCR-D8-2026-09-14-02: the cinematic router re-implements `base_form_advance_is_eligible` inline; #3954's "one shared predicate" is shared only on paper

**Labels**: low,scripting,tech-debt,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: LOW
- **Dimension**: Havok Idle / Cinematic Slice
- **Untrusted-Input**: No
- **Location**: `byroredux/src/systems/cinematic.rs::scene_trigger_actor_approach_system_inner` (inline `target_stage >= current_stage` / `!get_stage_done` / `evaluate_condition_list` conjunction, ~545–585); `crates/scripting/src/trigger.rs::base_form_advance_is_eligible`
- **Status**: NEW — residual of #3954 (CLOSED). The 09-11 report marked the sharing fixed.
- **Description**: The predicate's doc says it was "extracted so the router's target selection and the gate's `next_ready` share one predicate". Only the gate calls it; the router hand-copies the same conjunction. `scene_phase_awaited_stage`, #3954's other shared predicate, *is* called by the router.
- **Evidence**: The orchestrator confirmed that every call site of `base_form_advance_is_eligible` is in `trigger.rs` (the gate plus tests), and that `cinematic.rs` contains the inline conjunction.
- **Impact**: Equivalent today (both sides traced). A future change to the gate's rule won't reach the router, which is exactly the drift #3954 was filed for and would stall the MQ101 cart with no diagnostic.
- **Related**: #3954, #3937
- **Suggested Fix**: Call `byroredux_scripting::base_form_advance_is_eligible(...)` in the router's between-scenes cap, or add a source-shape test beside the #3937 guard requiring the call.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
