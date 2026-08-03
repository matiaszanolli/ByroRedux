# SCR-D6-NEW5-02: FragmentExecutionQueue's WaitForActors3DLoaded continuation has no retry cap or eviction path

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2288
**Source audit**: `docs/audits/AUDIT_SCRIPTING_2026-08-03.md`
**Severity**: MEDIUM
**Dimension**: Scripting Runtime Systems (Dimension 6)
**Location**: `crates/scripting/src/fragment.rs` (`FragmentExecutionQueue`, `actors_3d_loaded`, `fragment_continuation_system`)
**Labels**: medium, ecs, bug

## Body

(see GitHub issue for full body — description, evidence, impact, suggested fix, completeness checklist)

Summary: `fragment_continuation_system`'s `Actors3DLoaded` resume branch re-arms and re-queues an unresolved entry indefinitely — no max retry count, no elapsed-time ceiling, no eviction hook on `QuestStageState::reset` or actor despawn. Unlike the sibling `MAX_CASCADE=64` cascade guard in the same file, there is no analogous backstop here. Bounded in practice today (1 such effect in the real MQ101 corpus) but a genuine structural gap.

**Related**: same root-cause shape as #2287 (unbounded latent wait on live-entity resolution) — a shared "give up after N attempts/M seconds" helper could close both.
