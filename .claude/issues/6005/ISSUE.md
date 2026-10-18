# SCR-D5-2026-10-17-04: `resolve_wanted` restarts every actor sharing a behavior graph, not only the graphs that were waiting on the clip

Issue: #6005
Labels: medium, scripting, bug, animation

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-04: `resolve_wanted` restarts every actor sharing a behavior graph, not only the graphs that were waiting on the clip

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Cinematic
- **Location**: `byroredux/src/asset_provider/actor_graph.rs:322-335` (`.filter(|(_, graph)| std::ptr::eq(graph.graph(), &*job.loaded))` then `graph.start()` + `apply_playback_for`); `crates/scripting/src/animation_graph.rs:830-850` (`start`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The doc (module header, scripting.md "restarts the graph") says the pass restarts "the graphs that were waiting" on a clip. The filter selects *all* `ObjectAnimationGraph`s whose `LoadedGraph` is the job's shared graph (`ActorGraphCache` shares one `LoadedGraph` per `idle_key`, so every humanoid with the same behavior/folders). `start()` clears the schedule, re-schedules triggers from now and returns the playback of the *current* configuration with `start_time`/pose phase 0; `apply_playback_for` then replaces each actor's `AnimationPlayer`/`AnimationStack` clip at local time 0. Every first use of a new clip by any one actor (hundreds of lazily-resolved clips per FO4 graph, resolved during gameplay) therefore snaps all other same-graph actors' current animation back to its first frame (a mid-swing attack replays; a looping idle pops; a synced locomotion blend resets its phase) and re-bases their completion triggers.
- **Impact**: visible animation pops across a cell's NPCs on each clip first-use, until the graph's clip set is warm. Self-healing, visual only.
- **Related**: M47.2 3bb6806b5 (PlayAnimation reaches FO4 actors), #5861.
- **Suggested Fix**: restart only graphs that actually asked (record a per-graph `wanted` flag in `LazyClips::lookup`, or restart a graph only if its current `terms()` was empty before the clip was provided) and leave the rest running.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
