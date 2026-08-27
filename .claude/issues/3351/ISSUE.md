# FNV-2026-08-26-D9-05

**Issue**: #3351
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 9 — AI Packages & Procedures
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `docs/engine/npc-spawn-ai-packages.md:222-224, 452-454, 473-476` · `byroredux/src/systems/wander.rs:6-7, 49-55` · `travel.rs:57-60` · `follow.rs:47-48` · `escort.rs:49-50` · `guard.rs:59-60` · `patrol.rs:33-34` · `crates/plugin/src/esm/records/misc/pack.rs:229`

**Premise verified**: two shipped changes contradict the prose.
1. `ambient_ai_package_system` (M42.9 / #2652, `ai_package.rs:509-604`,
   registered unconditionally at `boot.rs:900`) re-selects the winning
   package once per in-game minute and on `EvaluatePackageRequest`, and
   swaps the behavior component when the winner changes.
2. NAVM pathing landed 2026-08-23 and now routes six of the seven
   procedures (`navmesh_path.rs`, consumed by
   travel/wander/patrol/follow/escort/guard).

**Evidence**:
```
docs/engine/npc-spawn-ai-packages.md:473
- **Selection is spawn-time-only.** No package re-evaluation as game
  time advances — `CTDA` conditions *are* now evaluated (M42.2), but
  only once, against the game hour and world state at spawn.

docs/engine/npc-spawn-ai-packages.md:223   "(no pathing — open ground only)"
docs/engine/npc-spawn-ai-packages.md:452   "v0 scope: identical to Wander's … (no pathing, …"
crates/plugin/.../pack.rs:229              "walk-to-point locomotion (no pathing/NAVM)"
byroredux/src/systems/wander.rs:6          "walk in a straight line (no pathing/NAVM)"

byroredux/src/systems/wander.rs:49
//! - **No per-frame package re-evaluation.** `WanderBehavior` is attached
//!   once at spawn (`npc_spawn.rs`); an actor picked for Wander at spawn
//!   keeps wandering even if its package's schedule would no longer be
//!   active at the current game hour — the same limitation
//!   `SandboxBehavior` has today.
```
The trailing half of that wander.rs sentence is now flatly false; the
identical claim is repeated by reference in travel/follow/escort/guard/patrol.
The doc also still describes the pre-#2031 wiring (*"`active_package_is_wander`
+ `active_wander_location` gate a `WanderBehavior` insert"* at
`npc-spawn-ai-packages.md:211-213`) and still names `npc_spawn.rs`, which
was split into `npc_spawn/ai_package.rs`. Those selectors are dead code
already tracked as **#3042** — not re-reported, but the doc rot pointing at
them is separate and untracked.

**Impact**: not a runtime bug. It is the exact failure mode the project's
audit-hygiene memory warns about — roughly 1 in 6 audit findings historically
carried a stale premise, and this doc is the single most-cited source for
this subsystem's "documented v0 scope", which is what a reviewer is told to
check a finding against before filing. Leaving it wrong actively manufactures
stale premises.

**Fix sketch**: update `npc-spawn-ai-packages.md`'s "What's not covered"
bullet to describe the per-in-game-minute re-evaluation and its bounds
(one pass per actor per minute, behavior replaced only when the winning
PACK FormID changes); replace the "no pathing" claims with the single-tile
Phase 3/4 scope + the Phase 2 blocked note; retire the seven `attached once
at spawn` sentences.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
