# #5448: PERF-D1-2026-10-08-02: `ambient_ai_package_system` probes the environment every frame, ahead of the minute gate that exists to make idle frames free

**Labels**: low,performance,gameplay,ai,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5448

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-08.md` — `PERF-D1-2026-10-08-02` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `std::env::var_os("BYRO_M42_DEBUG")` at `ai_package.rs:814` (before the `due.is_empty()` early-out) and `:892`; no other reader of the variable.

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:814-816` (per frame), `:892-897` (per due actor); the per-frame `last_evaluated` Vec at `:786-796`
- **Status**: NEW. Added by `00f580e09` (2026-10-08).
- **Description**: the system's own comment says nothing per-actor may be paid before the minute gate, so about 119 of every 120 frames fall straight through. The new `[m42-tick]` diagnostic sits before the `due.is_empty()` early-out. It calls `std::env::var_os("BYRO_M42_DEBUG")` on every frame that has at least one NPC with an `AmbientPackageRuntime`. That is a global environment read lock plus a linear `environ` scan. The variable is undocumented and nothing else reads it. The `[m42-eval]` probe at `:892` is per due actor per game minute, which is fine.
  The same function builds a fresh `Vec<(EntityId, Option<u16>)>` over every ambient NPC each frame (`:786`), an O(NPCs) allocation that a persistent scratch would remove. That part predates the baseline (#3353) and is noted here only because the new probe sits beside it.
- **Evidence**: `ai_package.rs:786-816`.
- **Impact**: one `getenv` scan per frame plus an O(NPCs) Vec fill and free per frame. Small. No quantitative guard exists.
- **Related**: #3353 (the minute-gate design), #2033 (the persistent-scratch pattern `sandbox` uses).
- **Suggested Fix**: read the variable once into a static (`OnceLock<bool>`) or drop the probes. Optionally move `last_evaluated` into a closure-held scratch like `make_animation_system`'s.

## Completeness Checks
- [ ] **SIBLING**: Other per-frame systems added by `00f580e09` (`eat_sleep`) checked for per-frame env reads
- [ ] **TESTS**: A regression test pins this specific fix
