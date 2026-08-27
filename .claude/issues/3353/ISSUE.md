# FNV-2026-08-26-D9-07

**Issue**: #3353
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 9 — AI Packages & Procedures
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `byroredux/src/npc_spawn/ai_package.rs:515-529`

**Premise verified**:
```rust
let runtimes: Vec<(EntityId, AmbientPackageRuntime)> = world
    .query::<AmbientPackageRuntime>()
    .map(|query| query.iter().map(|(actor, runtime)| (actor, runtime.clone())).collect())
    .unwrap_or_default();
…
for (actor, runtime) in runtimes {
    …
    if !explicitly_requested && runtime.last_evaluated_game_minute == Some(minute) { continue; }
```
`AmbientPackageRuntime::package_candidates` is a `Vec<u32>`, so `.clone()`
is a heap allocation per actor. The minute-gate — the entire point of
which is *"Schedule-only checks are bounded to one evaluation per in-game
minute per actor"* (module doc, `ai_package.rs:508-512`) — is checked
**after** the clone, so the allocation happens 60×/second regardless. The
loop also takes a fresh per-entity read lock via `world.get::<Dead>` and
`world.has::<EvaluatePackageRequest>` before the same gate.

**Evidence**: the system is registered unconditionally
(`boot.rs:900`, `Stage::Update`, exclusive) — unlike the seven locomotion
systems it is **not** behind an env gate, so this cost is paid in the
default configuration. FNV corpus: 3073 of 3816 NPC_ records carry PKID,
mean 2.4 entries (max 14), so each clone is a small `Vec<u32>` — the finding
is the churn shape (N allocations + 2N lock acquisitions × every frame),
the same class as #3269 but a different site and a different component.
At the default `time_scale = 30.0` (`components/game_time.rs:7`) an in-game
minute is 2 real seconds, so ~119 of every 120 frames' worth of that work
is thrown away.

**Fix sketch**: read `last_evaluated_game_minute` (a `Copy` field) under the
query without cloning, collect only the entities that actually need
re-evaluation, then clone `package_candidates` for that (usually empty) subset
— or keep a persistent scratch `Vec` in a `make_ambient_ai_package_system()`
factory, the same `#2033`/PERF-D1 pattern the six locomotion systems already use.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
