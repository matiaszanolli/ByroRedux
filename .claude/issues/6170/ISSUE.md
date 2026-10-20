# TD3-2026-10-19-10: `scripting.md` says no actor behavior graph runs; FO4 actors' graphs do (carried from 10-15 TD3-02, narrowed to one site)

Labels: low,scripting,documentation,doc-rot,tech-debt
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- Severity: LOW · Effort: trivial · Status: STILL LIVE (the other four sites - `anim_events.rs`, `animation_graph.rs`, `actor_graph.rs`, `scripting.md:1475-1524` - are fixed)
- Location: `docs/engine/scripting.md:1592-1593` ("What is **not** built is a *running* actor behavior graph (variables, blends and events of `MTBehavior` are not evaluated)")
- Evidence: same file `:1544` ("graphs run too (FO4 actors carry the same `ObjectAnimationGraph` component") and `:1621` ("an `ObjectAnimationGraph` runs it with `animated_root` the actor's skeleton"); `3bb6806b5`, `49529db7c`.
- Proposed fix: reword to what is unbuilt (root motion, pose modifiers: look-at/twist/foot IK/ragdoll/jiggle per `animation_modifiers.rs`).

Related: #5826 (closed; one site was missed).

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in the neighbouring docs / skills
- [ ] `.claude/commands/_audit-validate.sh` still passes after the edit

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
