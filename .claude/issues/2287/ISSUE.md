# SCR-D6-NEW5-01: ScenePackagePlayback's MoveTo action never completes once its actor entity is despawned

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2287
**Source audit**: `docs/audits/AUDIT_SCRIPTING_2026-08-03.md`
**Severity**: MEDIUM
**Dimension**: Scripting Runtime Systems (Dimension 6)
**Location**: `crates/scripting/src/package.rs` (`tick_command`'s `ScenePackageCommand::MoveTo` arm)
**Labels**: medium, ecs, bug

## Body

(see GitHub issue for full body — description, evidence, impact, suggested fix, completeness checklist)

Summary: `tick_command`'s `MoveTo` arm returns `false` ("not complete") whenever the actor's `Transform` lookup misses, with no fallback timeout. If the actor entity is despawned mid-travel (e.g. cell-streaming unload), the action is retried forever and any `SCEN` phase gated on it stalls permanently — no log line, no recovery. Reproduced directly (100 ticks at dt=1000.0, action never completed).

**Related**: same root-cause shape as #2288 (unbounded latent wait on live-entity resolution).
