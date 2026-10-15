# TD3-2026-10-15-02: Papyrus animation-graph module docs and scripting.md still say actor graphs do not run, blends/conditions/layered blenders/modifiers are not evaluated

Labels: low,scripting,animation,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-02`).

- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `crates/scripting/src/fragment/anim_events.rs:20-22`; `crates/scripting/src/animation_graph.rs:25-27`; `byroredux/src/asset_provider/actor_graph.rs:14-15`; `docs/engine/scripting.md:1475, 1491-1495, 1522-1524`
- **Status**: NEW (the VM lead; confirmed and widened)
- **Age**: `3bb6806b5` (FO4 actor graphs run, 2026-10-14), `49529db7c` (whole generator tree, layered blends, modifiers, 2026-10-14), `64fea511f` (unregistered event on unload), `scripted_sound_system` (SoundPlay.*)
- **Effort**: small
- **Description**: `anim_events.rs` lists as "Not modelled" three things that now are: `OnAnimationEventUnregistered` (`unregister_unloaded`, ~20 lines below, sends it), "actors' graphs (none runs)" (FO4 actors run an `ObjectAnimationGraph`, same component the system drains), and `SoundPlay.*` events (scripting.md:1519 says they play). `animation_graph.rs:26` says "Not run: layered (non-parametric, bone-masked) blenders ...; modifiers", yet 49529db7c's message is "layered blenders with bone-weight masks ... the modifiers that act on the graph itself run" and `animation_modifiers.rs` exists. `actor_graph.rs:14` says "Not evaluated: blend weights and variables, transition blends and conditions, and the graph's modifiers" while the sibling module doc (lines 17-24) says weights, blends and conditions run. `scripting.md` carries the same pre-`49529db7c` text ("Transitions are instantaneous (no blends)", "blend weights ... are not kept", "only the motion leaf plays at a time", "`SetAnimationVariable*` have no graph variable to act on and do nothing") and, at :1523, "Not modelled: actors' graphs (none runs)", eighty lines above its own :1592 "The graph itself now runs on the actor".
- **Evidence**: `sed -n 20,22p crates/scripting/src/fragment/anim_events.rs`; `grep -n "none runs\|no blends\|motion leaf" docs/engine/scripting.md`; `git show 49529db7c --stat`.
- **Impact**: `/audit-scripting` is told scripting.md is authoritative; it would flag live behaviour as missing or miss that the docs under-state the scope of `object_graph_step_system`.
- **Related**: #5788 (same class, different module), 6f48ffbcd.
- **Suggested Fix**: rewrite the three "Not modelled/Not run/Not evaluated" sentences to what each commit left (e.g. `SoundPlayAt.<name>.<node>` and pose-modifiers: look-at, twist, foot IK, ragdoll, jiggle, per `animation_modifiers.rs:31`), and delete the superseded scripting.md sentences.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
