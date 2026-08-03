# SAVE-D1-12: Registry-completeness guard only covers NPC-spawn-stamped components — no coverage for script/system-inserted state

**Filed from**: `docs/audits/AUDIT_SAVE_2026-08-03.md`
**Labels**: medium, ecs, bug

**Severity**: MEDIUM
**Dimension**: Snapshot Completeness & Determinism
**Data-Loss Class**: none (process/tooling gap — but it is why SAVE-D1-08/09/10 went unnoticed)
**Location**: `byroredux/src/save_io.rs:1142-1177` (`npc_spawn_stamped_components_are_saved_or_intentionally_rederived`)

## Description
The guard's doc-comment scopes it to "Persistent gameplay-state components stamped on the placement root by `spawn_npc_entity`." Its `NPC_SPAWN_STAMPED` list is nine manually-maintained names, with no visibility into components a *system* inserts later during gameplay — precisely the class `TwoStateActivator`/`ScriptVariables` (#2291), `ActorControlState` (#2292), `Dead` (#2293) all belong to.

## Impact
This is the structural reason five consecutive clean save audits (2026-06-23 → 2026-07-25) did not anticipate SAVE-D1-08/09/10 — the completeness net doesn't extend past spawn-time components. Every future scripting/system feature that adds a mutable component repeats this risk unless a broader check exists.

## Suggested Fix
A source-scan guard (same manually-maintained-allowlist philosophy) that greps every `impl Component for` / `impl Resource for` across `crates/core/src/ecs/components/`, `crates/scripting/src/`, `crates/physics/src/`, etc., and requires each name to appear in `build_save_registry`'s registered set OR an explicit `NOT_SAVED_BY_DESIGN` allowlist with a one-line reason per entry (mirroring `REDERIVED_NOT_SAVED`).

Classification at filing time: NEW, CONFIRMED against current HEAD — guard function and `NPC_SPAWN_STAMPED` const verified present and unchanged in scope.
