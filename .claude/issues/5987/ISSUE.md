# SCR-D2-2026-10-17-07: `hit_event_system` drops a hit the budget cannot start instead of deferring it

Issue: #5987
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D2-2026-10-17-07: `hit_event_system` drops a hit the budget cannot start instead of deferring it

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/hits.rs:205-214`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: With `starts == 0` the matching registration is skipped (`continue` at :210) while the `HitEvent` marker is drained at the end of the frame by `event_cleanup_system`. Every other budgeted path either defers the run (`run_papyrus_call`, `events::enqueue`) or leaves a *state-based* registration due (`OnUpdate`, distance polling). A hit is a one-shot marker, so this loses it; `RegisterForHitEvent` is itself one-shot, so the script stays armed for a hit that was already consumed by the engine. `docs/engine/scripting.md:909-918` describes deferral and OnUpdate coalescing only and does not list hits.
- **Evidence**: `if starts == 0 { continue; } starts -= 1; fired.push(...)` with the loop above reading `hits` (the marker snapshot).
- **Impact**: Under budget saturation (already a failing-script condition) FO4/Starfield hit handlers miss events silently.
- **Related**: #5764.
- **Suggested Fix**: When `starts == 0` build the `Effect::Interpret` and `budget::defer` it (removing the registration as for a fired one), as `reference_script_event_system` does for `OnHit`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
