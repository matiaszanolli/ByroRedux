# #3954 — SCR-D8-2026-09-06-02: the router (`scene_trigger_actor_approach_system_inner`) and the gate (`actor_quest_trigger_is_in_sequence`) agree for same-quest scene waits but diverge on two inputs — cross-quest `GetStageDone` phase waits and centerless t...

- **Finding ID**: SCR-D8-2026-09-06-02
- **Labels**: low,scripting,quests,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3954

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Havok Idle / Cinematic Slice · **Untrusted-Input**: No · **Location**: `byroredux/src/systems/cinematic.rs:470-486, 561-573` vs `crates/scripting/src/trigger.rs:369-395, 420-440` · **Status**: NEW (the 08-30 pass dropped the general "they disagree" candidate after tracing the same-quest case; these two corners were not examined then)
- **Description**: re-traced: between scenes (same quest) router min == gate min; during a running scene router ⊆ gate — routed ⇒ allowed, confirming 08-30. Two asymmetries survive: (a) the router collects `awaited` from **every** running scene's current phase with no `scene.quest_form_id == param_1` filter, while the gate consults only the owning quest's scenes — a scene of quest Q₀ awaiting `GetStageDone(Q, S)` can route Q's actor toward a trigger Q's own between-scenes rule then refuses; (b) the router additionally requires a resolvable center (`TriggerVolume` or catalog entry), the gate's `next_ready` does not — a centerless lowest-stage trigger routes the horse to the next-lowest, which the gate refuses. Both stall the cart silently. Neither input was located in content (no ESM parse run); severity LOW as content-gated.
- **Suggested Fix**: derive both from one `crates/scripting` helper (`next_allowed_base_form_stages(world, quest)`), so they cannot drift; add a cross-quest-wait agreement test.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
